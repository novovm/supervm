[CmdletBinding()]
param(
    [ValidateSet("register", "start", "stop", "status", "upgrade", "rollback", "set-runtime", "set-policy")]
    [string]$Action = "status",
    [string]$RepoRoot = "",
    [string]$Version = "",
    [string]$TargetVersion = "",
    [string]$RollbackVersion = "",
    [string]$GatewayBinaryFrom = "",
    [string]$NodeBinaryFrom = "",
    [switch]$SetCurrent,
    [string]$ReleaseRoot = "artifacts/runtime/releases",
    [string]$RuntimeStateFile = "artifacts/runtime/lifecycle/state.json",
    [string]$RuntimePidFile = "artifacts/runtime/lifecycle/novovm-up.pid",
    [string]$RuntimeLogDir = "artifacts/runtime/lifecycle/logs",
    [ValidateSet("dev", "prod")]
    [string]$Profile = "prod",
    [ValidateSet("full", "l1", "l2", "l3")]
    [string]$RoleProfile = "full",
    [switch]$NoGateway,
    [switch]$UseNodeWatchMode,
    [switch]$LeanIo,
    [switch]$EnableReconcileDaemon,
    [string]$ReconcileSenderAddress = "",
    [string]$ReconcileRpcEndpoint = "http://127.0.0.1:9899",
    [ValidateRange(1, 86400)]
    [int]$ReconcileIntervalSeconds = 15,
    [ValidateRange(1, 3600)]
    [int]$ReconcileRestartDelaySeconds = 3,
    [ValidateRange(0, 1000)]
    [int]$ReconcileReplayMaxPerPayout = 3,
    [ValidateRange(0, 86400)]
    [int]$ReconcileReplayCooldownSec = 30,
    [string]$GatewayBind = "127.0.0.1:9899",
    [string]$SpoolDir = "artifacts/ingress/spool",
    [ValidateRange(10, 60000)]
    [int]$PollMs = 200,
    [ValidateRange(50, 60000)]
    [int]$SupervisorPollMs = 1000,
    [ValidateRange(1, 1000000)]
    [int]$NodeWatchBatchMaxFiles = 1024,
    [ValidateRange(0, 86400)]
    [int]$IdleExitSeconds = 0,
    [ValidateRange(0, 4294967295)]
    [uint32]$GatewayMaxRequests = 0,
    [ValidateRange(1, 300)]
    [int]$StartGraceSeconds = 6,
    [ValidateRange(1, 600)]
    [int]$UpgradeHealthSeconds = 12,
    [string]$RuntimeTemplateFile = "",
    [switch]$RestartAfterSetRuntime,
    [string]$NodeGroup = "",
    [string]$UpgradeWindow = "",
    [string]$RequireNodeGroup = "",
    [switch]$Force
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Resolve-RootPath {
    param([string]$Root)
    if (-not $Root) {
        return (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
    }
    return (Resolve-Path $Root).Path
}

function Resolve-FullPath {
    param(
        [string]$Root,
        [string]$Value
    )
    if ([System.IO.Path]::IsPathRooted($Value)) {
        return [System.IO.Path]::GetFullPath($Value)
    }
    return [System.IO.Path]::GetFullPath((Join-Path $Root $Value))
}

function Ensure-ParentDir {
    param([string]$PathValue)
    $parent = Split-Path -Parent $PathValue
    if ($parent) {
        New-Item -ItemType Directory -Force -Path $parent | Out-Null
    }
}

function Ensure-Dir {
    param([string]$PathValue)
    New-Item -ItemType Directory -Force -Path $PathValue | Out-Null
}

function Get-BinaryFileName {
    param([string]$BaseName)
    if ($env:OS -eq "Windows_NT") {
        return ($BaseName + ".exe")
    }
    return $BaseName
}

function New-RuntimeConfigFromParams {
    return [pscustomobject]@{
        profile = $Profile
        role_profile = $RoleProfile
        no_gateway = [bool]$NoGateway
        use_node_watch_mode = [bool]$UseNodeWatchMode
        lean_io = [bool]$LeanIo
        enable_reconcile_daemon = [bool]$EnableReconcileDaemon
        reconcile_sender_address = $ReconcileSenderAddress
        reconcile_rpc_endpoint = $ReconcileRpcEndpoint
        reconcile_interval_seconds = $ReconcileIntervalSeconds
        reconcile_restart_delay_seconds = $ReconcileRestartDelaySeconds
        reconcile_replay_max_per_payout = $ReconcileReplayMaxPerPayout
        reconcile_replay_cooldown_sec = $ReconcileReplayCooldownSec
        gateway_bind = $GatewayBind
        spool_dir = $SpoolDir
        poll_ms = $PollMs
        supervisor_poll_ms = $SupervisorPollMs
        node_watch_batch_max_files = $NodeWatchBatchMaxFiles
        idle_exit_seconds = $IdleExitSeconds
        gateway_max_requests = $GatewayMaxRequests
    }
}

function New-DefaultState {
    param(
        [string]$CurrentRelease,
        [pscustomobject]$RuntimeConfig
    )
    return [pscustomobject]@{
        version = 1
        current_release = $CurrentRelease
        previous_release = ""
        runtime = $RuntimeConfig
        governance = [pscustomobject]@{
            node_group = ""
            upgrade_window = ""
        }
    }
}

function Load-State {
    param(
        [string]$PathValue,
        [string]$CurrentReleaseFallback,
        [pscustomobject]$RuntimeFallback
    )
    if (-not (Test-Path -LiteralPath $PathValue)) {
        return (New-DefaultState -CurrentRelease $CurrentReleaseFallback -RuntimeConfig $RuntimeFallback)
    }
    $raw = Get-Content -LiteralPath $PathValue -Raw
    if ([string]::IsNullOrWhiteSpace($raw)) {
        return (New-DefaultState -CurrentRelease $CurrentReleaseFallback -RuntimeConfig $RuntimeFallback)
    }
    $state = $raw | ConvertFrom-Json -ErrorAction Stop
    if ($null -eq $state.runtime) {
        $state | Add-Member -NotePropertyName runtime -NotePropertyValue $RuntimeFallback -Force
    }
    if ([string]::IsNullOrWhiteSpace([string]$state.current_release)) {
        $state.current_release = $CurrentReleaseFallback
    }
    if ($null -eq $state.previous_release) {
        $state | Add-Member -NotePropertyName previous_release -NotePropertyValue "" -Force
    }
    if ($null -eq $state.governance) {
        $state | Add-Member -NotePropertyName governance -NotePropertyValue ([pscustomobject]@{
                node_group = ""
                upgrade_window = ""
            }) -Force
    } else {
        if ($null -eq $state.governance.node_group) {
            $state.governance | Add-Member -NotePropertyName node_group -NotePropertyValue "" -Force
        }
        if ($null -eq $state.governance.upgrade_window) {
            $state.governance | Add-Member -NotePropertyName upgrade_window -NotePropertyValue "" -Force
        }
    }
    return $state
}

function Save-State {
    param(
        [string]$PathValue,
        [pscustomobject]$State
    )
    Ensure-ParentDir -PathValue $PathValue
    $json = $State | ConvertTo-Json -Depth 8
    [System.IO.File]::WriteAllText($PathValue, $json, [System.Text.Encoding]::UTF8)
}

function Resolve-ReleasePaths {
    param(
        [string]$Root,
        [string]$ReleaseVersion
    )
    if ([string]::IsNullOrWhiteSpace($ReleaseVersion)) {
        throw "release version is required"
    }
    $releaseDir = Join-Path $Root $ReleaseVersion
    $gatewayPath = Join-Path $releaseDir (Get-BinaryFileName -BaseName "novovm-evm-gateway")
    $nodePath = Join-Path $releaseDir (Get-BinaryFileName -BaseName "novovm-node")
    return [pscustomobject]@{
        release_dir = $releaseDir
        gateway_path = $gatewayPath
        node_path = $nodePath
    }
}

function Assert-ReleaseInstalled {
    param([pscustomobject]$ReleasePaths)
    if (-not (Test-Path -LiteralPath $ReleasePaths.gateway_path)) {
        throw ("release gateway binary not found: " + $ReleasePaths.gateway_path)
    }
    if (-not (Test-Path -LiteralPath $ReleasePaths.node_path)) {
        throw ("release node binary not found: " + $ReleasePaths.node_path)
    }
}

function Get-ManagedProcess {
    param([string]$PidFilePath)
    if (-not (Test-Path -LiteralPath $PidFilePath)) {
        return $null
    }
    $raw = (Get-Content -LiteralPath $PidFilePath -Raw).Trim()
    if ([string]::IsNullOrWhiteSpace($raw)) {
        return $null
    }
    $pidValue = 0
    if (-not [int]::TryParse($raw, [ref]$pidValue)) {
        return $null
    }
    return Get-Process -Id $pidValue -ErrorAction SilentlyContinue
}

function Stop-ManagedProcess {
    param([string]$PidFilePath)
    $proc = Get-ManagedProcess -PidFilePath $PidFilePath
    if ($null -eq $proc) {
        if (Test-Path -LiteralPath $PidFilePath) {
            Remove-Item -LiteralPath $PidFilePath -Force -ErrorAction SilentlyContinue
        }
        Write-Host "lifecycle_stop: already_stopped=true"
        return
    }
    Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $PidFilePath -Force -ErrorAction SilentlyContinue
    Write-Host ("lifecycle_stop: stopped=true pid={0}" -f $proc.Id)
}

function Start-ManagedProcess {
    param(
        [string]$RepoRootPath,
        [string]$UpScriptPath,
        [string]$PidFilePath,
        [string]$LogDirPath,
        [pscustomobject]$State,
        [int]$GraceSeconds
    )
    $existing = Get-ManagedProcess -PidFilePath $PidFilePath
    if ($null -ne $existing) {
        throw ("managed process already running: pid=" + $existing.Id)
    }

    $releasePaths = Resolve-ReleasePaths -Root $ReleaseRootPath -ReleaseVersion ([string]$State.current_release)
    Assert-ReleaseInstalled -ReleasePaths $releasePaths

    Ensure-Dir -PathValue $LogDirPath
    $stdout = Join-Path $LogDirPath "novovm-up.stdout.log"
    $stderr = Join-Path $LogDirPath "novovm-up.stderr.log"
    $runtime = $State.runtime

    $args = @(
        "-ExecutionPolicy", "Bypass",
        "-File", $UpScriptPath,
        "-Profile", [string]$runtime.profile,
        "-RoleProfile", [string]$runtime.role_profile,
        "-Daemon",
        "-SkipBuild",
        "-GatewayBinaryPath", $releasePaths.gateway_path,
        "-NodeBinaryPath", $releasePaths.node_path,
        "-GatewayBind", [string]$runtime.gateway_bind,
        "-SpoolDir", [string]$runtime.spool_dir,
        "-PollMs", [string]$runtime.poll_ms,
        "-SupervisorPollMs", [string]$runtime.supervisor_poll_ms,
        "-NodeWatchBatchMaxFiles", [string]$runtime.node_watch_batch_max_files,
        "-IdleExitSeconds", [string]$runtime.idle_exit_seconds,
        "-GatewayMaxRequests", [string]$runtime.gateway_max_requests
    )

    if ([bool]$runtime.no_gateway) { $args += "-NoGateway" }
    if ([bool]$runtime.use_node_watch_mode) { $args += "-UseNodeWatchMode" }
    if ([bool]$runtime.lean_io) { $args += "-LeanIo" }
    if ([bool]$runtime.enable_reconcile_daemon) {
        $args += "-EnableReconcileDaemon"
        $args += @("-ReconcileSenderAddress", [string]$runtime.reconcile_sender_address)
        $args += @("-ReconcileRpcEndpoint", [string]$runtime.reconcile_rpc_endpoint)
        $args += @("-ReconcileIntervalSeconds", [string]$runtime.reconcile_interval_seconds)
        $args += @("-ReconcileRestartDelaySeconds", [string]$runtime.reconcile_restart_delay_seconds)
        $args += @("-ReconcileReplayMaxPerPayout", [string]$runtime.reconcile_replay_max_per_payout)
        $args += @("-ReconcileReplayCooldownSec", [string]$runtime.reconcile_replay_cooldown_sec)
    }

    $proc = Start-Process `
        -FilePath "powershell" `
        -ArgumentList $args `
        -WorkingDirectory $RepoRootPath `
        -RedirectStandardOutput $stdout `
        -RedirectStandardError $stderr `
        -PassThru `
        -NoNewWindow `
        -ErrorAction Stop

    Ensure-ParentDir -PathValue $PidFilePath
    [System.IO.File]::WriteAllText($PidFilePath, $proc.Id.ToString(), [System.Text.Encoding]::UTF8)

    Start-Sleep -Seconds $GraceSeconds
    $running = Get-Process -Id $proc.Id -ErrorAction SilentlyContinue
    if ($null -eq $running -or $running.HasExited) {
        throw ("managed process exited during grace period: pid=" + $proc.Id)
    }
    Write-Host ("lifecycle_start: ok=true pid={0} release={1}" -f $proc.Id, $State.current_release)
}

function Read-RuntimeTemplate {
    param(
        [string]$RepoRootPath,
        [string]$TemplatePath
    )
    if ([string]::IsNullOrWhiteSpace($TemplatePath)) {
        throw "runtime template path is required"
    }
    $fullPath = Resolve-FullPath -Root $RepoRootPath -Value $TemplatePath
    if (-not (Test-Path -LiteralPath $fullPath)) {
        throw ("runtime template not found: " + $fullPath)
    }
    $raw = Get-Content -LiteralPath $fullPath -Raw
    if ([string]::IsNullOrWhiteSpace($raw)) {
        throw ("runtime template is empty: " + $fullPath)
    }
    $obj = $raw | ConvertFrom-Json -ErrorAction Stop
    return [pscustomobject]@{
        template_path = $fullPath
        template = $obj
    }
}

function Merge-RuntimeConfig {
    param(
        [pscustomobject]$BaseRuntime,
        [object]$TemplateRuntime,
        [System.Collections.IDictionary]$BoundParameters
    )
    $result = [ordered]@{
        profile = $(if ($null -ne $BaseRuntime.profile) { [string]$BaseRuntime.profile } else { $Profile })
        role_profile = $(if ($null -ne $BaseRuntime.role_profile) { [string]$BaseRuntime.role_profile } else { $RoleProfile })
        no_gateway = $(if ($null -ne $BaseRuntime.no_gateway) { [bool]$BaseRuntime.no_gateway } else { [bool]$NoGateway })
        use_node_watch_mode = $(if ($null -ne $BaseRuntime.use_node_watch_mode) { [bool]$BaseRuntime.use_node_watch_mode } else { [bool]$UseNodeWatchMode })
        lean_io = $(if ($null -ne $BaseRuntime.lean_io) { [bool]$BaseRuntime.lean_io } else { [bool]$LeanIo })
        enable_reconcile_daemon = $(if ($null -ne $BaseRuntime.enable_reconcile_daemon) { [bool]$BaseRuntime.enable_reconcile_daemon } else { [bool]$EnableReconcileDaemon })
        reconcile_sender_address = $(if ($null -ne $BaseRuntime.reconcile_sender_address) { [string]$BaseRuntime.reconcile_sender_address } else { $ReconcileSenderAddress })
        reconcile_rpc_endpoint = $(if ($null -ne $BaseRuntime.reconcile_rpc_endpoint) { [string]$BaseRuntime.reconcile_rpc_endpoint } else { $ReconcileRpcEndpoint })
        reconcile_interval_seconds = $(if ($null -ne $BaseRuntime.reconcile_interval_seconds) { [int]$BaseRuntime.reconcile_interval_seconds } else { $ReconcileIntervalSeconds })
        reconcile_restart_delay_seconds = $(if ($null -ne $BaseRuntime.reconcile_restart_delay_seconds) { [int]$BaseRuntime.reconcile_restart_delay_seconds } else { $ReconcileRestartDelaySeconds })
        reconcile_replay_max_per_payout = $(if ($null -ne $BaseRuntime.reconcile_replay_max_per_payout) { [int]$BaseRuntime.reconcile_replay_max_per_payout } else { $ReconcileReplayMaxPerPayout })
        reconcile_replay_cooldown_sec = $(if ($null -ne $BaseRuntime.reconcile_replay_cooldown_sec) { [int]$BaseRuntime.reconcile_replay_cooldown_sec } else { $ReconcileReplayCooldownSec })
        gateway_bind = $(if ($null -ne $BaseRuntime.gateway_bind) { [string]$BaseRuntime.gateway_bind } else { $GatewayBind })
        spool_dir = $(if ($null -ne $BaseRuntime.spool_dir) { [string]$BaseRuntime.spool_dir } else { $SpoolDir })
        poll_ms = $(if ($null -ne $BaseRuntime.poll_ms) { [int]$BaseRuntime.poll_ms } else { $PollMs })
        supervisor_poll_ms = $(if ($null -ne $BaseRuntime.supervisor_poll_ms) { [int]$BaseRuntime.supervisor_poll_ms } else { $SupervisorPollMs })
        node_watch_batch_max_files = $(if ($null -ne $BaseRuntime.node_watch_batch_max_files) { [int]$BaseRuntime.node_watch_batch_max_files } else { $NodeWatchBatchMaxFiles })
        idle_exit_seconds = $(if ($null -ne $BaseRuntime.idle_exit_seconds) { [int]$BaseRuntime.idle_exit_seconds } else { $IdleExitSeconds })
        gateway_max_requests = $(if ($null -ne $BaseRuntime.gateway_max_requests) { [uint32]$BaseRuntime.gateway_max_requests } else { $GatewayMaxRequests })
    }

    if ($null -ne $TemplateRuntime) {
        $runtimePropNames = @(
            "profile",
            "role_profile",
            "no_gateway",
            "use_node_watch_mode",
            "lean_io",
            "enable_reconcile_daemon",
            "reconcile_sender_address",
            "reconcile_rpc_endpoint",
            "reconcile_interval_seconds",
            "reconcile_restart_delay_seconds",
            "reconcile_replay_max_per_payout",
            "reconcile_replay_cooldown_sec",
            "gateway_bind",
            "spool_dir",
            "poll_ms",
            "supervisor_poll_ms",
            "node_watch_batch_max_files",
            "idle_exit_seconds",
            "gateway_max_requests"
        )
        foreach ($name in $runtimePropNames) {
            $prop = $TemplateRuntime.PSObject.Properties[$name]
            if ($null -ne $prop -and $null -ne $prop.Value) {
                $result[$name] = $prop.Value
            }
        }
    }

    if ($BoundParameters.ContainsKey("Profile")) { $result.profile = $Profile }
    if ($BoundParameters.ContainsKey("RoleProfile")) { $result.role_profile = $RoleProfile }
    if ($BoundParameters.ContainsKey("NoGateway")) { $result.no_gateway = [bool]$NoGateway }
    if ($BoundParameters.ContainsKey("UseNodeWatchMode")) { $result.use_node_watch_mode = [bool]$UseNodeWatchMode }
    if ($BoundParameters.ContainsKey("LeanIo")) { $result.lean_io = [bool]$LeanIo }
    if ($BoundParameters.ContainsKey("EnableReconcileDaemon")) { $result.enable_reconcile_daemon = [bool]$EnableReconcileDaemon }
    if ($BoundParameters.ContainsKey("ReconcileSenderAddress")) { $result.reconcile_sender_address = $ReconcileSenderAddress }
    if ($BoundParameters.ContainsKey("ReconcileRpcEndpoint")) { $result.reconcile_rpc_endpoint = $ReconcileRpcEndpoint }
    if ($BoundParameters.ContainsKey("ReconcileIntervalSeconds")) { $result.reconcile_interval_seconds = $ReconcileIntervalSeconds }
    if ($BoundParameters.ContainsKey("ReconcileRestartDelaySeconds")) { $result.reconcile_restart_delay_seconds = $ReconcileRestartDelaySeconds }
    if ($BoundParameters.ContainsKey("ReconcileReplayMaxPerPayout")) { $result.reconcile_replay_max_per_payout = $ReconcileReplayMaxPerPayout }
    if ($BoundParameters.ContainsKey("ReconcileReplayCooldownSec")) { $result.reconcile_replay_cooldown_sec = $ReconcileReplayCooldownSec }
    if ($BoundParameters.ContainsKey("GatewayBind")) { $result.gateway_bind = $GatewayBind }
    if ($BoundParameters.ContainsKey("SpoolDir")) { $result.spool_dir = $SpoolDir }
    if ($BoundParameters.ContainsKey("PollMs")) { $result.poll_ms = $PollMs }
    if ($BoundParameters.ContainsKey("SupervisorPollMs")) { $result.supervisor_poll_ms = $SupervisorPollMs }
    if ($BoundParameters.ContainsKey("NodeWatchBatchMaxFiles")) { $result.node_watch_batch_max_files = $NodeWatchBatchMaxFiles }
    if ($BoundParameters.ContainsKey("IdleExitSeconds")) { $result.idle_exit_seconds = $IdleExitSeconds }
    if ($BoundParameters.ContainsKey("GatewayMaxRequests")) { $result.gateway_max_requests = $GatewayMaxRequests }

    return [pscustomobject]$result
}

$RepoRootPath = Resolve-RootPath -Root $RepoRoot
$ReleaseRootPath = Resolve-FullPath -Root $RepoRootPath -Value $ReleaseRoot
$RuntimeStatePath = Resolve-FullPath -Root $RepoRootPath -Value $RuntimeStateFile
$RuntimePidPath = Resolve-FullPath -Root $RepoRootPath -Value $RuntimePidFile
$RuntimeLogDirPath = Resolve-FullPath -Root $RepoRootPath -Value $RuntimeLogDir
$UpScriptPath = Resolve-FullPath -Root $RepoRootPath -Value "scripts/novovm-up.ps1"
if (-not (Test-Path -LiteralPath $UpScriptPath)) {
    throw ("novovm-up script not found: " + $UpScriptPath)
}

$runtimeFromParams = New-RuntimeConfigFromParams
$state = Load-State -PathValue $RuntimeStatePath -CurrentReleaseFallback $Version -RuntimeFallback $runtimeFromParams

switch ($Action) {
    "register" {
        if ([string]::IsNullOrWhiteSpace($Version)) {
            throw "register requires -Version"
        }
        if ([string]::IsNullOrWhiteSpace($GatewayBinaryFrom) -or [string]::IsNullOrWhiteSpace($NodeBinaryFrom)) {
            throw "register requires -GatewayBinaryFrom and -NodeBinaryFrom"
        }
        $gatewayFromPath = Resolve-FullPath -Root $RepoRootPath -Value $GatewayBinaryFrom
        $nodeFromPath = Resolve-FullPath -Root $RepoRootPath -Value $NodeBinaryFrom
        if (-not (Test-Path -LiteralPath $gatewayFromPath)) {
            throw ("gateway source binary not found: " + $gatewayFromPath)
        }
        if (-not (Test-Path -LiteralPath $nodeFromPath)) {
            throw ("node source binary not found: " + $nodeFromPath)
        }
        $releasePaths = Resolve-ReleasePaths -Root $ReleaseRootPath -ReleaseVersion $Version
        Ensure-Dir -PathValue $releasePaths.release_dir
        Copy-Item -LiteralPath $gatewayFromPath -Destination $releasePaths.gateway_path -Force
        Copy-Item -LiteralPath $nodeFromPath -Destination $releasePaths.node_path -Force
        if ($SetCurrent -or [string]::IsNullOrWhiteSpace([string]$state.current_release)) {
            if (-not [string]::IsNullOrWhiteSpace([string]$state.current_release) -and $state.current_release -ne $Version) {
                $state.previous_release = [string]$state.current_release
            }
            $state.current_release = $Version
        }
        Save-State -PathValue $RuntimeStatePath -State $state
        Write-Host ("lifecycle_register: ok=true version={0} gateway={1} node={2}" -f $Version, $releasePaths.gateway_path, $releasePaths.node_path)
    }
    "start" {
        if (-not [string]::IsNullOrWhiteSpace($Version)) {
            if (-not [string]::IsNullOrWhiteSpace([string]$state.current_release) -and $state.current_release -ne $Version) {
                $state.previous_release = [string]$state.current_release
            }
            $state.current_release = $Version
        }
        if ([string]::IsNullOrWhiteSpace([string]$state.current_release)) {
            throw "start requires a current release (use -Version or register with -SetCurrent)"
        }
        $templateRuntime = $null
        if (-not [string]::IsNullOrWhiteSpace($RuntimeTemplateFile)) {
            $runtimeTemplateResult = Read-RuntimeTemplate -RepoRootPath $RepoRootPath -TemplatePath $RuntimeTemplateFile
            $templateRuntime = $runtimeTemplateResult.template
        }
        $state.runtime = Merge-RuntimeConfig -BaseRuntime $state.runtime -TemplateRuntime $templateRuntime -BoundParameters $PSBoundParameters
        Save-State -PathValue $RuntimeStatePath -State $state
        Start-ManagedProcess -RepoRootPath $RepoRootPath -UpScriptPath $UpScriptPath -PidFilePath $RuntimePidPath -LogDirPath $RuntimeLogDirPath -State $state -GraceSeconds $StartGraceSeconds
    }
    "stop" {
        Stop-ManagedProcess -PidFilePath $RuntimePidPath
    }
    "status" {
        $proc = Get-ManagedProcess -PidFilePath $RuntimePidPath
        $running = $null -ne $proc -and -not $proc.HasExited
        Write-Host ("lifecycle_status: running={0} pid={1} current_release={2} previous_release={3}" -f $running, $(if ($running) { $proc.Id } else { 0 }), [string]$state.current_release, [string]$state.previous_release)
        $obj = [ordered]@{
            running = $running
            pid = $(if ($running) { $proc.Id } else { 0 })
            current_release = [string]$state.current_release
            previous_release = [string]$state.previous_release
            runtime = $state.runtime
            governance = $state.governance
            state_file = $RuntimeStatePath
            pid_file = $RuntimePidPath
        }
        $obj | ConvertTo-Json -Depth 8
    }
    "upgrade" {
        if ([string]::IsNullOrWhiteSpace($TargetVersion)) {
            throw "upgrade requires -TargetVersion"
        }
        if (-not [string]::IsNullOrWhiteSpace($RequireNodeGroup)) {
            $currentNodeGroup = [string]$state.governance.node_group
            if ($currentNodeGroup -ne $RequireNodeGroup) {
                throw ("upgrade blocked by node group guard: required=" + $RequireNodeGroup + " current=" + $currentNodeGroup)
            }
        }
        $targetRelease = Resolve-ReleasePaths -Root $ReleaseRootPath -ReleaseVersion $TargetVersion
        Assert-ReleaseInstalled -ReleasePaths $targetRelease
        if ([string]::IsNullOrWhiteSpace([string]$state.current_release)) {
            throw "upgrade requires current_release in state"
        }
        $oldRelease = [string]$state.current_release
        Stop-ManagedProcess -PidFilePath $RuntimePidPath
        $state.previous_release = $oldRelease
        $state.current_release = $TargetVersion
        Save-State -PathValue $RuntimeStatePath -State $state

        try {
            Start-ManagedProcess -RepoRootPath $RepoRootPath -UpScriptPath $UpScriptPath -PidFilePath $RuntimePidPath -LogDirPath $RuntimeLogDirPath -State $state -GraceSeconds $StartGraceSeconds
            Start-Sleep -Seconds $UpgradeHealthSeconds
            $proc = Get-ManagedProcess -PidFilePath $RuntimePidPath
            if ($null -eq $proc -or $proc.HasExited) {
                throw "upgrade health check failed: process exited"
            }
            Write-Host ("lifecycle_upgrade: ok=true from={0} to={1} pid={2}" -f $oldRelease, $TargetVersion, $proc.Id)
        } catch {
            $rollbackRelease = [string]$state.previous_release
            if ([string]::IsNullOrWhiteSpace($rollbackRelease)) {
                throw ("upgrade failed without rollback candidate: " + $_.Exception.Message)
            }
            Stop-ManagedProcess -PidFilePath $RuntimePidPath
            $state.current_release = $rollbackRelease
            $state.previous_release = $TargetVersion
            Save-State -PathValue $RuntimeStatePath -State $state
            Start-ManagedProcess -RepoRootPath $RepoRootPath -UpScriptPath $UpScriptPath -PidFilePath $RuntimePidPath -LogDirPath $RuntimeLogDirPath -State $state -GraceSeconds $StartGraceSeconds
            throw ("upgrade failed and rolled back: from=" + $oldRelease + " to=" + $TargetVersion + " rollback=" + $rollbackRelease + " err=" + $_.Exception.Message)
        }
    }
    "rollback" {
        $target = $RollbackVersion
        if ([string]::IsNullOrWhiteSpace($target)) {
            $target = [string]$state.previous_release
        }
        if ([string]::IsNullOrWhiteSpace($target)) {
            throw "rollback requires -RollbackVersion or previous_release in state"
        }
        $targetRelease = Resolve-ReleasePaths -Root $ReleaseRootPath -ReleaseVersion $target
        Assert-ReleaseInstalled -ReleasePaths $targetRelease
        $oldRelease = [string]$state.current_release
        Stop-ManagedProcess -PidFilePath $RuntimePidPath
        $state.current_release = $target
        $state.previous_release = $oldRelease
        Save-State -PathValue $RuntimeStatePath -State $state
        Start-ManagedProcess -RepoRootPath $RepoRootPath -UpScriptPath $UpScriptPath -PidFilePath $RuntimePidPath -LogDirPath $RuntimeLogDirPath -State $state -GraceSeconds $StartGraceSeconds
        Write-Host ("lifecycle_rollback: ok=true from={0} to={1}" -f $oldRelease, $target)
    }
    "set-runtime" {
        $templateRuntime = $null
        $templatePath = ""
        if (-not [string]::IsNullOrWhiteSpace($RuntimeTemplateFile)) {
            $runtimeTemplateResult = Read-RuntimeTemplate -RepoRootPath $RepoRootPath -TemplatePath $RuntimeTemplateFile
            $templateRuntime = $runtimeTemplateResult.template
            $templatePath = $runtimeTemplateResult.template_path
        }
        $state.runtime = Merge-RuntimeConfig -BaseRuntime $state.runtime -TemplateRuntime $templateRuntime -BoundParameters $PSBoundParameters
        Save-State -PathValue $RuntimeStatePath -State $state
        Write-Host ("lifecycle_set_runtime: ok=true template={0}" -f $templatePath)
        if ($RestartAfterSetRuntime) {
            if ([string]::IsNullOrWhiteSpace([string]$state.current_release)) {
                throw "set-runtime with -RestartAfterSetRuntime requires current_release"
            }
            Stop-ManagedProcess -PidFilePath $RuntimePidPath
            Start-ManagedProcess -RepoRootPath $RepoRootPath -UpScriptPath $UpScriptPath -PidFilePath $RuntimePidPath -LogDirPath $RuntimeLogDirPath -State $state -GraceSeconds $StartGraceSeconds
            Write-Host "lifecycle_set_runtime_restart: ok=true"
        }
    }
    "set-policy" {
        if (-not $PSBoundParameters.ContainsKey("NodeGroup") -and -not $PSBoundParameters.ContainsKey("UpgradeWindow")) {
            throw "set-policy requires -NodeGroup or -UpgradeWindow"
        }
        if ($PSBoundParameters.ContainsKey("NodeGroup")) {
            $state.governance.node_group = $NodeGroup
        }
        if ($PSBoundParameters.ContainsKey("UpgradeWindow")) {
            $state.governance.upgrade_window = $UpgradeWindow
        }
        Save-State -PathValue $RuntimeStatePath -State $state
        Write-Host ("lifecycle_set_policy: ok=true node_group={0} upgrade_window={1}" -f [string]$state.governance.node_group, [string]$state.governance.upgrade_window)
    }
    default {
        throw ("unknown action: " + $Action)
    }
}
