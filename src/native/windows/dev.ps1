#requires -Version 7.4
param(
    [ValidateSet('Client','Daemon','Isolated')][string]$Mode = 'Client',
    [switch]$Release
)
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
$manifest = (Resolve-Path "$PSScriptRoot\..\..\muster\Cargo.toml").Path
$cargoProfile = if ($Release) { 'release' } else { 'dev' }
if ($Mode -eq 'Daemon') {
    cargo run --locked --manifest-path $manifest --profile $cargoProfile -p musterd --example local-dev
    return
}

# Use Cargo's actual artifacts, including a caller's target/profile overrides.
$artifacts = @(cargo build --locked --manifest-path $manifest --profile $cargoProfile -p muster-ffi -p muster-cli --message-format=json-render-diagnostics |
    ForEach-Object { ConvertFrom-Json $_ } | Where-Object reason -eq 'compiler-artifact')
$ffi = @($artifacts | Where-Object { $_.target.name -eq 'muster_ffi' } |
    ForEach-Object filenames | Where-Object { $_ -like '*\muster_ffi.dll' -or $_ -like '*/muster_ffi.dll' })
$cli = @($artifacts | Where-Object { $_.target.name -eq 'muster' -and $_.executable } | ForEach-Object executable)
if ($ffi.Count -ne 1 -or $cli.Count -ne 1) { throw 'Expected one Windows FFI DLL and CLI' }

$root = [IO.Directory]::CreateDirectory((Join-Path (Split-Path $cli[0]) 'windows-dev')).FullName
$run = Join-Path $root ([guid]::NewGuid().ToString('N'))
$config = if ($Release) { 'Release' } else { 'Debug' }
$fixture = $null
try {
    $isolatedBuild = ($Mode -eq 'Isolated').ToString().ToLowerInvariant()
    dotnet publish "$PSScriptRoot\Muster.csproj" -c $config -r win-x64 --no-self-contained -p:PublishSingleFile=false -p:MusterIsolatedDevelopment=$isolatedBuild -o $run
    Copy-Item -LiteralPath $ffi[0] -Destination $run
    $cliDir = [IO.Directory]::CreateDirectory("$run\cli")
    Copy-Item -LiteralPath $cli[0] -Destination $cliDir.FullName
    $start = [Diagnostics.ProcessStartInfo]::new("$run\Muster.exe")
    $start.UseShellExecute = $false
    if ($Mode -eq 'Isolated') {
        $example = @(cargo build --locked --manifest-path $manifest --profile $cargoProfile -p musterd --example local-dev --message-format=json-render-diagnostics |
            ForEach-Object { ConvertFrom-Json $_ } |
            Where-Object { $_.reason -eq 'compiler-artifact' -and $_.target.name -eq 'local-dev' -and $_.executable } |
            ForEach-Object executable)
        if ($example.Count -ne 1) { throw 'Expected one local-dev executable' }
        $fixtureStart = [Diagnostics.ProcessStartInfo]::new($example[0])
        $fixtureStart.UseShellExecute = $false
        $fixtureStart.CreateNoWindow = $true
        $fixtureStart.RedirectStandardInput = $true
        $fixtureStart.RedirectStandardOutput = $true
        $fixture = [Diagnostics.Process]::Start($fixtureStart)
        $socket = $fixture.StandardOutput.ReadLineAsync().WaitAsync([TimeSpan]::FromSeconds(30)).GetAwaiter().GetResult()
        if (!$socket) { throw 'The isolated daemon exited before announcing its endpoint' }
        $start.ArgumentList.Add('--connect-isolated')
        $start.ArgumentList.Add($socket)
        Write-Output "Isolated daemon socket: $socket"
        Write-Output "Development CLI: $($cli[0])"
    } else {
        $start.ArgumentList.Add('--connect-installed')
        $start.Environment['MUSTER_DIR'] = [IO.Directory]::CreateDirectory("$root\client-state").FullName
    }
    $app = [Diagnostics.Process]::Start($start)
    try {
        $app.WaitForExit()
        if ($app.ExitCode -eq 2 -and $Mode -eq 'Isolated') {
            throw 'Close the current Muster GUI before starting the isolated desktop. The installed daemon and vnet can stay running.'
        }
        if ($app.ExitCode) { throw "Development client exited with code $($app.ExitCode)" }
    } finally { $app.Dispose() }
} finally {
    if ($null -ne $fixture) {
        try {
            $fixture.StandardInput.Close()
            if (!$fixture.WaitForExit(15000)) {
                Write-Warning 'The isolated daemon is still shutting down'
            } elseif ($fixture.ExitCode) { Write-Warning "Isolated daemon exited with code $($fixture.ExitCode)" }
        } finally { $fixture.Dispose() }
    }
    if ([IO.Path]::GetDirectoryName($run) -eq $root -and [IO.Path]::GetFileName($run) -match '^[0-9a-f]{32}$' -and (Test-Path -LiteralPath $run)) {
        Remove-Item -LiteralPath $run -Recurse -Force
    }
}
