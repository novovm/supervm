# Implementation of install-novovm-rendezvous.ps1 -UserSession. Windows owns
# startup/recovery; this script is NOT a second service loop or key provisioner.
[CmdletBinding()]
param(
    [ValidateSet('Install','Status','Uninstall')][string]$Action = 'Status',
    [string]$SourceBinary,
    [string]$ExpectedSha256,
    [string]$Configuration,
    [string]$NativeMailboxListener,
    [string]$ControlBinary,
    [string]$ExpectedControlSha256,
    [switch]$Start
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$accountName = [Security.Principal.WindowsIdentity]::GetCurrent().Name
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
function Set-OwnerDirectory([string]$Path) {
    $acl = New-Object Security.AccessControl.DirectorySecurity
    $acl.SetAccessRuleProtection($true,$false)
    $acl.SetOwner([Security.Principal.SecurityIdentifier]::new($sid))
    foreach ($identity in @($sid,'S-1-5-18','S-1-5-32-544')) {
        $id = [Security.Principal.SecurityIdentifier]::new($identity)
        $acl.AddAccessRule((New-Object Security.AccessControl.FileSystemAccessRule($id,'FullControl','ContainerInherit,ObjectInherit','None','Allow')))
    }
    Set-Acl -LiteralPath $Path -AclObject $acl
}
function Invoke-Owner([string]$Verb, [string]$Listener = '') {
    $ownerArgs = @($Verb, ('"' + [IO.Path]::GetFullPath($Configuration) + '"'))
    if ($Listener) { $ownerArgs += ('"' + [IO.Path]::GetFullPath($Listener) + '"') }
    $process = Start-Process -FilePath ([IO.Path]::GetFullPath($SourceBinary)) -ArgumentList $ownerArgs -WindowStyle Hidden -Wait -PassThru
    if ($process.ExitCode -ne 0) { throw "Rendezvous owner $Verb failed (exit $($process.ExitCode))." }
}
if ((($ControlBinary -or $ExpectedControlSha256) -and -not $NativeMailboxListener) -or
    ($Action -ne 'Install' -and ($NativeMailboxListener -or $Start))) {
    throw 'Native listener, control executable and start options apply only to Install.'
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
if ($config.version -ne 3 -or -not [IO.Path]::IsPathRooted($config.root)) { throw 'Existing protected ML-DSA-65 self-owned v3 configuration required; old policy is not migrated by installation.' }
if ($config.chain_id -le 0 -or $config.library_sha256 -notmatch '^[a-fA-F0-9]{64}$' -or
    -not [IO.Path]::IsPathFullyQualified($config.library) -or
    -not [IO.Path]::IsPathFullyQualified($config.transport_identity_path)) {
    throw 'Explicit chain, qualified library pin and absolute protected transport identity are required.'
}
Assert-Plain $config.root
if ($manifest -and $manifest.stateRoot -ne $config.root) { throw 'Cannot replace an existing node root.' }
if ((Test-Path -LiteralPath $configPath) -and (Get-Content -LiteralPath $configPath -Raw).Trim() -ne $raw.Trim()) {
    throw 'Existing configuration differs; explicit owner migration required.'
}
if (-not $manifest -and (Test-Path -LiteralPath $directory)) { throw 'Unmanaged installation directory preserved.' }
if ($NativeMailboxListener) {
    if (-not $ControlBinary -or $ExpectedControlSha256 -notmatch '^[a-fA-F0-9]{64}$') {
        throw 'Verified control executable and SHA-256 required for native mailbox enrollment.'
    }
    Assert-Plain $NativeMailboxListener
    Assert-Plain $ControlBinary
    if ((Get-FileHash -LiteralPath $ControlBinary -Algorithm SHA256).Hash -ne $ExpectedControlSha256) {
        throw 'Control executable hash differs.'
    }
    $ownerRoot = [IO.Path]::GetFullPath($config.root)
    $transportKey = Join-Path $ownerRoot 'transport.sealed'
    $enrollment = Join-Path $ownerRoot 'native-enrollment.json'
    if ([IO.Path]::GetFullPath($config.transport_identity_path) -ne $transportKey -or
        $ownerRoot -eq $directory -or $ownerRoot.StartsWith($directory + '\', [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Use a separate mailbox state directory and its own transport.sealed.'
    }
    Assert-Plain $transportKey
    Assert-Plain $enrollment
    $expected = [ordered]@{
        version = 1
        configuration = (Get-FileHash -LiteralPath $Configuration -Algorithm SHA256).Hash
        listener = (Get-FileHash -LiteralPath $NativeMailboxListener -Algorithm SHA256).Hash
    } | ConvertTo-Json -Compress
    $newEnrollment = -not (Test-Path -LiteralPath $enrollment)
    if ($newEnrollment) {
        if ((Test-Path -LiteralPath $ownerRoot) -and @(Get-ChildItem -LiteralPath $ownerRoot -Force).Count) {
            throw 'Existing unmanaged mailbox state is preserved. Use the existing-owner install path.'
        }
        New-Item -ItemType Directory -Path $ownerRoot -Force | Out-Null
        Set-OwnerDirectory $ownerRoot
    } elseif ((Get-Content -LiteralPath $enrollment -Raw) -cne $expected) {
        throw 'Enrollment inputs changed. Existing mailbox state is preserved.'
    }
    Invoke-Owner '--check-self-owned-native' $NativeMailboxListener
    if ($newEnrollment) {
        $marker = [IO.File]::Open($enrollment, [IO.FileMode]::CreateNew)
        try {
            $bytes = [Text.Encoding]::UTF8.GetBytes($expected)
            $marker.Write($bytes, 0, $bytes.Length)
            $marker.Flush($true)
        } finally { $marker.Dispose() }
    }
    $operation = if (Test-Path -LiteralPath $transportKey -PathType Leaf) { 'inspect' } else { 'generate' }
    if ($operation -eq 'generate' -and (Test-Path -LiteralPath (Join-Path $ownerRoot 'enrollment.started'))) {
        throw 'Existing mailbox transport key is missing. Recovery is required.'
    }
    $identityArgs = @('native-identity', $operation, '--purpose', 'transport', '--chain-id', $config.chain_id,
        '--key-file', $transportKey, '--library', $config.library, '--library-sha256', $config.library_sha256.ToLowerInvariant())
    & ([IO.Path]::GetFullPath($ControlBinary)) @identityArgs | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "Mailbox transport identity $operation failed." }
    Invoke-Owner '--prepare-self-owned-native' $NativeMailboxListener
} else {
    foreach ($file in @('keys.dpapi','self-owned-policy.dpapi','service.journal')) {
        $state = Join-Path $config.root $file
        Assert-Plain $state
        if (-not (Test-Path -LiteralPath $state -PathType Leaf)) { throw 'Existing custody/history required; never initialize replacement keys.' }
    }
    Invoke-Owner '--check'
}
if (-not (Test-Path -LiteralPath $directory)) {
    New-Item -ItemType Directory -Path $directory -Force | Out-Null
    Set-OwnerDirectory $directory
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
# Keep SID-based ownership checks above; Task Scheduler's logon trigger on
# Windows can reject a SID UserId while accepting its resolved account name.
$trigger = New-ScheduledTaskTrigger -AtLogOn -User $accountName
$principal = New-ScheduledTaskPrincipal -UserId $accountName -LogonType Interactive -RunLevel Limited
$settings = New-ScheduledTaskSettingsSet -Hidden -StartWhenAvailable -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -ExecutionTimeLimit ([TimeSpan]::Zero) -MultipleInstances IgnoreNew -RestartCount 3 -RestartInterval (New-TimeSpan -Minutes 1)
Register-ScheduledTask -TaskName $name -TaskPath '\' -Action $actionDefinition -Trigger $trigger -Principal $principal -Settings $settings -Description 'NOVOVM self-owned communications, original user DPAPI; no ledger or VPN changes.' -Force | Out-Null
if ($Start) {
    Start-ScheduledTask -TaskName $name -TaskPath '\'
    Write-Output 'Managed start requested. Check the live signed record and authenticate from the intended networks.'
    return
}
Write-Output 'Registered for current-user logon; requested up to three crash restarts from Windows (verify on this host). Existing foreground node untouched; no pre-login or network availability claim.'
