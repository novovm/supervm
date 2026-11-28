#Requires -Version 7.0
[CmdletBinding(DefaultParameterSetName = 'Controller')]
param(
    [Parameter(Mandatory, ParameterSetName = 'Controller')][string]$PackageRoot,
    [Parameter(Mandatory, ParameterSetName = 'Controller')][string]$OriginalConfig,
    [Parameter(Mandatory, ParameterSetName = 'Controller')][ValidatePattern('^[0-9a-fA-F]{64}$')][string]$OriginalConfigSha256,
    [Parameter(Mandatory, ParameterSetName = 'Controller')][string]$OriginalProgress,
    [Parameter(Mandatory, ParameterSetName = 'Controller')][string]$OriginalFirewallRule,
    [Parameter(ParameterSetName = 'Controller')][switch]$Apply,
    [Parameter(Mandatory, ParameterSetName = 'Owner')][switch]$Owner
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'NovonetExistingProcess.ps1')

$oldTaskName = 'NOVOVM-D-654b9991-Existing-20261119'
$newTaskName = 'NOVOVM-D-98d497a8-Existing'
$work = Join-Path $env:ProgramData 'NOVOVM\D-round-recovery-98d497a8'
$originalHome = Join-Path $env:ProgramData 'NOVOVM\selfrun\91129655606029\D\novovm-common-654b9991-20261119'
$oldNodePin = '04a0e0b174bc9de1fa7cd71eb8aff54b06ad670fa0b407c0e75394e46a7e3ee4'
$nodePin = 'e4450d135c250147a2f6b7121bae039d86665bd506531d34eb37b686e0ed2cef'
$manifestPin = '0aa51f2f90f04fa8429fd7bc15ebee0b0da91eedc36a91c7e3d93bd1eef4a4d8'
$libraryPin = 'f9e8ded21753dfd32573de7bf66da8a785120444e810ba9fa865bfa4d639fcaa'
$localService = 'S-1-5-19'
$ports = @(41053, 42053)

function Assert-Hash([string]$Path, [string]$Sha256) {
    if ((Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash -ine $Sha256) {
        throw "Pinned file SHA-256 differs: $([IO.Path]::GetFileName($Path))"
    }
}

function Assert-PlainPath([string]$Path) {
    for ($item = Get-Item -LiteralPath $Path -Force; $item; $item = $(if ($item.PSIsContainer) { $item.Parent } else { $item.Directory })) {
        if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Redirected input path' }
    }
}

function Read-OriginalConfig([string]$Path, [string]$Pin) {
    if ($Path -ine (Join-Path $originalHome 'run.json')) { throw 'Expected the config used by the original D task' }
    Assert-PlainPath $Path
    Assert-Hash $Path $Pin
    $config = Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
    if ($config.profile -cne 'native_execution_tick' -or $config.udp_addr -cne '192.168.71.59:41053' -or
        $null -ne $config.business_activation) { throw 'Expected original D configuration without V6' }
    foreach ($path in @($config.database, $config.library, $config.signing_key_file, $config.transport_key_file, $config.novonet_config_file)) {
        if (![IO.Path]::IsPathFullyQualified($path)) { throw 'Original runtime paths must be absolute' }
        Assert-PlainPath $path
    }
    if (!(Test-Path -LiteralPath $config.database -PathType Container) -or
        !(Test-Path -LiteralPath (Join-Path $config.database 'CURRENT') -PathType Leaf)) { throw 'Original RocksDB does not exist' }
    Assert-Hash $config.library $libraryPin
    return $config
}

function Read-Progress([string]$Path) {
    $file = Get-Item -LiteralPath $Path
    if (([DateTime]::UtcNow - $file.LastWriteTimeUtc).TotalSeconds -gt 30) { throw 'Original progress is stale' }
    $progress = Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
    if ($progress.entry -cne 'original_native_execution_tick' -or $progress.chain_id -ne 91129655606029 -or
        $progress.current_height -ne 3 -or $progress.head.height -ne 2 -or $progress.rpc_indexed_height -ne 2 -or
        [Convert]::ToHexString([byte[]]$progress.head.block_hash) -ine '7f2d438862e6bd2934862e4cafcb86dc3bf6ffa0a414d532014e420efa1aafc6' -or
        [Convert]::ToHexString([byte[]]$progress.head.state_root) -ine '418cb1c74c5121d43d4a392a309a052d0f6a8a513bc9377b6aa503d80591e439' -or
        $progress.execution_failures -ne 0 -or $progress.recovery_in_progress -ne $false) {
        throw 'Expected healthy, committed H2 on the original chain'
    }
    return $progress
}

function Get-Node {
    $nodes = @((Get-NovonetNodeExecutionState).active)
    if ($nodes.Count -ne 1 -or (Invoke-CimMethod -InputObject $nodes[0] -MethodName GetOwnerSid).Sid -ne $localService) {
        throw 'Expected one LocalService database owner'
    }
    return $nodes[0]
}

function Read-Release([string]$Root) {
    Assert-PlainPath $Root
    $manifestPath = Join-Path $Root 'release-manifest.json'
    Assert-Hash $manifestPath $manifestPin
    $release = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
    foreach ($entry in $release.artifacts) {
        $path = [IO.Path]::GetFullPath((Join-Path $Root $entry.path))
        if (!$path.StartsWith($Root.TrimEnd('\') + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'Candidate path escapes release root' }
        Assert-PlainPath $path
        if ((Get-Item -LiteralPath $path).Length -ne $entry.bytes) { throw 'Candidate size mismatch' }
        Assert-Hash $path $entry.sha256
    }
    Assert-Hash (Join-Path $Root 'bin\novovm-node.exe') $nodePin
    Assert-Hash (Join-Path $Root 'aoem\windows\core\bin\aoem_ffi.dll') $libraryPin
    return $release
}

function Protect-WorkDirectory {
    $acl = [Security.AccessControl.DirectorySecurity]::new()
    $acl.SetAccessRuleProtection($true, $false)
    foreach ($sid in @('S-1-5-18', 'S-1-5-32-544', $localService)) {
        $rights = if ($sid -eq $localService) { 'ReadAndExecute' } else { 'FullControl' }
        $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new(
            [Security.Principal.SecurityIdentifier]::new($sid), $rights, 'ContainerInherit,ObjectInherit', 'None', 'Allow'))
    }
    Set-Acl -LiteralPath $work -AclObject $acl
}

if ($Owner) {
    if ([Security.Principal.WindowsIdentity]::GetCurrent().User.Value -ne $localService -or $PSScriptRoot -ne $work) {
        throw 'Candidate requires the staged LocalService task'
    }
    $plan = Get-Content -LiteralPath (Join-Path $work 'launch.json') -Raw | ConvertFrom-Json
    $null = Read-OriginalConfig $plan.config $plan.config_sha256
    Assert-Hash $plan.candidate $nodePin
    Assert-Hash $plan.archive_manifest $manifestPin
    Wait-NovonetNodeExit
    $env:AOEM_DLL_MANIFEST = $plan.library_manifest
    $env:NOVOVM_AOEM_MANIFEST = $plan.library_manifest
    $env:AOEM_DLL_MANIFEST_REQUIRED = '1'
    $env:AOEM_SEMANTIC_GRAPH_V2_WORKERS = '1'
    $env:NOVOVM_NODE_MODE = 'native_execution_tick'
    $env:NOVOVM_NATIVE_EXECUTION_CONFIG = $plan.config
    $env:NOVOVM_NATIVE_EXECUTION_START = 'existing'
    $env:NOVOVM_NATIVE_EXECUTION_RUN_MS = '86400000'
    $env:NOVOVM_PROFILE_NATIVE_DELIVERY = '1'
    $env:NOVOVM_NATIVE_EXECUTION_PIPELINE_PROGRESS_REPORT_PATH = Join-Path $work 'logs\progress.json'
    & $plan.candidate 1> (Join-Path $work 'logs\node.stdout') 2> (Join-Path $work 'logs\node.stderr')
    exit $LASTEXITCODE
}

$principal = [Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())
if (!$principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { throw 'Run D preflight in elevated PowerShell' }

$PackageRoot = (Resolve-Path -LiteralPath $PackageRoot).Path
$OriginalConfig = (Resolve-Path -LiteralPath $OriginalConfig).Path
$OriginalProgress = (Resolve-Path -LiteralPath $OriginalProgress).Path
if ($OriginalProgress -ine (Join-Path $originalHome 'logs\progress.json')) { throw 'Expected the original D progress file' }
$release = Read-Release $PackageRoot
$config = Read-OriginalConfig $OriginalConfig $OriginalConfigSha256
$prior = Read-Progress $OriginalProgress
$task = Get-ScheduledTask -TaskName $oldTaskName -TaskPath '\'
$user = $task.Principal.UserId
$sid = if ($user -match '^S-1-') { $user } else { [Security.Principal.NTAccount]::new($user).Translate([Security.Principal.SecurityIdentifier]).Value }
if ($task.State -ne 'Running' -or $sid -ne $localService) { throw 'Original D task is not running as LocalService' }
if (@($task.Actions).Count -ne 1 -or $task.Actions[0].Arguments -notmatch 'Start-Owner\.ps1.+-Owner(?:\s|$)') {
    throw 'Original D task action differs'
}
$original = Get-Node
Assert-Hash $original.ExecutablePath $oldNodePin
$ownerPorts = @((Get-NetUDPEndpoint -OwningProcess $original.ProcessId).LocalPort)
if (@($ports | Where-Object { $_ -notin $ownerPorts }).Count) { throw 'Original D listeners do not belong to the old node' }
$firewall = Get-NetFirewallRule -Name $OriginalFirewallRule
if ($firewall.Enabled -ne 'True' -or $firewall.Action -ne 'Allow' -or $firewall.Direction -ne 'Inbound' -or
    ($firewall | Get-NetFirewallApplicationFilter).Program -ine $original.ExecutablePath -or
    ($firewall | Get-NetFirewallPortFilter).Protocol -ne 'UDP') { throw 'Original firewall rule differs from D owner' }
if ((Test-Path -LiteralPath $work) -or (Get-ScheduledTask -TaskName $newTaskName -ErrorAction SilentlyContinue) -or
    (Get-NetFirewallRule -Name $newTaskName -ErrorAction SilentlyContinue)) { throw 'A prior candidate attempt exists; inspect its evidence' }

$dbBytes = (Get-ChildItem -LiteralPath $config.database -Recurse -File -Force | Measure-Object Length -Sum).Sum
$packageBytes = ($release.artifacts | Measure-Object bytes -Sum).Sum
if ([IO.DriveInfo]::new([IO.Path]::GetPathRoot($work)).AvailableFreeSpace -lt ($dbBytes + $packageBytes)) {
    throw 'Insufficient free space to stage the release and cold backup'
}
Assert-PlainPath (Split-Path -Parent $work)

if (!$Apply) {
    @{ mode = 'preflight'; applied = $false; source = $release.git_commit; prior_pid = $original.ProcessId;
       chain_id = $prior.chain_id; committed_height = $prior.head.height; current_height = $prior.current_height;
       round = $prior.round; start = 'existing'; v6 = $false } | ConvertTo-Json
    return
}

$null = New-Item -ItemType Directory -Path $work
Protect-WorkDirectory
$package = Join-Path $work 'release'
Copy-Item -LiteralPath $PackageRoot -Destination $package -Recurse
$null = Read-Release $package
$manifest = Join-Path $package 'release-manifest.json'
$candidate = Join-Path $package 'bin\novovm-node.exe'
$aoemManifest = Join-Path $package 'aoem\manifest\aoem-manifest.json'
$logs = New-Item -ItemType Directory -Path (Join-Path $work 'logs')
& icacls.exe $logs.FullName /grant '*S-1-5-19:(OI)(CI)M' | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'Cannot grant LocalService its log directory' }
Copy-Item -LiteralPath $PSCommandPath, (Join-Path $PSScriptRoot 'NovonetExistingProcess.ps1') -Destination $work
@{ config = $OriginalConfig; config_sha256 = $OriginalConfigSha256; candidate = $candidate; archive_manifest = $manifest;
   library_manifest = $aoemManifest } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $work 'launch.json')
Export-ScheduledTask -TaskName $oldTaskName -TaskPath '\' | Set-Content -LiteralPath (Join-Path $work 'original-task.xml')

$launchRequested = $false
$disabled = $false
try {
    $fresh = Get-Node
    if ($fresh.ProcessId -ne $original.ProcessId -or $fresh.CreationDate -ne $original.CreationDate) { throw 'Original D owner changed after preflight' }
    $prior = Read-Progress $OriginalProgress
    Disable-ScheduledTask -TaskName $oldTaskName -TaskPath '\' | Out-Null
    $disabled = $true
    Stop-NovonetExactProcess -ProcessId $original.ProcessId -ExpectedPath $original.ExecutablePath -ExpectedSha256 $oldNodePin
    if (Get-NetUDPEndpoint | Where-Object LocalPort -in $ports) { throw 'Original D listeners still in use' }
    $backup = Join-Path $work 'database-cold-before'
    Copy-Item -LiteralPath $config.database -Destination $backup -Recurse
    foreach ($file in Get-ChildItem -LiteralPath $config.database -Recurse -File -Force) {
        $relative = [IO.Path]::GetRelativePath($config.database, $file.FullName)
        Assert-Hash (Join-Path $backup $relative) (Get-FileHash -LiteralPath $file.FullName).Hash
    }
    Copy-NetFirewallRule -Name $firewall.Name -NewName $newTaskName | Out-Null
    Get-NetFirewallRule -Name $newTaskName | Get-NetFirewallApplicationFilter | Set-NetFirewallApplicationFilter -Program $candidate
    $action = New-ScheduledTaskAction -Execute (Get-Command pwsh).Source -Argument ('-NoProfile -NonInteractive -File "' + (Join-Path $work (Split-Path -Leaf $PSCommandPath)) + '" -Owner') -WorkingDirectory $work
    $owner = New-ScheduledTaskPrincipal -UserId $localService -LogonType ServiceAccount
    $settings = New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::Zero) -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
    Register-ScheduledTask -TaskName $newTaskName -Action $action -Principal $owner -Settings $settings | Out-Null
    $launchRequested = $true
    Start-ScheduledTask -TaskName $newTaskName
    $newProgress = Join-Path $logs.FullName 'progress.json'
    $deadline = [DateTime]::UtcNow.AddSeconds(60)
    while (!(Test-Path -LiteralPath $newProgress) -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 500 }
    $current = Read-Progress $newProgress
    $running = Get-Node
    $listeners = @((Get-NetUDPEndpoint -OwningProcess $running.ProcessId).LocalPort)
    $module = @((Get-Process -Id $running.ProcessId).Modules | Where-Object ModuleName -eq 'aoem_ffi.dll')
    if ($running.ExecutablePath -ine $candidate -or $module.Count -ne 1 -or
        @($ports | Where-Object { $_ -notin $listeners }).Count -ne 0 -or
        ($current.head | ConvertTo-Json -Compress) -cne ($prior.head | ConvertTo-Json -Compress)) { throw 'Candidate owner or original H2 state differs' }
    Assert-Hash $module[0].FileName $libraryPin
    Assert-Hash $OriginalConfig $OriginalConfigSha256
    @{ mode = 'candidate-started'; applied = $true; pid = $running.ProcessId; chain_id = $current.chain_id;
       committed_height = $current.head.height; current_height = $current.current_height;
       round = $current.round; v6 = $false; peer_vote_verified = $false } | ConvertTo-Json
} catch {
    if ($disabled -and !$launchRequested) {
        $nodes = @((Get-NovonetNodeExecutionState).active)
        if ($nodes.Count -eq 0) { Wait-NovonetNodeExit }
        if ($nodes.Count -eq 0 -or ($nodes.Count -eq 1 -and $nodes[0].ProcessId -eq $original.ProcessId -and $nodes[0].CreationDate -eq $original.CreationDate)) {
            Enable-ScheduledTask -TaskName $oldTaskName -TaskPath '\' | Out-Null
            if ($nodes.Count -eq 0) { Start-ScheduledTask -TaskName $oldTaskName -TaskPath '\' }
        }
    }
    throw
}
