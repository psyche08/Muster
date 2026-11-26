#requires -Version 5.1
[CmdletBinding()]
param([string]$Destination = (Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'Programs\Muster'),
      [string]$MachineArchive)

$ErrorActionPreference = 'Stop'
# The packager pins the exact release.json bytes in this entry point.
$manifestHash = '@MUSTER_RELEASE_SHA256@'

function Install-MachineRelease {
    param([string]$pipeName, [int]$manifestLength, [string]$expectedHash, [string]$nodeSid, [string]$archivePath)
    $ErrorActionPreference = 'Stop'
    $env:PSModulePath = $PSHOME + '\Modules'
    $system = [Security.Principal.WindowsBuiltInRole]::Administrator
    $principal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
    if (!$principal.IsInRole($system) -or ![Environment]::Is64BitProcess) {
        throw 'The machine update requires elevated x64 Windows PowerShell.'
    }
    $sidType = [Security.Principal.SecurityIdentifier]
    $trustedOwners = @('S-1-5-18', 'S-1-5-32-544',
        'S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464')
    $writeMask = [Security.AccessControl.FileSystemRights]'Write, Delete, DeleteSubdirectoriesAndFiles, ChangePermissions, TakeOwnership'
    function Assert-Protected($item) {
        if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Unexpected reparse point: $($item.FullName)" }
        $acl = $item.GetAccessControl()
        $descriptor = [Security.AccessControl.RawSecurityDescriptor]::new($acl.GetSecurityDescriptorBinaryForm(), 0)
        if ($null -eq $descriptor.DiscretionaryAcl) { throw "Unrestricted release path: $($item.FullName)" }
        if ($acl.GetOwner($sidType).Value -notin $trustedOwners) { throw "Unexpected owner: $($item.FullName)" }
        foreach ($rule in $acl.GetAccessRules($true, $true, $sidType)) {
            # CREATOR OWNER may grant rights to the administrator creating a child.
            if ($rule.IdentityReference.Value -eq 'S-1-3-0' -and
                ($rule.PropagationFlags -band [Security.AccessControl.PropagationFlags]::InheritOnly)) { continue }
            # Include GENERIC_ALL and GENERIC_WRITE in inheritable ACEs.
            if ($rule.AccessControlType -eq 'Allow' -and
                (([long]$rule.FileSystemRights -band ([long]$writeMask -bor 0x50000000)) -ne 0) -and
                $rule.IdentityReference.Value -notin $trustedOwners) {
                throw "Unprotected release path: $($item.FullName)"
            }
        }
    }
    function Register-NetworkHelper([string]$directory) {
        if ($nodeSid -notmatch '\AS-1-(?:5-21|12-1)-(?:[0-9]+-){3}[0-9]+\z') { throw 'Expected the original Windows user SID' }
        foreach ($file in 'muster-tun.exe','wintun.dll','musterd.exe') {
            if (!$hashes.ContainsKey($file)) { throw "Missing network service component $file" }
        }
        $sc = Join-Path ([Environment]::SystemDirectory) 'sc.exe'
        $image = '"' + (Join-Path $directory 'muster-tun.exe') + '" --service ' + $nodeSid
        $serviceKey = 'HKLM:\SYSTEM\CurrentControlSet\Services\MusterNetwork'
        $existing = Test-Path -LiteralPath $serviceKey
        if ($existing) {
            $old = Get-ItemProperty -LiteralPath $serviceKey
            $expected = '^"' + [regex]::Escape($root) + '\\[0-9]+\.[0-9]+\.[0-9]+\\muster-tun\.exe" --service ' + [regex]::Escape($nodeSid) + '$'
            if ($old.ImagePath -notmatch $expected -or $old.ObjectName -ne 'LocalSystem') {
                throw 'Existing MusterNetwork registration belongs to a different account or installation'
            }
        } else {
            New-Service -Name MusterNetwork -DisplayName 'Muster Network' -BinaryPathName $image -StartupType Manual -ErrorAction Stop | Out-Null
        }
        # The enrolled account can start and query the helper. Configuration requires an administrator.
        & $sc sdset MusterNetwork "D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;LCRP;;;$nodeSid)" | Out-Null
        if ($LASTEXITCODE -ne 0) {
            $failure = $LASTEXITCODE
            if (!$existing) {
                & $sc delete MusterNetwork | Out-Null
                if ($LASTEXITCODE -ne 0) { throw "Network service permissions failed ($failure) and cleanup failed ($LASTEXITCODE)" }
            }
            throw "Network service permissions failed ($failure)"
        }
        if ($existing -and $old.ImagePath -cne $image) {
            $svc = Get-CimInstance -ClassName Win32_Service -Filter "Name='MusterNetwork'"
            $changed = Invoke-CimMethod -InputObject $svc -MethodName Change -Arguments @{PathName=$image; StartMode='Manual'}
            if ($changed.ReturnValue -ne 0) { throw "Network service update failed ($($changed.ReturnValue))" }
        }
    }
    function Register-StreamHost([string]$directory) {
        foreach ($file in 'muster-stream-host.exe','muster-stream-worker.exe','Sunshine/muster-stream-host.exe') {
            if (!$hashes.ContainsKey($file)) { throw "Missing stream host component $file" }
        }
        $name = 'MusterStream'
        $svcKey = "HKLM:\SYSTEM\CurrentControlSet\Services\$name"
        $account = 'NT AUTHORITY\LocalService'
        $image = '"' + (Join-Path $directory 'muster-stream-host.exe') + '" --service ' + $nodeSid
        $taskName = 'Muster Stream Worker'
        $worker = Join-Path $directory 'muster-stream-worker.exe'
        $scheduler = New-Object -ComObject Schedule.Service
        $scheduler.Connect()
        $folder = $scheduler.GetFolder('\')
        $oldTask = $null
        try { $oldTask = $folder.GetTask($taskName) }
        catch { if ($_.Exception.HResult -ne -2147024894) { throw } }
        $oldService = Get-Service $name -ErrorAction SilentlyContinue
        $expectedDirectory = '^' + [regex]::Escape($root) + '\\[0-9]+\.[0-9]+\.[0-9]+\\'
        if ($oldTask) {
            $old = $oldTask.Definition
            if ($old.Principal.UserId -ne $nodeSid -and
                ([Security.Principal.NTAccount]::new($old.Principal.UserId)).Translate($sidType).Value -ne $nodeSid) {
                throw 'Stream worker belongs to another account'
            }
            if ($old.Actions.Count -ne 1 -or $old.Actions.Item(1).Path -notmatch ($expectedDirectory + 'muster-stream-worker\.exe$') -or
                $old.Actions.Item(1).Arguments -ne '--console-worker' -or $old.Principal.RunLevel -ne 0 -or $old.Principal.LogonType -ne 3) {
                throw 'Unexpected existing stream worker registration'
            }
        }
        if ($oldService) {
            $previous = Get-ItemProperty -LiteralPath $svcKey
            $expectedImage = '^"' + $expectedDirectory.Substring(1) + 'muster-stream-host\.exe" --service ' + [regex]::Escape($nodeSid) + '$'
            if ($previous.ObjectName -ne $account -or $previous.ImagePath -notmatch $expectedImage) {
                throw 'Unexpected existing stream host service'
            }
        }
        $definition = $scheduler.NewTask(0)
        $definition.RegistrationInfo.Description = 'Muster physical console stream worker'
        $definition.Principal.UserId = $nodeSid
        $definition.Principal.LogonType = 3 # TASK_LOGON_INTERACTIVE_TOKEN
        $definition.Principal.RunLevel = 0
        $definition.Settings.ExecutionTimeLimit = 'PT0S'
        $definition.Settings.DisallowStartIfOnBatteries = $false
        $definition.Settings.StopIfGoingOnBatteries = $false
        $definition.Settings.MultipleInstances = 2 # TASK_INSTANCES_IGNORE_NEW
        # Task Scheduler also sees account logon/session events on hosts where
        # LocalService cannot query WTS user metadata. The worker independently
        # verifies its session, token and display before accepting any media.
        $logon = $definition.Triggers.Create(9)
        $logon.UserId = $nodeSid
        foreach ($sessionChange in 1,8) { # console connect, workstation unlock
            $trigger = $definition.Triggers.Create(11)
            $trigger.StateChange = $sessionChange
            $trigger.UserId = $nodeSid
        }
        $action = $definition.Actions.Create(0)
        $action.Path = $worker
        $action.Arguments = '--console-worker'
        $action.WorkingDirectory = $directory
        # Only SCM's service SID can request the fixed worker outside the installer.
        $sc = Join-Path ([Environment]::SystemDirectory) 'sc.exe'
        $running = $oldService -and $oldService.Status -eq 'Running'
        $registered = $false
        $oldTaskXml = if ($oldTask) { $oldTask.Xml }
        $oldTaskAcl = if ($oldTask) { $oldTask.GetSecurityDescriptor(4) }
        try {
            if ($oldService) {
                if ($running) { Stop-Service $name -ErrorAction Stop }
                if ($oldTask -and $oldTask.GetInstances(0).Count -gt 0) { $oldTask.Stop(0) }
                $svc = Get-CimInstance Win32_Service -Filter "Name='MusterStream'"
                $change = Invoke-CimMethod -InputObject $svc -MethodName Change -Arguments @{PathName=$image; StartMode='Automatic'}
                if ($change.ReturnValue -ne 0) { throw "Stream service update failed ($($change.ReturnValue))" }
            } else {
                # Use the SCM API via WMI to preserve exact quoting on Windows PowerShell 5.
                $create = Invoke-CimMethod -ClassName Win32_Service -MethodName Create -Arguments @{
                    Name=$name; DisplayName='Muster Stream Host'; PathName=$image; ServiceType=[byte]16;
                    ErrorControl=[byte]1; StartMode='Automatic'; StartName=$account
                }
                if ($create.ReturnValue -ne 0) { throw "Stream service creation failed ($($create.ReturnValue))" }
            }
            $registered = $true
            & $sc sidtype $name unrestricted | Out-Null
            if ($LASTEXITCODE -ne 0) { throw 'Stream service SID setup failed' }
            $serviceSid = ([Security.Principal.NTAccount]::new("NT SERVICE\$name")).Translate($sidType).Value
            $acl = "D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GRGX;;;$serviceSid)"
            [void]$folder.RegisterTaskDefinition($taskName, $definition, 22, $nodeSid, $null, 3, $acl)
            Start-Service $name -ErrorAction Stop
        } catch {
            if ($registered) {
                if ($oldTask) { [void]$folder.RegisterTask($taskName, $oldTaskXml, 22, $nodeSid, $null, 3, $oldTaskAcl) }
                elseif (@($folder.GetTasks(1) | Where-Object Name -eq $taskName).Count) { $folder.DeleteTask($taskName, 0) }
                if (!$oldService) { & $sc delete $name | Out-Null }
                else {
                    $restore = Invoke-CimMethod -InputObject $svc -MethodName Change -Arguments @{PathName=$previous.ImagePath}
                    if ($restore.ReturnValue -ne 0) { throw 'Stream service rollback failed' }
                }
            }
            if ($running) { Start-Service $name }
            throw
        }
    }
    $sha = [Security.Cryptography.SHA256]::Create()
    function Digest([byte[]]$bytes) { [BitConverter]::ToString($sha.ComputeHash($bytes)).Replace('-', '') }
    # The elevated reader never opens a file or follows a link in the extracted ZIP.
    # Anonymous impersonation prevents the serving process from using the elevated token.
    $archive = $null
    $reader = $null
    function Read-ReleaseEntry([string]$name, [int]$count) {
        if (!$archive) { return ,$reader.ReadBytes($count) }
        $entry = $archive.GetEntry($name)
        if (!$entry -or $entry.Length -ne $count) { throw "Archive length mismatch: $name" }
        $inputReader = [IO.BinaryReader]::new($entry.Open())
        try { return ,$inputReader.ReadBytes($count) } finally { $inputReader.Dispose() }
    }
    if ($archivePath) {
        Add-Type -AssemblyName System.IO.Compression.FileSystem
        $archive = [IO.Compression.ZipFile]::OpenRead($archivePath)
        $manifestEntry = $archive.GetEntry('release.json')
        if (!$manifestEntry -or $manifestEntry.Length -gt 4194304) { throw 'Invalid archive manifest' }
        $manifestLength = [int]$manifestEntry.Length
    } else {
        $pipe = [IO.Pipes.NamedPipeClientStream]::new('.', $pipeName, [IO.Pipes.PipeDirection]::In,
            [IO.Pipes.PipeOptions]::None, [Security.Principal.TokenImpersonationLevel]::Anonymous)
        $pipe.Connect(30000)
        $reader = [IO.BinaryReader]::new($pipe)
    }
    $data = Read-ReleaseEntry 'release.json' $manifestLength
    if ((Digest $data) -ne $expectedHash) { throw 'Release manifest hash mismatch' }
    $release = [Text.Encoding]::UTF8.GetString($data) | ConvertFrom-Json
    if ($release.version -notmatch '\A[0-9]+\.[0-9]+\.[0-9]+\z') { throw 'Invalid release version' }
    $hashes = @{}
    foreach ($entry in $release.files.psobject.Properties) {
        if ($entry.Name -notmatch '\A[a-zA-Z0-9_-]+(?:[./][a-zA-Z0-9_-]+)*\z' -or
            $entry.Value -notmatch '\A[0-9a-fA-F]{64}\z') { throw 'Invalid package entry' }
        $hashes.Add($entry.Name, $entry.Value)
    }
    if (!$hashes.ContainsKey('Muster.exe')) { throw 'Missing desktop executable' }
    $root = [Environment]::GetFolderPath('ProgramFiles')
    Assert-Protected ([IO.DirectoryInfo]::new($root))
    foreach ($component in 'Muster','versions') {
        $root = Join-Path $root $component
        Assert-Protected ([IO.Directory]::CreateDirectory($root))
    }
    $active = Join-Path $root $release.version
    function Test-Installed([IO.DirectoryInfo]$dir, [string]$base, [hashtable]$remaining) {
        Assert-Protected $dir
        foreach ($item in $dir.EnumerateFileSystemInfos()) {
            if ($item -is [IO.DirectoryInfo]) { Test-Installed $item $base $remaining; continue }
            Assert-Protected $item
            $name = $item.FullName.Substring($base.Length + 1).Replace('\','/')
            $stream = $item.OpenRead()
            try { $actual = [BitConverter]::ToString($sha.ComputeHash($stream)).Replace('-', '') }
            finally { $stream.Dispose() }
            if ($actual -ne $remaining[$name]) { throw "Installed file mismatch: $name" }
            $remaining.Remove($name)
        }
    }
    function Assert-Release([string]$path) {
        $remaining = $hashes.Clone()
        Test-Installed ([IO.DirectoryInfo]::new($path)) $path $remaining
        if ($remaining.Count) { throw 'Installed release is incomplete' }
    }

    # Concurrent machine updates can stage independently. Rename never overwrites a release.
    $registrationLock = $null
    $stage = Join-Path $root ('.stage-' + [Guid]::NewGuid().ToString('N'))
    try {
        Assert-Protected ([IO.Directory]::CreateDirectory($stage))
        foreach ($entry in $release.files.psobject.Properties) {
            $name = $entry.Name
            $size = [int]$release.sizes.$name
            $bytes = Read-ReleaseEntry ("payload/" + $entry.Name) $size
            if ($bytes.Length -ne $size -or (Digest $bytes) -ne $hashes[$name]) {
                throw "Package verification failed: $name"
            }
            $target = Join-Path $stage $name
            [void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($target))
            [IO.File]::WriteAllBytes($target, $bytes)
        }
        Assert-Release $stage
        try { [IO.Directory]::Move($stage, $active) }
        catch [IO.IOException] { if (![IO.Directory]::Exists($active)) { throw } }
        Assert-Release $active
        # Serialize changes to the global service/task pair across installers.
        $registrationLock = [IO.File]::Open((Join-Path $root 'service-update.lock'), 'OpenOrCreate', 'ReadWrite', 'None')
        $netFilter = "Name='MusterNetwork'"
        $priorNetwork = Get-CimInstance Win32_Service -Filter $netFilter
        Register-NetworkHelper $active
        try { Register-StreamHost $active }
        catch {
            # The running daemon accepts only its matching network helper.
            $network = Get-CimInstance Win32_Service -Filter $netFilter
            if (!$priorNetwork) { $rollback = Invoke-CimMethod -InputObject $network -MethodName Delete }
            elseif ($priorNetwork.PathName -ne $network.PathName) {
                $rollback = Invoke-CimMethod -InputObject $network -MethodName Change -Arguments @{PathName=$priorNetwork.PathName}
            } else { throw }
            if ($rollback.ReturnValue -ne 0) { throw "Network service rollback failed ($($rollback.ReturnValue))" }
            throw
        }
    } finally {
        if ($registrationLock) { $registrationLock.Dispose() }
        if ($reader) { $reader.Dispose() }
        if ($archive) { $archive.Dispose() }
        $sha.Dispose()
        if ([IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($stage)) -eq $root -and [IO.Directory]::Exists($stage)) {
            [IO.Directory]::Delete($stage, $true)
        }
    }
}

if (![Environment]::Is64BitProcess -or $PSVersionTable.PSEdition -ne 'Desktop') {
    $systemDirectory = [Environment]::SystemDirectory
    if (![Environment]::Is64BitProcess) { $systemDirectory = Join-Path ([Environment]::GetFolderPath('Windows')) 'Sysnative' }
    $forward = @{}
    if ($MachineArchive) { $forward.MachineArchive = $MachineArchive }
    & (Join-Path $systemDirectory 'WindowsPowerShell\v1.0\powershell.exe') -NoProfile -ExecutionPolicy RemoteSigned -File $PSCommandPath -Destination $Destination @forward
    exit $LASTEXITCODE
}
$caller = [Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())
if ($MachineArchive) {
    if (!$caller.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw 'Machine-only update requires an existing administrator session.'
    }
    # Upgrade an enrolled machine only. The per-user shortcut is activated
    # separately in the enrolled account after the new machine release passes.
    $old = Get-ItemProperty -LiteralPath 'HKLM:\SYSTEM\CurrentControlSet\Services\MusterNetwork'
    if ($old.ImagePath -notmatch ' --service (S-1-(?:5-21|12-1)-(?:[0-9]+-){3}[0-9]+)$') {
        throw 'No enrolled network service for a machine-only upgrade'
    }
    $enrolled = $Matches[1]
    Install-MachineRelease '' 0 $manifestHash $enrolled ([IO.Path]::GetFullPath($MachineArchive))
    return
}
if ($caller.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'Run Apply-Update.cmd from your ordinary user session. The machine installation requests UAC.'
}
$manifest = [IO.File]::ReadAllBytes((Join-Path $PSScriptRoot 'release.json'))
$sha = [Security.Cryptography.SHA256]::Create()
try { $digest = [BitConverter]::ToString($sha.ComputeHash($manifest)).Replace('-', '') }
finally { $sha.Dispose() }
if ($digest -ne $manifestHash) { throw 'Release manifest does not match this updater.' }
$release = [Text.Encoding]::UTF8.GetString($manifest) | ConvertFrom-Json
$active = Join-Path ([Environment]::GetFolderPath('ProgramFiles')) "Muster\versions\$($release.version)"

# Only the machine install runs under the UAC identity. The original user reads the package.
$pipeName = 'Muster-Update-' + [Guid]::NewGuid().ToString('N')
$acl = [IO.Pipes.PipeSecurity]::new()
foreach ($sid in @([Security.Principal.WindowsIdentity]::GetCurrent().User,
                   [Security.Principal.SecurityIdentifier]::new('S-1-5-32-544'))) {
    $acl.AddAccessRule([IO.Pipes.PipeAccessRule]::new($sid, 'FullControl', 'Allow'))
}
$pipe = [IO.Pipes.NamedPipeServerStream]::new($pipeName, [IO.Pipes.PipeDirection]::Out, 1,
    [IO.Pipes.PipeTransmissionMode]::Byte, [IO.Pipes.PipeOptions]::Asynchronous, 0, 65536, $acl)
$body = (Get-Item Function:\Install-MachineRelease).ScriptBlock.ToString()
$nodeSid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$command = "try { & { $body } '$pipeName' $($manifest.Length) '$manifestHash' '$nodeSid'; exit 0 } catch { [Console]::Error.WriteLine(" + '$_' + "); exit 1 }"
$encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($command))
$ps = Join-Path ([Environment]::SystemDirectory) 'WindowsPowerShell\v1.0\powershell.exe'
try {
    $connection = $pipe.WaitForConnectionAsync()
    $installed = Start-Process -FilePath $ps -WorkingDirectory ([Environment]::SystemDirectory) -Verb RunAs -WindowStyle Hidden -PassThru -ArgumentList @('-NoProfile','-NonInteractive','-EncodedCommand',$encoded)
    if (!$connection.Wait(30000)) { throw 'The machine installer did not connect.' }
    $pipe.Write($manifest, 0, $manifest.Length)
    foreach ($entry in $release.files.psobject.Properties) {
        $file = [IO.File]::OpenRead((Join-Path $PSScriptRoot "payload\$($entry.Name)"))
        try {
            if ($file.Length -ne $release.sizes.($entry.Name)) { throw "Package size mismatch: $($entry.Name)" }
            $file.CopyTo($pipe)
        } finally { $file.Dispose() }
    }
} finally { $pipe.Dispose() }
$installed.WaitForExit()
if ($installed.ExitCode -ne 0) { throw "Machine update failed (exit $($installed.ExitCode))." }

$root = [IO.Directory]::CreateDirectory($Destination).FullName
$run = [Guid]::NewGuid().ToString('N')
$link = Join-Path $root 'Muster.lnk'
$pending = Join-Path $root "$run.lnk"
$backup = Join-Path $root "$run.previous.lnk"
$lock = [IO.File]::Open((Join-Path $root 'update.lock'), 'OpenOrCreate', 'ReadWrite', 'None')
try {
    $shell = New-Object -ComObject WScript.Shell
    $shortcut = $shell.CreateShortcut($pending)
    $shortcut.TargetPath = Join-Path $active 'Muster.exe'
    $shortcut.WorkingDirectory = $active
    $shortcut.Save()
    $replacing = [IO.File]::Exists($link)
    if ($replacing) { [IO.File]::Replace($pending, $link, $backup) }
    else { [IO.File]::Move($pending, $link) }
    try {
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
    $lock.Dispose()
}
