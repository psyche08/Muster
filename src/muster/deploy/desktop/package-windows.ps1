param(
  [Parameter(Mandatory=$true)][string]$AppDirectory,
  [Parameter(Mandatory=$true)][string]$Version,
  [Parameter(Mandatory=$true)][string]$OutputDirectory,
  [ValidateSet('win-x64','win-arm64')][string]$Runtime = 'win-x64'
)
$ErrorActionPreference = 'Stop'
# Only the new muster-ffi client can be packaged; the ABI 1.86 preview is retired.
if (!(Test-Path "$AppDirectory/muster_ffi.dll") -or !(Test-Path "$AppDirectory/Muster.exe")) { throw 'New Muster client and muster_ffi.dll required' }
if (!$env:MUSTER_RELEASE_KEY -or !$env:MUSTER_SIGN_TEMPLATE) { throw 'Feed signing key and Windows signing template required' }
if ((vpk --help | Out-String) -notmatch '1\.2\.158') { throw 'Velopack 1.2.158 required' }
vpk pack --packId Muster --packTitle Muster --packVersion $Version --packDir $AppDirectory --mainExe Muster.exe --runtime $Runtime --channel $Runtime --delta None --outputDir $OutputDirectory --signTemplate $env:MUSTER_SIGN_TEMPLATE
if ($LASTEXITCODE -ne 0) { throw 'Velopack packaging failed' }
python "$PSScriptRoot/sign-feed.py" "$OutputDirectory/releases.$Runtime.json" --key $env:MUSTER_RELEASE_KEY
if ($LASTEXITCODE -ne 0) { throw 'Feed signing failed' }
