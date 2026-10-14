# Implementation of install-novovm-rendezvous.ps1 -UserSession. Windows owns
# startup/recovery; this script is NOT a second service loop or key provisioner.
[CmdletBinding()]
param(
    [ValidateSet('Install','Status','Uninstall')][string]$Action = 'Status',
    [string]$SourceBinary,
    [string]$ExpectedSha256,
    [string]$Configuration
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$name = 'NOVOVM-Rendezvous-' + $sid
$directory = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'NOVOVM\Rendezvous'
$program = Join-Path $directory 'novovm-rendezvous-service.exe'
$configPath = Join-Path $directory 'config.json'
$manifestPath = Join-Path $directory 'install.json'
$arguments = '--foreground "' + $configPath + '"'

function Assert-Plain([string]$Path) {
    $cursor = [IO.Path]::GetFullPath($Path)
    while ($cursor) {
        if ((Test-Path -LiteralPath $cursor) -and
            ((Get-Item -LiteralPath $cursor -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)) {
            throw 'Installation/state paths must not contain reparse points.'
        }
        $cursor = [IO.Path]::GetDirectoryName($cursor)
    }
}
foreach ($p in @($directory,$program,$configPath,$manifestPath)) { Assert-Plain $p }
$manifest = if (Test-Path -LiteralPath $manifestPath) { Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json } else { $null }
$task = Get-ScheduledTask -TaskPath '\' -ErrorAction Stop | Where-Object TaskName -eq $name
$taskSid = $null
if ($task) {
    # Task Scheduler normalizes a supplied SID to an account name on readback.
    # Compare resolved identity, not that presentation string.
    $taskSid = if ($task.Principal.UserId -match '^S-1-') {
        ([Security.Principal.SecurityIdentifier]::new($task.Principal.UserId)).Value
    } else {
        ([Security.Principal.NTAccount]::new($task.Principal.UserId)).Translate([Security.Principal.SecurityIdentifier]).Value
    }
}
if ($manifest -and ($manifest.version -ne 2 -or $manifest.kind -ne 'user-session' -or
    $manifest.sid -ne $sid -or $manifest.program -ne $program -or $manifest.config -ne $configPath)) {
    throw 'Unrecognized installation; preserved.'
}
if ($task -and (-not $manifest -or @($task.Actions).Count -ne 1 -or
    $task.Actions[0].Execute -ne $program -or $task.Actions[0].Arguments -ne $arguments -or
    $task.Actions[0].WorkingDirectory -ne $directory -or
    $taskSid -ne $sid -or [int]$task.Principal.LogonType -ne 3 -or [int]$task.Principal.RunLevel -ne 0)) {
    throw 'Task ownership differs; preserved.'
}
if ($Action -eq 'Status') {
    $info = if ($task) { Get-ScheduledTaskInfo -InputObject $task } else { $null }
    $live = @(Get-CimInstance Win32_Process -Filter "Name='novovm-rendezvous-service.exe'" |
        Where-Object { $_.ExecutablePath -eq $program })
    [ordered]@{installed=[bool]$task; startup='current-user-logon';
        taskState=$(if($task){[string]$task.State}else{'Absent'});
        lastResult=$(if($info){$info.LastTaskResult}else{$null});
        configuredRestartCount=$(if($task){$task.Settings.RestartCount}else{0});
        managedProcessCount=$live.Count; onlineVerified=$false} | ConvertTo-Json
    return
}
if ($Action -eq 'Uninstall') {
    # Stop the exact native action, not arbitrary same-name foreground nodes.
    if ($task) {
        Stop-ScheduledTask -InputObject $task
        Unregister-ScheduledTask -InputObject $task -Confirm:$false
    }
    Write-Output 'Automatic startup removed. Existing node identity, messages and installation files retained.'
    return
}
if ($task -and $task.State -eq 'Running') { throw 'Stop the managed node before replacing its binary.' }
if ($ExpectedSha256 -notmatch '^[a-fA-F0-9]{64}$' -or -not (Test-Path -LiteralPath $SourceBinary -PathType Leaf)) {
    throw 'Verified source binary and SHA-256 required.'
}
Assert-Plain $SourceBinary
Assert-Plain $Configuration
if ((Get-FileHash -LiteralPath $SourceBinary -Algorithm SHA256).Hash -ne $ExpectedSha256) { throw 'Binary hash differs.' }
# Direct native task action, no detached child or infinite PowerShell restart
# wrapper. Reject console-subsystem executables rather than flashing a window.
$pe = [IO.BinaryReader]::new([IO.File]::OpenRead($SourceBinary))
try {
    $pe.BaseStream.Position = 0x3c
    $offset = $pe.ReadUInt32()
    $pe.BaseStream.Position = $offset
    if ($pe.ReadUInt32() -ne 0x4550) { throw 'Invalid PE header.' }
    $pe.BaseStream.Position = $offset + 24 + 68
    if ($pe.ReadUInt16() -ne 2) { throw 'Windowless Windows entrypoint required.' }
} finally { $pe.Dispose() }
$raw = Get-Content -LiteralPath $Configuration -Raw
$config = $raw | ConvertFrom-Json
if ($config.version -ne 2 -or -not [IO.Path]::IsPathRooted($config.root)) { throw 'Existing self-owned v2 configuration required.' }
Assert-Plain $config.root
foreach ($file in @('keys.dpapi','self-owned-policy.dpapi','service.journal')) {
    $state = Join-Path $config.root $file
    Assert-Plain $state
    if (-not (Test-Path -LiteralPath $state -PathType Leaf)) { throw 'Existing custody/history required; never initialize replacement keys.' }
}
if ($manifest -and $manifest.stateRoot -ne $config.root) { throw 'Cannot replace an existing node root.' }
if ((Test-Path -LiteralPath $configPath) -and (Get-Content -LiteralPath $configPath -Raw).Trim() -ne $raw.Trim()) {
    throw 'Existing configuration differs; explicit owner migration required.'
}
if (-not $manifest -and (Test-Path -LiteralPath $directory)) { throw 'Unmanaged installation directory preserved.' }
# Validate before touching installation files; validation does not take the
# running owner's lock, bind a socket, or initialize custody.
$check = Start-Process -FilePath ([IO.Path]::GetFullPath($SourceBinary)) -ArgumentList @('--check',('"'+[IO.Path]::GetFullPath($Configuration)+'"')) -WindowStyle Hidden -Wait -PassThru
if ($check.ExitCode -ne 0) { throw 'Configuration check failed; installation unchanged.' }
if (-not (Test-Path -LiteralPath $directory)) {
    New-Item -ItemType Directory -Path $directory -Force | Out-Null
    $acl = New-Object Security.AccessControl.DirectorySecurity
    $acl.SetAccessRuleProtection($true,$false)
    foreach ($identity in @($sid,'S-1-5-18','S-1-5-32-544')) {
        $id = New-Object Security.Principal.SecurityIdentifier($identity)
        $acl.AddAccessRule((New-Object Security.AccessControl.FileSystemAccessRule($id,'FullControl','ContainerInherit,ObjectInherit','None','Allow')))
    }
    Set-Acl -LiteralPath $directory -AclObject $acl
}
$pending = Join-Path $directory 'binary.pending'
Copy-Item -LiteralPath $SourceBinary -Destination $pending -Force
if ((Get-FileHash -LiteralPath $pending -Algorithm SHA256).Hash -ne $ExpectedSha256) { throw 'Staged binary differs.' }
Move-Item -LiteralPath $pending -Destination $program -Force
[IO.File]::WriteAllText($configPath,$raw,(New-Object Text.UTF8Encoding($false)))
[ordered]@{version=2;kind='user-session';sid=$sid;program=$program;config=$configPath;
    stateRoot=$config.root;sha256=$ExpectedSha256.ToLowerInvariant()} | ConvertTo-Json |
    Set-Content -LiteralPath $manifestPath -Encoding UTF8
$actionDefinition = New-ScheduledTaskAction -Execute $program -Argument $arguments -WorkingDirectory $directory
$trigger = New-ScheduledTaskTrigger -AtLogOn -User $sid
$principal = New-ScheduledTaskPrincipal -UserId $sid -LogonType Interactive -RunLevel Limited
$settings = New-ScheduledTaskSettingsSet -Hidden -StartWhenAvailable -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -ExecutionTimeLimit ([TimeSpan]::Zero) -MultipleInstances IgnoreNew -RestartCount 3 -RestartInterval (New-TimeSpan -Minutes 1)
Register-ScheduledTask -TaskName $name -TaskPath '\' -Action $actionDefinition -Trigger $trigger -Principal $principal -Settings $settings -Description 'NOVOVM self-owned communications, original user DPAPI; no ledger or VPN changes.' -Force | Out-Null
Write-Output 'Registered for current-user logon; requested up to three crash restarts from Windows (verify on this host). Existing foreground node untouched; no pre-login or network availability claim.'
