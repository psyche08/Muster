#requires -Version 7.4
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$PreviousFixture,
    [Parameter(Mandatory)][string]$NextFixture,
    [Parameter(Mandatory)][string]$PreviousCli,
    [Parameter(Mandatory)][string]$NextCli
)

$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
function Get-CliVersion($cli) {
    $version = & $cli --version
    if ($version -notmatch '^muster (\d+\.\d+\.\d+)$') { throw "Unexpected CLI version from $cli" }
    $Matches[1]
}

# Both generations use only this socket, including the setup writes.
function Invoke-FixtureRpc($cli, $request) {
    $reply = $request | ConvertTo-Json -Compress -Depth 8 |
        & $cli --socket $socket rpc --request - --timeout 8 | Out-String | ConvertFrom-Json
    if (!$reply.response) { throw "No fixture response for $($request.Keys)" }
    $reply.response
}

function Use-Fixture($exe, [scriptblock]$operation) {
    $start = [Diagnostics.ProcessStartInfo]::new($exe)
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardInput = $true
    $start.RedirectStandardOutput = $true
    $start.Environment['MUSTER_FIXTURE_NODE'] = $state
    foreach ($arg in '--exact', 'fixture_node_process', '--ignored', '--nocapture') {
        $start.ArgumentList.Add($arg)
    }
    $node = [Diagnostics.Process]::Start($start)
    try {
        do {
            $line = $node.StandardOutput.ReadLineAsync().WaitAsync([TimeSpan]::FromSeconds(20)).GetAwaiter().GetResult()
            if ($null -eq $line) { throw 'Fixture exited before announcing readiness' }
        } until ($line -eq 'fixture ready')
        & $operation
    } finally {
        try {
            $node.StandardInput.Close()
            if (!$node.WaitForExit(20000)) {
                $node.Kill()
                $node.WaitForExit()
                throw 'Fixture failed to shut down cleanly'
            }
            if ($node.ExitCode) { throw "Fixture exited with code $($node.ExitCode)" }
        } finally { $node.Dispose() }
    }
}

function Read-Generation($expectedVersion) {
    $info = (Invoke-FixtureRpc $NextCli @{device_info=@{}}).device_info
    $olderReader = (Invoke-FixtureRpc $PreviousCli @{device_info=@{}}).device_info
    if (!$info.device_id -or !$info.fingerprint -or !$info.commit -or $info.state.vnet_up -ne $false -or
        $info.version -cne $expectedVersion -or $olderReader.version -cne $expectedVersion -or
        $olderReader.device_id -cne $info.device_id -or $olderReader.commit -cne $info.commit) {
        throw "Unexpected device, version or vnet state while reading $expectedVersion"
    }
    $presence = (Invoke-FixtureRpc $NextCli @{device_presence=@{}}).presence_info
    if ([Convert]::ToBase64String([byte[]]$presence.public_key) -cne $expectedCandidate -or
        $presence.enrolled -ne $false) {
        throw "Expected the original, unenrolled Presence candidate in $expectedVersion"
    }
    $net = (Invoke-FixtureRpc $NextCli @{network_status=@{}}).network_status
    $members = @($net.members)
    if (!$net.has_network -or !$net.network_id -or $net.name -cne 'Upgrade test network' -or
        $members.Count -ne 1 -or !$members[0].is_self -or $members[0].device_id -cne $info.device_id -or
        $members[0].fingerprint -cne $info.fingerprint -or $members[0].role -cne 'admin' -or !$members[0].vip) {
        throw "Unexpected synthetic roster in $expectedVersion"
    }
    [pscustomobject]@{ Info=$info; Network=$net; Member=$members[0] }
}

function Compare-Fields($old, $new, [string[]]$names) {
    foreach ($name in $names) {
        if ($old.$name -cne $new.$name) { throw "Field changed across restart: $name" }
    }
}

$PreviousFixture = (Get-Item -LiteralPath $PreviousFixture).FullName
$NextFixture = (Get-Item -LiteralPath $NextFixture).FullName
$PreviousCli = (Get-Item -LiteralPath $PreviousCli).FullName
$NextCli = (Get-Item -LiteralPath $NextCli).FullName
$previousVersion = Get-CliVersion $PreviousCli
$nextVersion = Get-CliVersion $NextCli
if ($previousVersion -eq $nextVersion) { throw 'Supply CLIs from two different releases' }
$state = [IO.Directory]::CreateTempSubdirectory('muster-state-upgrade-').FullName
try {
    $socket = Join-Path $state 'node\musterd.sock'
    [IO.File]::WriteAllBytes((Join-Path $state 'fixture.key'), [Security.Cryptography.RandomNumberGenerator]::GetBytes(32))
    $key = [Security.Cryptography.ECDsa]::Create([Security.Cryptography.ECCurve+NamedCurves]::nistP256)
    try {
        $point = $key.ExportParameters($false).Q
        $expectedCandidate = [Convert]::ToBase64String([byte[]](@(4) + $point.X + $point.Y))
    } finally { $key.Dispose() }
    $old = Use-Fixture $PreviousFixture {
        $device = (Invoke-FixtureRpc $PreviousCli @{device_info=@{}}).device_info
        if ($device.version -cne $previousVersion -or !$device.device_id) { throw 'Wrong initial fixture' }
        $null = Invoke-FixtureRpc $PreviousCli @{device_set_relay=@{relay='127.0.0.1:49837'}}
        $null = Invoke-FixtureRpc $PreviousCli @{network_create=@{name='Upgrade test network'}}
        $null = Invoke-FixtureRpc $PreviousCli @{network_rename=@{device_id=$device.device_id; name='Upgrade fixture'}}
        # Seed an existing member's pending candidate without granting enrollment.
        $null = & $PreviousCli --socket $socket presence --register $expectedCandidate
        Read-Generation $previousVersion
    }
    $new = Use-Fixture $NextFixture { Read-Generation $nextVersion }
    if ($old.Info.name -cne 'Upgrade fixture' -or $old.Info.relay -cne '127.0.0.1:49837') {
        throw 'Initial node did not apply the fixture configuration'
    }
    Compare-Fields $old.Info $new.Info @('name', 'device_id', 'fingerprint', 'relay')
    Compare-Fields $old.Network $new.Network @('name', 'network_id', 'self_id', 'vnet_cidr')
    Compare-Fields $old.Member $new.Member @('name', 'device_id', 'fingerprint', 'vip', 'role')
    if ((($old.Member.scopes | Sort-Object) -join ',') -cne (($new.Member.scopes | Sort-Object) -join ',')) {
        throw 'Member scopes changed across restart'
    }
    "Passed $previousVersion ($($old.Info.commit)) -> $nextVersion ($($new.Info.commit)): both CLIs, identity, roster and pending Presence candidate"
} finally {
    $tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\')
    if ([IO.Path]::GetDirectoryName($state) -eq $tempRoot -and [IO.Path]::GetFileName($state).StartsWith('muster-state-upgrade-')) {
        Remove-Item -LiteralPath $state -Recurse -Force
    }
}
