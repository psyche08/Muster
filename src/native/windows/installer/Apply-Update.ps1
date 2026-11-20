#requires -Version 5.1
[CmdletBinding()]
param([string]$Destination = (Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'Programs\Muster'))

$ErrorActionPreference = 'Stop'
$release = Get-Content -Raw -LiteralPath "$PSScriptRoot\release.json" | ConvertFrom-Json
if ($release.version -notmatch '^\d+\.\d+\.\d+$') { throw 'Invalid package version' }
$hashes = @{}
foreach ($entry in $release.files.psobject.Properties) {
    if ($entry.Name -notmatch '^[a-zA-Z0-9_-]+(?:/[a-zA-Z0-9_-]+)*\.[a-zA-Z0-9.]+$' -or
        $entry.Value -notmatch '^[a-fA-F0-9]{64}$' -or $hashes.ContainsKey($entry.Name)) {
        throw "Invalid package entry: $($entry.Name)"
    }
    $hashes.Add($entry.Name, $entry.Value)
}
function Test-Payload([string]$Directory) {
    $files = @(Get-ChildItem -LiteralPath $Directory -Recurse -File -Force)
    if ($files.Count -ne $hashes.Count) { throw 'Package file count mismatch' }
    foreach ($file in $files) {
        $relative = $file.FullName.Substring($Directory.Length + 1).Replace('\', '/')
        if ((Get-FileHash -LiteralPath $file.FullName).Hash -ne $hashes[$relative]) {
            throw "Package verification failed: $relative"
        }
    }
}

$root = [IO.Directory]::CreateDirectory($Destination).FullName
$versions = [IO.Directory]::CreateDirectory((Join-Path $root 'versions')).FullName
$payload = Join-Path $PSScriptRoot 'payload'
Test-Payload $payload
$active = Join-Path $versions $release.version
$run = [Guid]::NewGuid().ToString('N')
$stage = Join-Path $versions ".stage-$run"
$link = Join-Path $root 'Muster.lnk'
$pending = Join-Path $root "$run.lnk"
$backup = Join-Path $root "$run.previous.lnk"
$lock = [IO.File]::Open((Join-Path $root 'update.lock'), 'OpenOrCreate', 'ReadWrite', 'None')
try {
    if (Test-Path -LiteralPath $active) {
        Test-Payload $active
    } else {
        Copy-Item -LiteralPath $payload -Destination $stage -Recurse
        Test-Payload $stage
        [IO.Directory]::Move($stage, $active)
    }
    $shell = New-Object -ComObject WScript.Shell
    $shortcut = $shell.CreateShortcut($pending)
    $shortcut.TargetPath = Join-Path $active 'Muster.exe'
    $shortcut.WorkingDirectory = $active
    $shortcut.Save()
    $replacing = [IO.File]::Exists($link)
    if ($replacing) { [IO.File]::Replace($pending, $link, $backup) }
    else { [IO.File]::Move($pending, $link) }
    try {
        # Chrome checks the 32-bit view first. This also works on x64 Windows.
        $hive = [Microsoft.Win32.RegistryKey]::OpenBaseKey('CurrentUser', 'Registry32')
        try {
            $hostKey = $hive.CreateSubKey('Software\Google\Chrome\NativeMessagingHosts\dev.linsheng.muster.browser')
            try { $hostKey.SetValue('', (Join-Path $active 'dev.linsheng.muster.browser.json'), 'String') }
            finally { $hostKey.Dispose() }
        } finally { $hive.Dispose() }
    } catch {
        if ($replacing) { [IO.File]::Replace($backup, $link, $pending) }
        else { [IO.File]::Delete($link) }
        throw
    }
    Write-Output $link
} finally {
    foreach ($file in @($pending, $backup)) {
        if ([IO.File]::Exists($file)) { [IO.File]::Delete($file) }
    }
    if ([IO.Directory]::Exists($stage) -and [IO.Path]::GetDirectoryName($stage) -eq $versions -and
        [IO.Path]::GetFileName($stage) -eq ".stage-$run") {
        Remove-Item -LiteralPath $stage -Recurse -Force
    }
    $lock.Dispose()
}
