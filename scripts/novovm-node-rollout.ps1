[CmdletBinding()]
param(
    [ValidateSet("upgrade", "rollback", "status", "set-policy")]
    [string]$Action = "upgrade",
    [string]$PlanFile = "config/runtime/lifecycle/rollout.plan.json",
    [string]$TargetVersion = "",
    [string]$RollbackVersion = "",
    [string[]]$GroupOrder = @("canary", "stable"),
    [ValidateRange(1, 600)]
    [int]$UpgradeHealthSeconds = 12,
    [ValidateRange(0, 100)]
    [int]$DefaultMaxFailures = 0,
    [ValidateRange(0, 600)]
    [int]$PauseSecondsBetweenNodes = 3,
    [ValidateSet("local", "ssh", "winrm")]
    [string]$DefaultTransport = "local",
    [string]$SshBinary = "ssh",
    [string]$SshIdentityFile = "",
    [string]$SshKnownHostsFile = "",
    [ValidateSet("accept-new", "yes", "no")]
    [string]$SshStrictHostKeyChecking = "accept-new",
    [ValidateRange(1, 3600)]
    [int]$RemoteTimeoutSeconds = 30,
    [string]$RemoteShell = "powershell",
    [string]$WinRmCredentialUserEnv = "",
    [string]$WinRmCredentialPasswordEnv = "",
    [string]$ControllerId = "local-controller",
    [string]$OperationId = "",
    [string]$AuditFile = "artifacts/runtime/rollout/audit.jsonl",
    [switch]$IgnoreUpgradeWindow,
    [switch]$AutoRollbackOnFailure,
    [switch]$ContinueOnFailure,
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

function Escape-SingleQuoted {
    param([string]$Value)
    if ($null -eq $Value) {
        return ""
    }
    return $Value.Replace("'", "''")
}

function Convert-ToCliLiteral {
    param([object]$Value)
    if ($null -eq $Value) {
        return "''"
    }
    if ($Value -is [bool]) {
        if ([bool]$Value) {
            return "'true'"
        }
        return "'false'"
    }
    return ("'" + (Escape-SingleQuoted -Value ([string]$Value)) + "'")
}

function Convert-HashtableToCliArgumentString {
    param([hashtable]$Table)
    $parts = @()
    foreach ($k in $Table.Keys) {
        $parts += ("-" + $k + " " + (Convert-ToCliLiteral -Value $Table[$k]))
    }
    return ($parts -join " ")
}

function Get-NowUnixMs {
    return [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
}

function Resolve-OperationId {
    param([string]$Raw)
    if (-not [string]::IsNullOrWhiteSpace($Raw)) {
        return $Raw
    }
    return ("op-" + (Get-NowUnixMs) + "-" + $PID)
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

function Get-NodeTransport {
    param(
        [object]$Node,
        [string]$FallbackTransport
    )
    $transport = $FallbackTransport
    if ($null -ne $Node.transport -and -not [string]::IsNullOrWhiteSpace([string]$Node.transport)) {
        $transport = [string]$Node.transport
    } elseif ($null -ne $Node.remote_mode -and -not [string]::IsNullOrWhiteSpace([string]$Node.remote_mode)) {
        $transport = [string]$Node.remote_mode
    }
    $transport = $transport.ToLowerInvariant()
    if ($transport -ne "local" -and $transport -ne "ssh" -and $transport -ne "winrm") {
        throw ("unsupported node transport: " + $transport)
    }
    return $transport
}

function Load-Plan {
    param(
        [string]$RepoRootPath,
        [string]$PlanFilePath
    )
    $fullPath = Resolve-FullPath -Root $RepoRootPath -Value $PlanFilePath
    if (-not (Test-Path -LiteralPath $fullPath)) {
        throw ("rollout plan not found: " + $fullPath)
    }
    $raw = Get-Content -LiteralPath $fullPath -Raw
    if ([string]::IsNullOrWhiteSpace($raw)) {
        throw ("rollout plan is empty: " + $fullPath)
    }
    $obj = $raw | ConvertFrom-Json -ErrorAction Stop
    if ($null -eq $obj.nodes -or $obj.nodes.Count -eq 0) {
        throw ("rollout plan has no nodes: " + $fullPath)
    }
    return [pscustomobject]@{
        plan_path = $fullPath
        plan = $obj
    }
}

function Assert-ControllerAuthorized {
    param(
        [object]$Plan,
        [string]$CurrentControllerId
    )
    if ([string]::IsNullOrWhiteSpace($CurrentControllerId)) {
        throw "ControllerId cannot be empty"
    }
    if ($null -eq $Plan.controllers -or $null -eq $Plan.controllers.allowed_ids) {
        return
    }
    $allowed = @()
    foreach ($id in $Plan.controllers.allowed_ids) {
        if (-not [string]::IsNullOrWhiteSpace([string]$id)) {
            $allowed += [string]$id
        }
    }
    if ($allowed.Count -eq 0) {
        return
    }
    if (-not ($allowed -contains $CurrentControllerId)) {
        throw ("controller is not authorized by plan: controller_id=" + $CurrentControllerId)
    }
}

function Get-GroupList {
    param(
        [object]$Plan,
        [string[]]$DefaultGroups
    )
    if ($null -ne $Plan.group_order -and $Plan.group_order.Count -gt 0) {
        return @($Plan.group_order)
    }
    return @($DefaultGroups)
}

function Get-GroupMaxFailures {
    param(
        [object]$Plan,
        [string]$GroupName,
        [int]$Fallback
    )
    if ($null -eq $Plan.groups) {
        return $Fallback
    }
    foreach ($g in $Plan.groups) {
        if ([string]$g.name -eq $GroupName) {
            if ($null -ne $g.max_failures) {
                return [int]$g.max_failures
            }
            break
        }
    }
    return $Fallback
}

function Get-NodeListByGroup {
    param(
        [object]$Plan,
        [string]$GroupName
    )
    $result = @()
    foreach ($n in $Plan.nodes) {
        $enabled = $true
        if ($null -ne $n.enabled) {
            $enabled = [bool]$n.enabled
        }
        if (-not $enabled) {
            continue
        }
        $nodeGroup = "stable"
        if ($null -ne $n.node_group -and -not [string]::IsNullOrWhiteSpace([string]$n.node_group)) {
            $nodeGroup = [string]$n.node_group
        }
        if ($nodeGroup -eq $GroupName) {
            $result += $n
        }
    }
    return ,$result
}

function Test-NodeUpgradeWindow {
    param([object]$Node)
    $windowRaw = [string]$Node.upgrade_window
    if ([string]::IsNullOrWhiteSpace($windowRaw)) {
        return [pscustomobject]@{
            has_window = $false
            in_window = $true
            reason = "no window"
        }
    }
    $m = [regex]::Match($windowRaw, "^\s*(\d{2}):(\d{2})-(\d{2}):(\d{2})\s*UTC\s*$")
    if (-not $m.Success) {
        return [pscustomobject]@{
            has_window = $true
            in_window = $false
            reason = ("invalid window format: " + $windowRaw)
        }
    }
    $startHour = [int]$m.Groups[1].Value
    $startMin = [int]$m.Groups[2].Value
    $endHour = [int]$m.Groups[3].Value
    $endMin = [int]$m.Groups[4].Value
    if ($startHour -gt 23 -or $endHour -gt 23 -or $startMin -gt 59 -or $endMin -gt 59) {
        return [pscustomobject]@{
            has_window = $true
            in_window = $false
            reason = ("invalid window value: " + $windowRaw)
        }
    }
    $startTotal = ($startHour * 60) + $startMin
    $endTotal = ($endHour * 60) + $endMin
    $now = [DateTime]::UtcNow
    $nowTotal = ($now.Hour * 60) + $now.Minute

    if ($startTotal -eq $endTotal) {
        return [pscustomobject]@{
            has_window = $true
            in_window = $true
            reason = "full day window"
        }
    }

    $allowed = $false
    if ($startTotal -lt $endTotal) {
        $allowed = ($nowTotal -ge $startTotal -and $nowTotal -lt $endTotal)
    } else {
        $allowed = ($nowTotal -ge $startTotal -or $nowTotal -lt $endTotal)
    }
    return [pscustomobject]@{
        has_window = $true
        in_window = $allowed
        reason = ("window=" + $windowRaw + " now_utc=" + $now.ToString("HH:mm"))
    }
}

function Resolve-WinRmCredential {
    param([object]$Node)
    $userEnvName = ""
    $passEnvName = ""

    if ($null -ne $Node.winrm_cred_user_env -and -not [string]::IsNullOrWhiteSpace([string]$Node.winrm_cred_user_env)) {
        $userEnvName = [string]$Node.winrm_cred_user_env
    } else {
        $userEnvName = $WinRmCredentialUserEnv
    }
    if ($null -ne $Node.winrm_cred_pass_env -and -not [string]::IsNullOrWhiteSpace([string]$Node.winrm_cred_pass_env)) {
        $passEnvName = [string]$Node.winrm_cred_pass_env
    } else {
        $passEnvName = $WinRmCredentialPasswordEnv
    }

    if ([string]::IsNullOrWhiteSpace($userEnvName) -and [string]::IsNullOrWhiteSpace($passEnvName)) {
        return $null
    }
    if ([string]::IsNullOrWhiteSpace($userEnvName) -or [string]::IsNullOrWhiteSpace($passEnvName)) {
        throw "winrm credential env requires both user env and password env"
    }

    $userItem = Get-Item -Path ("Env:" + $userEnvName) -ErrorAction SilentlyContinue
    $passItem = Get-Item -Path ("Env:" + $passEnvName) -ErrorAction SilentlyContinue
    if ($null -eq $userItem -or [string]::IsNullOrWhiteSpace($userItem.Value)) {
        throw ("winrm user env missing: " + $userEnvName)
    }
    if ($null -eq $passItem -or [string]::IsNullOrWhiteSpace($passItem.Value)) {
        throw ("winrm password env missing: " + $passEnvName)
    }

    $secure = ConvertTo-SecureString -String $passItem.Value -AsPlainText -Force
    return New-Object System.Management.Automation.PSCredential ($userItem.Value, $secure)
}

function Invoke-LifecycleAction {
    param(
        [string]$RepoRootPath,
        [string]$AuditPath,
        [string]$CurrentOperationId,
        [string]$CurrentControllerId,
        [object]$Node,
        [string]$NodeGroup,
        [string]$LifecycleAction,
        [hashtable]$ExtraArgs
    )
    $nodeName = [string]$Node.name
    if ([string]::IsNullOrWhiteSpace($nodeName)) {
        if ($null -ne $Node.remote_host -and -not [string]::IsNullOrWhiteSpace([string]$Node.remote_host)) {
            $nodeName = [string]$Node.remote_host
        } else {
            $nodeName = [string]$Node.repo_root
        }
    }
    $transport = Get-NodeTransport -Node $Node -FallbackTransport $DefaultTransport
    $invokeArgs = @{}
    if ($null -ne $ExtraArgs) {
        foreach ($k in $ExtraArgs.Keys) {
            $invokeArgs[$k] = $ExtraArgs[$k]
        }
    }

    if (-not $IgnoreUpgradeWindow -and $LifecycleAction -eq "upgrade") {
        $windowCheck = Test-NodeUpgradeWindow -Node $Node
        if (-not $windowCheck.in_window) {
            $msg = ("upgrade blocked by window gate: " + $windowCheck.reason)
            Write-Host ("rollout_error: node={0} transport={1} action={2} err={3}" -f $nodeName, $transport, $LifecycleAction, $msg)
            Write-AuditRecord -AuditPath $AuditPath -Record ([pscustomobject][ordered]@{
                    timestamp_utc = [DateTime]::UtcNow.ToString("o")
                    operation_id = $CurrentOperationId
                    controller_id = $CurrentControllerId
                    action = $Action
                    lifecycle_action = $LifecycleAction
                    node_group = $NodeGroup
                    node = $nodeName
                    transport = $transport
                    target_version = [string]$invokeArgs["TargetVersion"]
                    rollback_version = [string]$invokeArgs["RollbackVersion"]
                    result = "blocked"
                    error = $msg
                    dry_run = [bool]$DryRun
                })
            return $false
        }
    }

    if ($DryRun) {
        $kv = @()
        foreach ($k in $invokeArgs.Keys) {
            $kv += ($k + "=" + [string]$invokeArgs[$k])
        }
        Write-Host ("rollout_dryrun: node={0} transport={1} action={2} args={3}" -f $nodeName, $transport, $LifecycleAction, ($kv -join ";"))
        Write-AuditRecord -AuditPath $AuditPath -Record ([pscustomobject][ordered]@{
                timestamp_utc = [DateTime]::UtcNow.ToString("o")
                operation_id = $CurrentOperationId
                controller_id = $CurrentControllerId
                action = $Action
                lifecycle_action = $LifecycleAction
                node_group = $NodeGroup
                node = $nodeName
                transport = $transport
                target_version = [string]$invokeArgs["TargetVersion"]
                rollback_version = [string]$invokeArgs["RollbackVersion"]
                result = "dryrun"
                error = ""
                dry_run = $true
            })
        return $true
    }

    $ok = $false
    $err = ""

    try {
        switch ($transport) {
            "local" {
                if ([string]::IsNullOrWhiteSpace([string]$Node.repo_root)) {
                    throw ("node repo_root is required for local transport, node=" + $nodeName)
                }
                $nodeRepoRoot = Resolve-FullPath -Root $RepoRootPath -Value ([string]$Node.repo_root)
                $lifecycleScript = Join-Path $nodeRepoRoot "scripts/novovm-node-lifecycle.ps1"
                if (-not (Test-Path -LiteralPath $lifecycleScript)) {
                    throw ("lifecycle script not found for node=" + $nodeName + ": " + $lifecycleScript)
                }
                $localArgs = @{
                    Action = $LifecycleAction
                    RepoRoot = $nodeRepoRoot
                }
                foreach ($k in $invokeArgs.Keys) {
                    $localArgs[$k] = $invokeArgs[$k]
                }
                & $lifecycleScript @localArgs | Out-Host
            }
            "ssh" {
                if ([string]::IsNullOrWhiteSpace([string]$Node.remote_host)) {
                    throw ("node remote_host is required for ssh transport, node=" + $nodeName)
                }
                $remoteRepoRoot = [string]$Node.remote_repo_root
                if ([string]::IsNullOrWhiteSpace($remoteRepoRoot)) {
                    $remoteRepoRoot = [string]$Node.repo_root
                }
                if ([string]::IsNullOrWhiteSpace($remoteRepoRoot)) {
                    throw ("node remote_repo_root or repo_root is required for ssh transport, node=" + $nodeName)
                }
                $remoteShellExec = $RemoteShell
                if ($null -ne $Node.remote_shell -and -not [string]::IsNullOrWhiteSpace([string]$Node.remote_shell)) {
                    $remoteShellExec = [string]$Node.remote_shell
                }
                $remoteScriptPath = [string]$Node.lifecycle_script_path
                if ([string]::IsNullOrWhiteSpace($remoteScriptPath)) {
                    if ($remoteRepoRoot.Contains("\")) {
                        $remoteScriptPath = ($remoteRepoRoot.TrimEnd("\") + "\scripts\novovm-node-lifecycle.ps1")
                    } else {
                        $remoteScriptPath = ($remoteRepoRoot.TrimEnd("/") + "/scripts/novovm-node-lifecycle.ps1")
                    }
                }

                $sshArgsMap = @{
                    Action = $LifecycleAction
                    RepoRoot = $remoteRepoRoot
                }
                foreach ($k in $invokeArgs.Keys) {
                    $sshArgsMap[$k] = $invokeArgs[$k]
                }
                $argString = Convert-HashtableToCliArgumentString -Table $sshArgsMap
                $remoteBody = "& '" + (Escape-SingleQuoted -Value $remoteScriptPath) + "' " + $argString
                $bytes = [System.Text.Encoding]::Unicode.GetBytes($remoteBody)
                $encoded = [Convert]::ToBase64String($bytes)
                $remoteCommand = $remoteShellExec + " -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand " + $encoded

                $target = [string]$Node.remote_host
                if ($null -ne $Node.remote_user -and -not [string]::IsNullOrWhiteSpace([string]$Node.remote_user)) {
                    $target = ([string]$Node.remote_user) + "@" + $target
                }

                $identityFile = $SshIdentityFile
                if ($null -ne $Node.ssh_identity_file -and -not [string]::IsNullOrWhiteSpace([string]$Node.ssh_identity_file)) {
                    $identityFile = [string]$Node.ssh_identity_file
                }
                $knownHostsFile = $SshKnownHostsFile
                if ($null -ne $Node.ssh_known_hosts_file -and -not [string]::IsNullOrWhiteSpace([string]$Node.ssh_known_hosts_file)) {
                    $knownHostsFile = [string]$Node.ssh_known_hosts_file
                }
                $strictHostKey = $SshStrictHostKeyChecking
                if ($null -ne $Node.ssh_strict_host_key -and -not [string]::IsNullOrWhiteSpace([string]$Node.ssh_strict_host_key)) {
                    $strictHostKey = ([string]$Node.ssh_strict_host_key).ToLowerInvariant()
                }
                if ($strictHostKey -ne "accept-new" -and $strictHostKey -ne "yes" -and $strictHostKey -ne "no") {
                    throw ("invalid ssh strict host key mode: " + $strictHostKey)
                }

                $sshCall = @()
                if ($null -ne $Node.remote_port -and -not [string]::IsNullOrWhiteSpace([string]$Node.remote_port)) {
                    $sshCall += @("-p", [string]$Node.remote_port)
                }
                if ($RemoteTimeoutSeconds -gt 0) {
                    $sshCall += @("-o", ("ConnectTimeout=" + [string]$RemoteTimeoutSeconds))
                }
                $sshCall += @("-o", ("StrictHostKeyChecking=" + $strictHostKey))
                if (-not [string]::IsNullOrWhiteSpace($knownHostsFile)) {
                    $sshCall += @("-o", ("UserKnownHostsFile=" + $knownHostsFile))
                }
                if (-not [string]::IsNullOrWhiteSpace($identityFile)) {
                    $sshCall += @("-i", $identityFile)
                }
                $sshCall += $target
                $sshCall += $remoteCommand

                & $SshBinary @sshCall | Out-Host
                if ($LASTEXITCODE -ne 0) {
                    throw ("ssh exited with code " + $LASTEXITCODE)
                }
            }
            "winrm" {
                if ([string]::IsNullOrWhiteSpace([string]$Node.remote_host)) {
                    throw ("node remote_host is required for winrm transport, node=" + $nodeName)
                }
                $remoteRepoRoot = [string]$Node.remote_repo_root
                if ([string]::IsNullOrWhiteSpace($remoteRepoRoot)) {
                    $remoteRepoRoot = [string]$Node.repo_root
                }
                if ([string]::IsNullOrWhiteSpace($remoteRepoRoot)) {
                    throw ("node remote_repo_root or repo_root is required for winrm transport, node=" + $nodeName)
                }
                $remoteScriptPath = [string]$Node.lifecycle_script_path
                if ([string]::IsNullOrWhiteSpace($remoteScriptPath)) {
                    $remoteScriptPath = Join-Path $remoteRepoRoot "scripts/novovm-node-lifecycle.ps1"
                }

                $pairs = @()
                foreach ($k in $invokeArgs.Keys) {
                    $pairs += [pscustomobject]@{
                        key = $k
                        value = $invokeArgs[$k]
                    }
                }

                $invokeParams = @{
                    ComputerName = [string]$Node.remote_host
                    ScriptBlock = {
                        param($scriptPath, $repoRoot, $lifecycleAction, $kvPairs)
                        $argsMap = @{
                            Action = $lifecycleAction
                            RepoRoot = $repoRoot
                        }
                        foreach ($pair in $kvPairs) {
                            $argsMap[[string]$pair.key] = $pair.value
                        }
                        & $scriptPath @argsMap | Out-Host
                    }
                    ArgumentList = @($remoteScriptPath, $remoteRepoRoot, $LifecycleAction, $pairs)
                    ErrorAction = "Stop"
                }
                if ($null -ne $Node.winrm_use_ssl -and [bool]$Node.winrm_use_ssl) {
                    $invokeParams["UseSSL"] = $true
                }
                if ($null -ne $Node.winrm_port -and -not [string]::IsNullOrWhiteSpace([string]$Node.winrm_port)) {
                    $invokeParams["Port"] = [int]$Node.winrm_port
                }
                if ($null -ne $Node.winrm_auth -and -not [string]::IsNullOrWhiteSpace([string]$Node.winrm_auth)) {
                    $invokeParams["Authentication"] = [string]$Node.winrm_auth
                }

                $cred = Resolve-WinRmCredential -Node $Node
                if ($null -ne $cred) {
                    $invokeParams["Credential"] = $cred
                }

                $opTimeoutSec = $RemoteTimeoutSeconds
                if ($null -ne $Node.winrm_operation_timeout_sec -and -not [string]::IsNullOrWhiteSpace([string]$Node.winrm_operation_timeout_sec)) {
                    $opTimeoutSec = [int]$Node.winrm_operation_timeout_sec
                }
                if ($opTimeoutSec -gt 0) {
                    $invokeParams["SessionOption"] = New-PSSessionOption -OperationTimeout ($opTimeoutSec * 1000)
                }

                Invoke-Command @invokeParams | Out-Host
            }
            default {
                throw ("unsupported transport branch: " + $transport)
            }
        }
        $ok = $true
    } catch {
        $ok = $false
        $err = $_.Exception.Message
        Write-Host ("rollout_error: node={0} transport={1} action={2} err={3}" -f $nodeName, $transport, $LifecycleAction, $err)
    }

    $resultText = "ok"
    if (-not $ok) {
        $resultText = "error"
    }
    Write-AuditRecord -AuditPath $AuditPath -Record ([pscustomobject][ordered]@{
            timestamp_utc = [DateTime]::UtcNow.ToString("o")
            operation_id = $CurrentOperationId
            controller_id = $CurrentControllerId
            action = $Action
            lifecycle_action = $LifecycleAction
            node_group = $NodeGroup
            node = $nodeName
            transport = $transport
            target_version = [string]$invokeArgs["TargetVersion"]
            rollback_version = [string]$invokeArgs["RollbackVersion"]
            result = $resultText
            error = $err
            dry_run = $false
        })
    return $ok
}

function Get-ReversedArray {
    param([object[]]$InputArray)
    $copy = @($InputArray)
    [array]::Reverse($copy)
    return ,$copy
}

$RepoRootPath = Resolve-RootPath -Root ""
$AuditFilePath = Resolve-FullPath -Root $RepoRootPath -Value $AuditFile
$CurrentOperationId = Resolve-OperationId -Raw $OperationId
$CurrentControllerId = $ControllerId

$planResult = Load-Plan -RepoRootPath $RepoRootPath -PlanFilePath $PlanFile
$planPath = $planResult.plan_path
$plan = $planResult.plan
Assert-ControllerAuthorized -Plan $plan -CurrentControllerId $CurrentControllerId
$groups = Get-GroupList -Plan $plan -DefaultGroups $GroupOrder

Write-Host ("rollout_plan_loaded: path={0} groups={1} controller={2} op={3} audit={4}" -f $planPath, ($groups -join ","), $CurrentControllerId, $CurrentOperationId, $AuditFilePath)

switch ($Action) {
    "set-policy" {
        $okCount = 0
        $errCount = 0
        foreach ($group in $groups) {
            $nodes = Get-NodeListByGroup -Plan $plan -GroupName $group
            foreach ($node in $nodes) {
                $args = @{
                    NodeGroup = $group
                }
                if ($null -ne $node.upgrade_window -and -not [string]::IsNullOrWhiteSpace([string]$node.upgrade_window)) {
                    $args["UpgradeWindow"] = [string]$node.upgrade_window
                }
                $ok = Invoke-LifecycleAction -RepoRootPath $RepoRootPath -AuditPath $AuditFilePath -CurrentOperationId $CurrentOperationId -CurrentControllerId $CurrentControllerId -Node $node -NodeGroup $group -LifecycleAction "set-policy" -ExtraArgs $args
                if ($ok) {
                    $okCount += 1
                } else {
                    $errCount += 1
                    if (-not $ContinueOnFailure) {
                        throw ("set-policy failed and stopped at node=" + [string]$node.name)
                    }
                }
            }
        }
        Write-Host ("rollout_set_policy_done: ok={0} err={1}" -f $okCount, $errCount)
    }
    "status" {
        $okCount = 0
        $errCount = 0
        foreach ($group in $groups) {
            $nodes = Get-NodeListByGroup -Plan $plan -GroupName $group
            foreach ($node in $nodes) {
                $ok = Invoke-LifecycleAction -RepoRootPath $RepoRootPath -AuditPath $AuditFilePath -CurrentOperationId $CurrentOperationId -CurrentControllerId $CurrentControllerId -Node $node -NodeGroup $group -LifecycleAction "status" -ExtraArgs @{}
                if ($ok) {
                    $okCount += 1
                } else {
                    $errCount += 1
                    if (-not $ContinueOnFailure) {
                        throw ("status failed and stopped at node=" + [string]$node.name)
                    }
                }
            }
        }
        Write-Host ("rollout_status_done: ok={0} err={1}" -f $okCount, $errCount)
    }
    "upgrade" {
        if ([string]::IsNullOrWhiteSpace($TargetVersion)) {
            throw "upgrade requires -TargetVersion"
        }
        $totalOk = 0
        $totalErr = 0
        foreach ($group in $groups) {
            $groupMaxFailures = Get-GroupMaxFailures -Plan $plan -GroupName $group -Fallback $DefaultMaxFailures
            $groupErr = 0
            $nodes = Get-NodeListByGroup -Plan $plan -GroupName $group
            Write-Host ("rollout_group_in: group={0} node_count={1} max_failures={2}" -f $group, $nodes.Count, $groupMaxFailures)
            foreach ($node in $nodes) {
                $ok = Invoke-LifecycleAction -RepoRootPath $RepoRootPath -AuditPath $AuditFilePath -CurrentOperationId $CurrentOperationId -CurrentControllerId $CurrentControllerId -Node $node -NodeGroup $group -LifecycleAction "upgrade" -ExtraArgs @{
                    TargetVersion = $TargetVersion
                    UpgradeHealthSeconds = $UpgradeHealthSeconds
                    RequireNodeGroup = $group
                }
                if ($ok) {
                    $totalOk += 1
                } else {
                    $totalErr += 1
                    $groupErr += 1
                    if ($AutoRollbackOnFailure) {
                        $rollbackArgs = @{}
                        if (-not [string]::IsNullOrWhiteSpace($RollbackVersion)) {
                            $rollbackArgs["RollbackVersion"] = $RollbackVersion
                        }
                        [void](Invoke-LifecycleAction -RepoRootPath $RepoRootPath -AuditPath $AuditFilePath -CurrentOperationId $CurrentOperationId -CurrentControllerId $CurrentControllerId -Node $node -NodeGroup $group -LifecycleAction "rollback" -ExtraArgs $rollbackArgs)
                    }
                    if (-not $ContinueOnFailure) {
                        throw ("upgrade failed and stopped at node=" + [string]$node.name)
                    }
                    if ($groupErr -gt $groupMaxFailures) {
                        throw ("upgrade failed over threshold: group=" + $group + " errors=" + $groupErr + " max=" + $groupMaxFailures)
                    }
                }
                if ($PauseSecondsBetweenNodes -gt 0) {
                    Start-Sleep -Seconds $PauseSecondsBetweenNodes
                }
            }
            if ($groupErr -gt $groupMaxFailures) {
                throw ("upgrade group threshold exceeded: group=" + $group + " errors=" + $groupErr + " max=" + $groupMaxFailures)
            }
            Write-Host ("rollout_group_out: group={0} ok={1} err={2}" -f $group, ($nodes.Count - $groupErr), $groupErr)
        }
        Write-Host ("rollout_upgrade_done: target={0} ok={1} err={2}" -f $TargetVersion, $totalOk, $totalErr)
        if ($totalErr -gt 0) {
            throw ("rollout upgrade completed with errors: " + $totalErr)
        }
    }
    "rollback" {
        $rollbackGroups = Get-ReversedArray -InputArray $groups
        $okCount = 0
        $errCount = 0
        foreach ($group in $rollbackGroups) {
            $nodes = Get-NodeListByGroup -Plan $plan -GroupName $group
            Write-Host ("rollout_rollback_group_in: group={0} node_count={1}" -f $group, $nodes.Count)
            foreach ($node in $nodes) {
                $args = @{}
                if (-not [string]::IsNullOrWhiteSpace($RollbackVersion)) {
                    $args["RollbackVersion"] = $RollbackVersion
                }
                $ok = Invoke-LifecycleAction -RepoRootPath $RepoRootPath -AuditPath $AuditFilePath -CurrentOperationId $CurrentOperationId -CurrentControllerId $CurrentControllerId -Node $node -NodeGroup $group -LifecycleAction "rollback" -ExtraArgs $args
                if ($ok) {
                    $okCount += 1
                } else {
                    $errCount += 1
                    if (-not $ContinueOnFailure) {
                        throw ("rollback failed and stopped at node=" + [string]$node.name)
                    }
                }
                if ($PauseSecondsBetweenNodes -gt 0) {
                    Start-Sleep -Seconds $PauseSecondsBetweenNodes
                }
            }
            Write-Host ("rollout_rollback_group_out: group={0} done={1}" -f $group, $nodes.Count)
        }
        Write-Host ("rollout_rollback_done: ok={0} err={1}" -f $okCount, $errCount)
        if ($errCount -gt 0) {
            throw ("rollout rollback completed with errors: " + $errCount)
        }
    }
    default {
        throw ("unknown action: " + $Action)
    }
}
