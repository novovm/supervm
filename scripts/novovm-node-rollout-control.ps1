[CmdletBinding()]
param(
    [ValidateSet("upgrade", "rollback", "status", "set-policy")]
    [string]$PlanAction = "upgrade",
    [string]$QueueFile = "config/runtime/lifecycle/rollout.queue.json",
    [ValidateRange(1, 64)]
    [int]$MaxConcurrentPlans = 1,
    [ValidateRange(1, 300)]
    [int]$PollSeconds = 2,
    [ValidateRange(0, 600)]
    [int]$DispatchPauseSeconds = 1,
    [string]$GlobalTargetVersion = "",
    [string]$GlobalRollbackVersion = "",
    [string]$ControllerId = "ops-main",
    [string]$OperationId = "",
    [string]$AuditFile = "artifacts/runtime/rollout/control-plane-audit.jsonl",
    [switch]$IgnoreRegionWindow,
    [switch]$ContinueOnPlanFailure,
    [switch]$DryRun
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
    if (-not [string]::IsNullOrWhiteSpace($parent)) {
        New-Item -ItemType Directory -Force -Path $parent | Out-Null
    }
}

function Resolve-OperationId {
    param([string]$Raw)
    if (-not [string]::IsNullOrWhiteSpace($Raw)) {
        return $Raw
    }
    return ("control-" + [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds() + "-" + $PID)
}

function Write-AuditRecord {
    param(
        [string]$AuditPath,
        [pscustomobject]$Record
    )
    Ensure-ParentDir -PathValue $AuditPath
    $line = $Record | ConvertTo-Json -Compress -Depth 12
    Add-Content -LiteralPath $AuditPath -Value $line -Encoding UTF8
}

function Load-Queue {
    param(
        [string]$RepoRootPath,
        [string]$QueueFilePath
    )
    $fullPath = Resolve-FullPath -Root $RepoRootPath -Value $QueueFilePath
    if (-not (Test-Path -LiteralPath $fullPath)) {
        throw ("rollout queue not found: " + $fullPath)
    }
    $raw = Get-Content -LiteralPath $fullPath -Raw
    if ([string]::IsNullOrWhiteSpace($raw)) {
        throw ("rollout queue is empty: " + $fullPath)
    }
    $obj = $raw | ConvertFrom-Json -ErrorAction Stop
    if ($null -eq $obj.plans -or $obj.plans.Count -eq 0) {
        throw ("rollout queue has no plans: " + $fullPath)
    }
    return [pscustomobject]@{
        queue_path = $fullPath
        queue = $obj
    }
}

function Test-TimeWindowUtc {
    param([string]$WindowRaw)
    if ([string]::IsNullOrWhiteSpace($WindowRaw)) {
        return [pscustomobject]@{
            has_window = $false
            in_window = $true
            reason = "no window"
        }
    }
    $m = [regex]::Match($WindowRaw, "^\s*(\d{2}):(\d{2})-(\d{2}):(\d{2})\s*UTC\s*$")
    if (-not $m.Success) {
        return [pscustomobject]@{
            has_window = $true
            in_window = $false
            reason = ("invalid format: " + $WindowRaw)
        }
    }
    $sh = [int]$m.Groups[1].Value
    $sm = [int]$m.Groups[2].Value
    $eh = [int]$m.Groups[3].Value
    $em = [int]$m.Groups[4].Value
    if ($sh -gt 23 -or $eh -gt 23 -or $sm -gt 59 -or $em -gt 59) {
        return [pscustomobject]@{
            has_window = $true
            in_window = $false
            reason = ("invalid value: " + $WindowRaw)
        }
    }
    $start = ($sh * 60) + $sm
    $end = ($eh * 60) + $em
    $now = [DateTime]::UtcNow
    $nowMin = ($now.Hour * 60) + $now.Minute
    if ($start -eq $end) {
        return [pscustomobject]@{
            has_window = $true
            in_window = $true
            reason = "full day"
        }
    }
    $inWindow = $false
    if ($start -lt $end) {
        $inWindow = ($nowMin -ge $start -and $nowMin -lt $end)
    } else {
        $inWindow = ($nowMin -ge $start -or $nowMin -lt $end)
    }
    return [pscustomobject]@{
        has_window = $true
        in_window = $inWindow
        reason = ("window=" + $WindowRaw + " now_utc=" + $now.ToString("HH:mm"))
    }
}

function Start-PlanProcess {
    param(
        [string]$RepoRootPath,
        [string]$RolloutScriptPath,
        [object]$Plan,
        [string]$BaseAction,
        [string]$GlobalTarget,
        [string]$GlobalRollback,
        [string]$DefaultControllerId,
        [string]$DefaultOpId,
        [string]$DefaultAuditFile
    )
    $planName = [string]$Plan.name
    if ([string]::IsNullOrWhiteSpace($planName)) {
        throw "plan.name is required"
    }
    $planFileRaw = [string]$Plan.plan_file
    if ([string]::IsNullOrWhiteSpace($planFileRaw)) {
        throw ("plan_file is required, plan=" + $planName)
    }
    $planFilePath = Resolve-FullPath -Root $RepoRootPath -Value $planFileRaw
    $planAction = $BaseAction
    if ($null -ne $Plan.action -and -not [string]::IsNullOrWhiteSpace([string]$Plan.action)) {
        $planAction = [string]$Plan.action
    }
    $targetVersion = $GlobalTarget
    if ([string]::IsNullOrWhiteSpace($targetVersion) -and $null -ne $Plan.target_version) {
        $targetVersion = [string]$Plan.target_version
    }
    $rollbackVersion = $GlobalRollback
    if ([string]::IsNullOrWhiteSpace($rollbackVersion) -and $null -ne $Plan.rollback_version) {
        $rollbackVersion = [string]$Plan.rollback_version
    }
    $controller = $DefaultControllerId
    if ($null -ne $Plan.controller_id -and -not [string]::IsNullOrWhiteSpace([string]$Plan.controller_id)) {
        $controller = [string]$Plan.controller_id
    }
    $operation = $DefaultOpId + "-" + $planName
    if ($null -ne $Plan.operation_id -and -not [string]::IsNullOrWhiteSpace([string]$Plan.operation_id)) {
        $operation = [string]$Plan.operation_id
    }
    $auditFile = $DefaultAuditFile
    if ($null -ne $Plan.audit_file -and -not [string]::IsNullOrWhiteSpace([string]$Plan.audit_file)) {
        $auditFile = [string]$Plan.audit_file
    }

    if ($planAction -eq "upgrade" -and [string]::IsNullOrWhiteSpace($targetVersion)) {
        throw ("upgrade plan missing target version, plan=" + $planName)
    }

    $args = @(
        "-ExecutionPolicy", "Bypass",
        "-File", $RolloutScriptPath,
        "-Action", $planAction,
        "-PlanFile", $planFilePath,
        "-ControllerId", $controller,
        "-OperationId", $operation,
        "-AuditFile", (Resolve-FullPath -Root $RepoRootPath -Value $auditFile)
    )
    if (-not [string]::IsNullOrWhiteSpace($targetVersion)) {
        $args += @("-TargetVersion", $targetVersion)
    }
    if (-not [string]::IsNullOrWhiteSpace($rollbackVersion)) {
        $args += @("-RollbackVersion", $rollbackVersion)
    }
    if ($null -ne $Plan.upgrade_health_seconds) {
        $args += @("-UpgradeHealthSeconds", [string][int]$Plan.upgrade_health_seconds)
    }
    if ($null -ne $Plan.default_max_failures) {
        $args += @("-DefaultMaxFailures", [string][int]$Plan.default_max_failures)
    }
    if ($null -ne $Plan.pause_seconds_between_nodes) {
        $args += @("-PauseSecondsBetweenNodes", [string][int]$Plan.pause_seconds_between_nodes)
    }
    if ($null -ne $Plan.default_transport -and -not [string]::IsNullOrWhiteSpace([string]$Plan.default_transport)) {
        $args += @("-DefaultTransport", [string]$Plan.default_transport)
    }
    if ($null -ne $Plan.ssh_binary -and -not [string]::IsNullOrWhiteSpace([string]$Plan.ssh_binary)) {
        $args += @("-SshBinary", [string]$Plan.ssh_binary)
    }
    if ($null -ne $Plan.remote_timeout_seconds) {
        $args += @("-RemoteTimeoutSeconds", [string][int]$Plan.remote_timeout_seconds)
    }
    if ($null -ne $Plan.auto_rollback_on_failure -and [bool]$Plan.auto_rollback_on_failure) {
        $args += "-AutoRollbackOnFailure"
    }
    if ($null -ne $Plan.continue_on_failure -and [bool]$Plan.continue_on_failure) {
        $args += "-ContinueOnFailure"
    }
    if ($null -ne $Plan.ignore_upgrade_window -and [bool]$Plan.ignore_upgrade_window) {
        $args += "-IgnoreUpgradeWindow"
    }
    if ($DryRun) {
        $args += "-DryRun"
    }

    $logRoot = Join-Path $RepoRootPath "artifacts/runtime/rollout/control-plane-logs"
    New-Item -ItemType Directory -Force -Path $logRoot | Out-Null
    $stdout = Join-Path $logRoot ($operation + ".stdout.log")
    $stderr = Join-Path $logRoot ($operation + ".stderr.log")

    $proc = Start-Process -FilePath "powershell" -ArgumentList $args -WorkingDirectory $RepoRootPath -RedirectStandardOutput $stdout -RedirectStandardError $stderr -PassThru -NoNewWindow -ErrorAction Stop
    return [pscustomobject]@{
        plan_name = $planName
        operation_id = $operation
        action = $planAction
        target_version = $targetVersion
        rollback_version = $rollbackVersion
        process = $proc
        stdout_log = $stdout
        stderr_log = $stderr
    }
}

$RepoRootPath = Resolve-RootPath -Root ""
$RolloutScriptPath = Join-Path $RepoRootPath "scripts/novovm-node-rollout.ps1"
if (-not (Test-Path -LiteralPath $RolloutScriptPath)) {
    throw ("rollout script not found: " + $RolloutScriptPath)
}

$AuditFilePath = Resolve-FullPath -Root $RepoRootPath -Value $AuditFile
$CurrentOperationId = Resolve-OperationId -Raw $OperationId
$queueResult = Load-Queue -RepoRootPath $RepoRootPath -QueueFilePath $QueueFile
$QueuePath = $queueResult.queue_path
$queue = $queueResult.queue

$effectiveConcurrent = $MaxConcurrentPlans
if ($null -ne $queue.max_concurrent_plans) {
    $effectiveConcurrent = [Math]::Max(1, [int]$queue.max_concurrent_plans)
}
$effectivePoll = $PollSeconds
if ($null -ne $queue.poll_seconds) {
    $effectivePoll = [Math]::Max(1, [int]$queue.poll_seconds)
}
$effectivePause = $DispatchPauseSeconds
if ($null -ne $queue.dispatch_pause_seconds) {
    $effectivePause = [Math]::Max(0, [int]$queue.dispatch_pause_seconds)
}

Write-Host ("rollout_control_in: queue={0} action={1} max_concurrent={2} controller={3} operation={4}" -f $QueuePath, $PlanAction, $effectiveConcurrent, $ControllerId, $CurrentOperationId)

$pendingPlans = @()
foreach ($plan in $queue.plans) {
    $enabled = $true
    if ($null -ne $plan.enabled) {
        $enabled = [bool]$plan.enabled
    }
    if (-not $enabled) {
        continue
    }
    if (-not $IgnoreRegionWindow) {
        $windowRaw = ""
        if ($null -ne $plan.region_window) {
            $windowRaw = [string]$plan.region_window
        }
        $windowCheck = Test-TimeWindowUtc -WindowRaw $windowRaw
        if (-not $windowCheck.in_window) {
            $planName = [string]$plan.name
            if ([string]::IsNullOrWhiteSpace($planName)) {
                $planName = "(unnamed-plan)"
            }
            $msg = ("blocked by region window: " + $windowCheck.reason)
            Write-Host ("rollout_control_skip: plan={0} reason={1}" -f $planName, $msg)
            Write-AuditRecord -AuditPath $AuditFilePath -Record ([pscustomobject][ordered]@{
                    timestamp_utc = [DateTime]::UtcNow.ToString("o")
                    operation_id = $CurrentOperationId
                    controller_id = $ControllerId
                    queue_file = $QueuePath
                    plan = $planName
                    action = $PlanAction
                    result = "blocked"
                    error = $msg
                })
            continue
        }
    }
    $pendingPlans += $plan
}

$running = @()
$doneOk = 0
$doneErr = 0
$stopDispatch = $false

while (($pendingPlans.Count -gt 0 -and -not $stopDispatch) -or $running.Count -gt 0) {
    while (-not $stopDispatch -and $running.Count -lt $effectiveConcurrent -and $pendingPlans.Count -gt 0) {
        $plan = $pendingPlans[0]
        if ($pendingPlans.Count -eq 1) {
            $pendingPlans = @()
        } else {
            $pendingPlans = $pendingPlans[1..($pendingPlans.Count - 1)]
        }
        $planName = [string]$plan.name
        if ([string]::IsNullOrWhiteSpace($planName)) {
            $planName = "(unnamed-plan)"
        }
        try {
            $job = Start-PlanProcess -RepoRootPath $RepoRootPath -RolloutScriptPath $RolloutScriptPath -Plan $plan -BaseAction $PlanAction -GlobalTarget $GlobalTargetVersion -GlobalRollback $GlobalRollbackVersion -DefaultControllerId $ControllerId -DefaultOpId $CurrentOperationId -DefaultAuditFile $AuditFile
            $running += $job
            Write-Host ("rollout_control_dispatch: plan={0} pid={1} action={2}" -f $job.plan_name, $job.process.Id, $job.action)
            Write-AuditRecord -AuditPath $AuditFilePath -Record ([pscustomobject][ordered]@{
                    timestamp_utc = [DateTime]::UtcNow.ToString("o")
                    operation_id = $CurrentOperationId
                    controller_id = $ControllerId
                    queue_file = $QueuePath
                    plan = $job.plan_name
                    plan_operation_id = $job.operation_id
                    action = $job.action
                    result = "dispatched"
                    error = ""
                })
        } catch {
            $doneErr += 1
            $msg = $_.Exception.Message
            Write-Host ("rollout_control_error: plan={0} err={1}" -f $planName, $msg)
            Write-AuditRecord -AuditPath $AuditFilePath -Record ([pscustomobject][ordered]@{
                    timestamp_utc = [DateTime]::UtcNow.ToString("o")
                    operation_id = $CurrentOperationId
                    controller_id = $ControllerId
                    queue_file = $QueuePath
                    plan = $planName
                    action = $PlanAction
                    result = "error"
                    error = $msg
                })
            if (-not $ContinueOnPlanFailure) {
                $stopDispatch = $true
            }
        }
        if ($effectivePause -gt 0) {
            Start-Sleep -Seconds $effectivePause
        }
    }

    if ($running.Count -eq 0) {
        continue
    }

    $stillRunning = @()
    foreach ($job in $running) {
        $p = Get-Process -Id $job.process.Id -ErrorAction SilentlyContinue
        if ($null -eq $p -or $p.HasExited) {
            $exitCode = 1
            try {
                $exitCode = $job.process.ExitCode
            } catch {
                $exitCode = 1
            }
            if ($exitCode -eq 0) {
                $doneOk += 1
                Write-Host ("rollout_control_done: plan={0} result=ok" -f $job.plan_name)
                Write-AuditRecord -AuditPath $AuditFilePath -Record ([pscustomobject][ordered]@{
                        timestamp_utc = [DateTime]::UtcNow.ToString("o")
                        operation_id = $CurrentOperationId
                        controller_id = $ControllerId
                        queue_file = $QueuePath
                        plan = $job.plan_name
                        plan_operation_id = $job.operation_id
                        action = $job.action
                        result = "ok"
                        error = ""
                        stdout_log = $job.stdout_log
                        stderr_log = $job.stderr_log
                    })
            } else {
                $doneErr += 1
                $msg = ("exit_code=" + $exitCode)
                Write-Host ("rollout_control_done: plan={0} result=error {1}" -f $job.plan_name, $msg)
                Write-AuditRecord -AuditPath $AuditFilePath -Record ([pscustomobject][ordered]@{
                        timestamp_utc = [DateTime]::UtcNow.ToString("o")
                        operation_id = $CurrentOperationId
                        controller_id = $ControllerId
                        queue_file = $QueuePath
                        plan = $job.plan_name
                        plan_operation_id = $job.operation_id
                        action = $job.action
                        result = "error"
                        error = $msg
                        stdout_log = $job.stdout_log
                        stderr_log = $job.stderr_log
                    })
                if (-not $ContinueOnPlanFailure) {
                    $stopDispatch = $true
                }
            }
        } else {
            $stillRunning += $job
        }
    }
    $running = $stillRunning
    if ($running.Count -gt 0) {
        Start-Sleep -Seconds $effectivePoll
    }
}

Write-Host ("rollout_control_out: ok={0} err={1} operation={2}" -f $doneOk, $doneErr, $CurrentOperationId)
if ($doneErr -gt 0) {
    throw ("rollout control completed with errors: " + $doneErr)
}
