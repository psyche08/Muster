#requires -Version 7.4
param(
    [Parameter(Mandatory)][version]$Version,
    [Parameter(Mandatory)][string]$OutputDirectory,
    [string]$WixBin = $env:WIX_BIN,
    [string]$WintunArchive = $env:WINTUN_ARCHIVE
)
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
$root = (Resolve-Path "$PSScriptRoot\..\..\..\..").Path
$project = "$root\src\native\windows"
$out = [IO.Directory]::CreateDirectory($OutputDirectory).FullName
$name = "Muster-$Version-win-x64.msi"
if (Test-Path -LiteralPath "$out\$name") { throw "Output already exists: $out\$name" }
if ((rustc -vV) -notcontains 'host: x86_64-pc-windows-msvc') { throw 'Use the x64 MSVC Rust toolchain' }
if (!$WixBin) { $WixBin = Split-Path (Get-Command candle.exe).Source }
$work = [IO.Directory]::CreateTempSubdirectory('muster-msi-').FullName
$stage = [IO.Directory]::CreateDirectory("$work\payload").FullName
$manifest = "$root\src\muster\Cargo.toml"
$previousVersion = $env:MUSTER_MSI_VERSION
Push-Location $root
try {
    $env:MUSTER_MSI_VERSION = $Version.ToString()
    $daemon = @(cargo build --manifest-path $manifest --release --locked --message-format=json-render-diagnostics -p musterd -p muster-cli -p muster-ffi -p muster-browser -p muster-vnet |
        ForEach-Object { ConvertFrom-Json $_ } |
        Where-Object { $_.reason -eq 'compiler-artifact' -and $_.target.name -eq 'musterd' -and $_.executable })
    if ($daemon.Count -ne 1) { throw 'Missing daemon build artifact' }
    $bin = Split-Path $daemon[0].executable
    dotnet publish "$project\Muster.csproj" -c Release -r win-x64 "-p:Version=$Version" -o "$work\gui"
    Copy-Item -LiteralPath "$work\gui\Muster.exe" -Destination $stage
    foreach ($file in 'muster-tun.exe','musterd.exe','muster-vnet.exe','muster_ffi.dll','muster-browser-host.exe') {
        Copy-Item -LiteralPath "$bin\$file" -Destination $stage
    }
    $cli = [IO.Directory]::CreateDirectory("$stage\cli")
    Copy-Item -LiteralPath "$bin\muster.exe" -Destination $cli.FullName
    if (!$WintunArchive) {
        $WintunArchive = "$work\wintun.zip"
        Invoke-WebRequest 'https://www.wintun.net/builds/wintun-0.14.1.zip' -OutFile $WintunArchive
    }
    if ((Get-FileHash -LiteralPath $WintunArchive).Hash -ne '07c256185d6ee3652e09fa55c0b673e2624b565e02c4b9091c79ca7d2f24ef51') {
        throw 'Wintun archive hash mismatch'
    }
    Expand-Archive -LiteralPath $WintunArchive -DestinationPath "$work\driver"
    Copy-Item -LiteralPath "$work\driver\wintun\bin\amd64\wintun.dll" -Destination $stage
    Copy-Item -LiteralPath "$work\driver\wintun\LICENSE.txt" -Destination "$stage\Wintun-LICENSE.txt"
    Copy-Item -LiteralPath "$root\LICENSE" -Destination "$stage\LICENSE.txt"
    python -X utf8 "$PSScriptRoot\runtime-notices.py" "$project\obj\project.assets.json" "$stage\Runtime-Notices.txt"
    Copy-Item -LiteralPath "$PSScriptRoot\dev.linsheng.muster.browser.json" -Destination $stage
    $extension = [IO.Directory]::CreateDirectory("$stage\AgentHaloBrowser")
    foreach ($file in 'manifest.json','sw.js','jumper.js') {
        Copy-Item -LiteralPath "$root\src\browser\agenthalo-extension\$file" -Destination $extension.FullName
    }
    foreach ($binary in Get-ChildItem -LiteralPath $stage -Recurse -File | Where-Object Extension -in '.exe','.dll') {
        $reader = [IO.BinaryReader]::new([IO.File]::OpenRead($binary.FullName))
        try {
            $reader.BaseStream.Position = 0x3c
            $reader.BaseStream.Position = $reader.ReadInt32()
            if ($reader.ReadUInt32() -ne 0x4550 -or $reader.ReadUInt16() -ne 0x8664) {
                throw "Expected an x64 PE image: $($binary.Name)"
            }
        } finally { $reader.Dispose() }
    }
    & "$WixBin\candle.exe" -nologo -arch x64 "-dVersion=$Version" -dNativeWix=1 -out "$work\Muster.wixobj" "$PSScriptRoot\Muster.wxs"
    & "$WixBin\light.exe" -nologo -b $stage -out "$work\$name" "$work\Muster.wixobj"
    $image = "$work\verify"
    $extract = Start-Process msiexec.exe -WindowStyle Hidden -ArgumentList "/a `"$work\$name`" /qn TARGETDIR=`"$image`"" -Wait -PassThru
    if ($extract.ExitCode) { throw "Administrative extraction failed ($($extract.ExitCode))" }
    $installed = @(Get-ChildItem -LiteralPath $image -Recurse -Directory -Filter Muster)
    if ($installed.Count -ne 1) { throw 'Missing or ambiguous Muster installation directory' }
    foreach ($file in Get-ChildItem -LiteralPath $stage -File -Recurse) {
        $relative = [IO.Path]::GetRelativePath($stage, $file.FullName)
        if ((Get-FileHash -LiteralPath (Join-Path $installed[0].FullName $relative)).Hash -ne (Get-FileHash -LiteralPath $file.FullName).Hash) {
            throw "MSI payload mismatch: $relative"
        }
    }
    Copy-Item -LiteralPath "$work\$name" -Destination $out
    Get-FileHash -LiteralPath "$out\$name"
} finally {
    Pop-Location
    $env:MUSTER_MSI_VERSION = $previousVersion
    $tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\')
    if ([IO.Path]::GetDirectoryName($work) -eq $tempRoot -and [IO.Path]::GetFileName($work).StartsWith('muster-msi-')) {
        Remove-Item -LiteralPath $work -Recurse -Force
    }
}
