<#
DEPRECATED COMPATIBILITY SHELL

This script is NOT part of the NOVOVM mainline entry path.
Mainline entry is `novovmctl`.

This script exists only for compatibility forwarding.

Do NOT add:
- policy logic
- rollout logic
- lifecycle logic
- daemon logic
- any business logic

Only parameter forwarding and exit-code passthrough are allowed.
#>
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
    [bool]$EnablePriorityPreemption = $true,
    [ValidateRange(0, 600)]
    [int]$PreemptRequeueSeconds = 3,
    [string]$GlobalTargetVersion = "",
    [string]$GlobalRollbackVersion = "",
    [string]$ControllerId = "ops-main",
    [string]$OperationId = "",
    [string]$AuditFile = "artifacts/runtime/rollout/control-plane-audit.jsonl",
    [string]$ControllerLeaseFile = "artifacts/runtime/rollout/controller-lease.json",
    [string]$DedupeFile = "artifacts/runtime/rollout/dedupe-execution.json",
    [ValidateRange(5, 86400)]
    [int]$DedupeTtlSeconds = 86400,
    [ValidateRange(5, 3600)]
    [int]$LeaseTtlSeconds = 30,
    [ValidateRange(1, 300)]
    [int]$LeaseHeartbeatSeconds = 5,
    [switch]$AllowStandbyTakeover,
    [switch]$EnableSiteConsensus,
    [string]$SiteId = "",
    [string]$SiteConsensusStateFile = "artifacts/runtime/rollout/site-consensus-state.json",
    [ValidateRange(1, 16)]
    [int]$SiteConsensusRequiredSites = 1,
    [ValidateRange(5, 86400)]
    [int]$SiteConsensusVoteTtlSeconds = 120,
    [ValidateRange(1, 300)]
    [int]$SiteConsensusRetrySeconds = 3,
    [switch]$EnableSiteConflictAccountability,
    [string]$SiteConflictAccountabilityFile = "artifacts/runtime/rollout/site-consensus-accountability.json",
    [ValidateRange(0, 10000)]
    [int]$SiteConflictMaxPenaltyPoints = 200,
    [ValidateRange(0, 100)]
    [int]$SiteConflictRecoveryPerWin = 1,
    [switch]$EnableSiteConflictReputationAging,
    [ValidateRange(60, 86400)]
    [int]$SiteConflictReputationAgingIntervalSeconds = 3600,
    [ValidateRange(0, 100)]
    [int]$SiteConflictReputationRecoverPointsPerInterval = 1,
    [ValidateRange(0, 864000)]
    [int]$SiteConflictReputationRecoverIdleSeconds = 1800,
    [switch]$EnableSiteConflictRiskPredictor,
    [string]$SiteConflictRiskStateFile = "artifacts/runtime/rollout/site-consensus-risk.json",
    [ValidateRange(0.01, 1.0)]
    [double]$SiteConflictRiskEmaAlpha = 0.2,
    [switch]$EnableSiteConflictRiskAutoThrottle,
    [ValidateRange(1, 64)]
    [int]$SiteConflictRiskYellowMaxConcurrentPlans = 1,
    [ValidateRange(0, 600)]
    [int]$SiteConflictRiskYellowDispatchPauseSeconds = 3,
    [ValidateRange(1, 64)]
    [int]$SiteConflictRiskOrangeMaxConcurrentPlans = 1,
    [ValidateRange(0, 600)]
    [int]$SiteConflictRiskOrangeDispatchPauseSeconds = 6,
    [bool]$SiteConflictRiskRedBlock = $true,
    [switch]$EnableSiteConflictRiskWinnerGuard,
    [string]$SiteConflictRiskWinnerBlockedLevels = "red",
    [bool]$SiteConflictRiskWinnerFallbackAllow = $true,
    [switch]$EnableAdaptivePolicy,
    [string]$AdaptiveStateFile = "artifacts/runtime/rollout/adaptive-policy-state.json",
    [ValidateRange(0.01, 1.0)]
    [double]$AdaptiveAlpha = 0.2,
    [ValidateRange(0.0, 1.0)]
    [double]$AdaptiveHighFailureRate = 0.35,
    [ValidateRange(0.0, 1.0)]
    [double]$AdaptiveLowFailureRate = 0.10,
    [ValidateRange(0, 8)]
    [int]$AdaptiveMaxCapBoost = 1,
    [switch]$EnableStateRecovery,
    [string]$StateSnapshotFile = "artifacts/runtime/rollout/control-plane-state-snapshot.json",
    [string]$StateReplayFile = "artifacts/runtime/rollout/control-plane-replay.jsonl",
    [string[]]$StateSnapshotReplicaFiles = @(),
    [string[]]$StateReplayReplicaFiles = @(),
    [ValidateRange(100, 200000)]
    [int]$StateReplayMaxEntries = 5000,
    [switch]$EnableStateReplicaValidation,
    [ValidateRange(1, 86400)]
    [int]$StateReplicaValidationIntervalSeconds = 15,
    [ValidateRange(0, 10000)]
    [int]$StateReplicaAllowedLagEntries = 0,
    [switch]$EnableStateReplicaAutoFailover,
    [string]$StateReplicaHealthFile = "artifacts/runtime/rollout/control-plane-replica-health.json",
    [ValidateRange(1, 86400)]
    [int]$StateReplicaFailoverCooldownSeconds = 30,
    [switch]$EnableStateReplicaFailoverPolicy,
    [bool]$StateReplicaFailoverPolicyDefaultAllow = $true,
    [switch]$EnableStateReplicaFailoverSloLink,
    [ValidateRange(0.0, 100.0)]
    [double]$StateReplicaFailoverSloMinEffectiveScore = 60.0,
    [bool]$StateReplicaFailoverSloBlockOnViolation = $true,
    [switch]$EnableStateReplicaFailoverDrillLink,
    [ValidateRange(0.0, 1.0)]
    [double]$StateReplicaFailoverDrillMinPassRate = 0.5,
    [ValidateRange(0.0, 100.0)]
    [double]$StateReplicaFailoverDrillMinAverageScore = 60.0,
    [bool]$StateReplicaFailoverDrillRequireLastPass = $false,
    [switch]$EnableStateReplicaFailoverRiskLink,
    [string]$StateReplicaFailoverRiskBlockedLevels = "red",
    [switch]$StateReplicaFailoverOnStartup,
    [switch]$EnableStateReplicaSwitchback,
    [ValidateRange(1, 1000)]
    [int]$StateReplicaSwitchbackStableCycles = 3,
    [switch]$EnableStateReplicaDrill,
    [string]$StateReplicaDrillId = "",
    [switch]$EnableStateReplicaDrillScore,
    [string]$StateReplicaDrillScoreFile = "artifacts/runtime/rollout/control-plane-replica-drill-score.json",
    [ValidateRange(1, 10000)]
    [int]$StateReplicaDrillScoreWindowSamples = 20,
    [ValidateRange(0.0, 100.0)]
    [double]$StateReplicaDrillPassScore = 70.0,
    [switch]$EnableStateReplicaSlo,
    [string]$StateReplicaSloFile = "artifacts/runtime/rollout/control-plane-replica-slo.json",
    [ValidateRange(5, 10000)]
    [int]$StateReplicaSloWindowSamples = 60,
    [ValidateRange(0.0, 1.0)]
    [double]$StateReplicaSloMinGreenRate = 0.95,
    [ValidateRange(0, 10000)]
    [int]$StateReplicaSloMaxRedInWindow = 0,
    [switch]$StateReplicaSloBlockOnViolation,
    [switch]$EnableStateReplicaCircuitBreaker,
    [ValidateRange(1, 64)]
    [int]$StateReplicaYellowMaxConcurrentPlans = 1,
    [ValidateRange(0, 600)]
    [int]$StateReplicaYellowDispatchPauseSeconds = 3,
    [bool]$StateReplicaCircuitRedBlock = $true,
    [switch]$EnableStateReplicaAdaptiveThreshold,
    [string]$StateReplicaAdaptiveFile = "artifacts/runtime/rollout/control-plane-replica-adaptive.json",
    [ValidateRange(0.1, 20.0)]
    [double]$StateReplicaAdaptiveStep = 2.0,
    [ValidateRange(0.0, 100.0)]
    [double]$StateReplicaAdaptiveGoodScore = 92.0,
    [ValidateRange(0.0, 100.0)]
    [double]$StateReplicaAdaptiveBadScore = 70.0,
    [ValidateRange(0.0, 50.0)]
    [double]$StateReplicaAdaptiveMaxShift = 12.0,
    [switch]$ResumeFromSnapshot,
    [switch]$ReplayConflictsOnStart,
    [switch]$IgnoreRegionWindow,
    [switch]$ContinueOnPlanFailure,
    [switch]$DryRun
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

# COMPATIBILITY SHELL ONLY
# This script must not contain policy logic, rollout logic, risk evaluation,
# failover logic, or runtime decision logic on the mainline path.
# Mainline execution must go through `novovmctl rollout-control`.
# PowerShell here is limited to parameter forwarding and exit-code passthrough.

$script:NovovmCtlRolloutControlSupportedParams = @(
    "PlanAction",
    "QueueFile",
    "ControllerId",
    "OperationId",
    "AuditFile",
    "ResumeFromSnapshot",
    "ReplayConflictsOnStart",
    "ContinueOnPlanFailure",
    "DryRun"
)

function Resolve-NovovmCtlBinary {
    $candidates = @(
        "target/release/novovmctl.exe",
        "target/release/novovmctl",
        "target/debug/novovmctl.exe",
        "target/debug/novovmctl",
        "crates/novovmctl/target/release/novovmctl.exe",
        "crates/novovmctl/target/release/novovmctl",
        "crates/novovmctl/target/debug/novovmctl.exe",
        "crates/novovmctl/target/debug/novovmctl"
    )

    foreach ($candidate in $candidates) {
        if (Test-Path -LiteralPath $candidate) {
            return $candidate
        }
    }

    throw "novovmctl not found in default candidate paths"
}

function Invoke-NovovmCtlRolloutControlBridge {
    $unsupported = @($PSBoundParameters.Keys | Where-Object { $script:NovovmCtlRolloutControlSupportedParams -notcontains $_ })
    if ($unsupported.Count -gt 0) {
        throw ("novovm-node-rollout-control.ps1 compatibility shell only supports novovmctl-backed parameters. Unsupported legacy parameters: " + ($unsupported -join ", "))
    }

    if ([string]::IsNullOrWhiteSpace($QueueFile)) {
        throw "QueueFile is required"
    }

    $novovmctl = Resolve-NovovmCtlBinary
    $argsList = New-Object System.Collections.Generic.List[string]
    $argsList.Add("rollout-control")
    $argsList.Add("--queue-file")
    $argsList.Add($QueueFile)
    $argsList.Add("--plan-action")
    $argsList.Add($PlanAction)

    if (-not [string]::IsNullOrWhiteSpace($ControllerId)) {
        $argsList.Add("--controller-id")
        $argsList.Add($ControllerId)
    }
    if (-not [string]::IsNullOrWhiteSpace($OperationId)) {
        $argsList.Add("--operation-id")
        $argsList.Add($OperationId)
    }
    if (-not [string]::IsNullOrWhiteSpace($AuditFile)) {
        $argsList.Add("--audit-file")
        $argsList.Add($AuditFile)
    }
    if ($ResumeFromSnapshot) {
        $argsList.Add("--resume-from-snapshot")
    }
    if ($ReplayConflictsOnStart) {
        $argsList.Add("--replay-conflicts-on-start")
    }
    if ($ContinueOnPlanFailure) {
        $argsList.Add("--continue-on-plan-failure")
    }
    if ($DryRun) {
        $argsList.Add("--dry-run")
    }

    Write-Host ("[compat-shell] forwarding to novovmctl: {0} {1}" -f $novovmctl, ($argsList -join " "))
    & $novovmctl @argsList
    exit $LASTEXITCODE
}

Invoke-NovovmCtlRolloutControlBridge

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

function Resolve-FullPathList {
    param(
        [string]$Root,
        [object]$Values
    )
    $result = @()
    if ($null -eq $Values) {
        return $result
    }
    foreach ($v in $Values) {
        $s = [string]$v
        if ([string]::IsNullOrWhiteSpace($s)) {
            continue
        }
        $result += (Resolve-FullPath -Root $Root -Value $s)
    }
    return $result
}

function Ensure-ParentDir {
    param([string]$PathValue)
    $parent = Split-Path -Parent $PathValue
    if (-not [string]::IsNullOrWhiteSpace($parent)) {
        New-Item -ItemType Directory -Force -Path $parent | Out-Null
    }
}

function Now-Ms {
    return [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
}

function Resolve-OperationId {
    param([string]$Raw)
    if (-not [string]::IsNullOrWhiteSpace($Raw)) {
        return $Raw
    }
    return ("control-" + (Now-Ms) + "-" + $PID)
}

function Write-Audit {
    param(
        [string]$Path,
        [pscustomobject]$Obj
    )
    Ensure-ParentDir -PathValue $Path
    $line = $Obj | ConvertTo-Json -Compress -Depth 20
    Add-Content -LiteralPath $Path -Value $line -Encoding UTF8
}

function Test-WindowUtc {
    param([string]$Window)
    if ([string]::IsNullOrWhiteSpace($Window)) {
        return [pscustomobject]@{ allowed = $true; reason = "no window" }
    }
    $m = [regex]::Match($Window, "^\s*(\d{2}):(\d{2})-(\d{2}):(\d{2})\s*UTC\s*$")
    if (-not $m.Success) {
        return [pscustomobject]@{ allowed = $false; reason = ("invalid format: " + $Window) }
    }
    $sh = [int]$m.Groups[1].Value
    $sm = [int]$m.Groups[2].Value
    $eh = [int]$m.Groups[3].Value
    $em = [int]$m.Groups[4].Value
    if ($sh -gt 23 -or $eh -gt 23 -or $sm -gt 59 -or $em -gt 59) {
        return [pscustomobject]@{ allowed = $false; reason = ("invalid value: " + $Window) }
    }
    $start = ($sh * 60) + $sm
    $end = ($eh * 60) + $em
    $now = [DateTime]::UtcNow
    $nowMin = ($now.Hour * 60) + $now.Minute
    if ($start -eq $end) {
        return [pscustomobject]@{ allowed = $true; reason = "full day" }
    }
    $ok = $false
    if ($start -lt $end) {
        $ok = ($nowMin -ge $start -and $nowMin -lt $end)
    } else {
        $ok = ($nowMin -ge $start -or $nowMin -lt $end)
    }
    return [pscustomobject]@{ allowed = $ok; reason = ("window=" + $Window + " now_utc=" + $now.ToString("HH:mm")) }
}

function Region-Key {
    param([string]$Region)
    if ([string]::IsNullOrWhiteSpace($Region)) {
        return "DEFAULT"
    }
    return $Region.ToUpperInvariant()
}

function Get-RegionCaps {
    param(
        [object]$Queue,
        [int]$Fallback
    )
    $map = @{}
    if ($null -ne $Queue.region_capacities) {
        foreach ($p in $Queue.region_capacities.PSObject.Properties) {
            $v = [int]$p.Value
            if ($v -gt 0) {
                $map[(Region-Key -Region $p.Name)] = $v
            }
        }
    }
    if (-not $map.ContainsKey("DEFAULT")) {
        $map["DEFAULT"] = $Fallback
    }
    return $map
}

function Region-Cap {
    param(
        [hashtable]$Caps,
        [string]$Region,
        [int]$Fallback
    )
    $k = Region-Key -Region $Region
    if ($Caps.ContainsKey($k)) {
        return [int]$Caps[$k]
    }
    if ($Caps.ContainsKey("DEFAULT")) {
        return [int]$Caps["DEFAULT"]
    }
    return $Fallback
}

function Running-InRegion {
    param(
        [object[]]$Running,
        [string]$Region
    )
    $k = Region-Key -Region $Region
    $c = 0
    foreach ($r in $Running) {
        if ((Region-Key -Region ([string]$r.region)) -eq $k) {
            $c += 1
        }
    }
    return $c
}

function Pending-InRegion {
    param(
        [object[]]$Pending,
        [string]$Region
    )
    $k = Region-Key -Region $Region
    $c = 0
    foreach ($r in $Pending) {
        if ((Region-Key -Region ([string]$r.region)) -eq $k) {
            $c += 1
        }
    }
    return $c
}

function Remove-At {
    param(
        [object[]]$ArrayValue,
        [int]$Index
    )
    if ($null -eq $ArrayValue -or $ArrayValue.Count -eq 0) {
        return @()
    }
    if ($ArrayValue.Count -eq 1) {
        return @()
    }
    if ($Index -le 0) {
        return @($ArrayValue[1..($ArrayValue.Count - 1)])
    }
    if ($Index -ge ($ArrayValue.Count - 1)) {
        return @($ArrayValue[0..($ArrayValue.Count - 2)])
    }
    return @($ArrayValue[0..($Index - 1)] + $ArrayValue[($Index + 1)..($ArrayValue.Count - 1)])
}

function Clone-Entry {
    param(
        [object]$Source,
        [int]$Attempt,
        [int64]$NextRun
    )
    $copy = [pscustomobject]@{}
    foreach ($p in $Source.PSObject.Properties) {
        $copy | Add-Member -NotePropertyName $p.Name -NotePropertyValue $p.Value -Force
    }
    $copy.attempt = $Attempt
    $copy.next_run = $NextRun
    return $copy
}

function Convert-ToBooleanLoose {
    param(
        [object]$Value,
        [bool]$Default = $false
    )
    if ($null -eq $Value) {
        return $Default
    }
    if ($Value -is [bool]) {
        return [bool]$Value
    }
    $text = ([string]$Value).Trim().ToLowerInvariant()
    if ([string]::IsNullOrWhiteSpace($text)) {
        return $Default
    }
    if ($text -in @("1", "true", "yes", "on")) {
        return $true
    }
    if ($text -in @("0", "false", "no", "off")) {
        return $false
    }
    return $Default
}

function Convert-CandidateLiteralToList {
    param(
        [object]$Value
    )
    if ($null -eq $Value) {
        return @()
    }
    if ($Value -is [System.Array]) {
        $out = @()
        foreach ($item in $Value) {
            if ($null -ne $item) {
                $text = ([string]$item).Trim()
                if (-not [string]::IsNullOrWhiteSpace($text)) {
                    $out += $text
                }
            }
        }
        return $out
    }
    return ([string]$Value).Split(@(",", ";"), [System.StringSplitOptions]::RemoveEmptyEntries) | ForEach-Object { $_.Trim() } | Where-Object { -not [string]::IsNullOrWhiteSpace($_) }
}

function Try-ParseJsonObject {
    param(
        [string]$Raw
    )
    if ([string]::IsNullOrWhiteSpace($Raw)) {
        return $null
    }
    try {
        return ($Raw | ConvertFrom-Json -ErrorAction Stop)
    } catch {
        return $null
    }
}

function Get-ObjectPropertyValueCI {
    param(
        [object]$Object,
        [string]$Name
    )
    if ($null -eq $Object) {
        return $null
    }
    if ($Object -is [System.Collections.IDictionary]) {
        foreach ($key in $Object.Keys) {
            if ([string]::Equals([string]$key, $Name, [System.StringComparison]::OrdinalIgnoreCase)) {
                return $Object[$key]
            }
        }
        return $null
    }
    foreach ($prop in $Object.PSObject.Properties) {
        if ([string]::Equals([string]$prop.Name, $Name, [System.StringComparison]::OrdinalIgnoreCase)) {
            return $prop.Value
        }
    }
    return $null
}

function Get-CandidatesFromDirectoryForPenalty {
    param(
        [string]$DirectoryFile,
        [string]$Region,
        [int]$Limit
    )
    if ([string]::IsNullOrWhiteSpace($DirectoryFile)) {
        return @()
    }
    if (-not (Test-Path -LiteralPath $DirectoryFile)) {
        return @()
    }
    $raw = Get-Content -LiteralPath $DirectoryFile -Raw -ErrorAction SilentlyContinue
    if ([string]::IsNullOrWhiteSpace($raw)) {
        return @()
    }
    $obj = Try-ParseJsonObject -Raw $raw
    if ($null -eq $obj) {
        return @()
    }
    $relays = Get-ObjectPropertyValueCI -Object $obj -Name "relays"
    if ($null -eq $relays -or -not ($relays -is [System.Array])) {
        return @()
    }
    $effectiveRegion = "default"
    if (-not [string]::IsNullOrWhiteSpace($Region)) {
        $effectiveRegion = ([string]$Region).Trim()
    }
    $picked = @()
    foreach ($relay in $relays) {
        if ($null -eq $relay) {
            continue
        }
        $idRaw = Get-ObjectPropertyValueCI -Object $relay -Name "id"
        if ($null -eq $idRaw) {
            continue
        }
        $relayId = ([string]$idRaw).Trim()
        if ([string]::IsNullOrWhiteSpace($relayId)) {
            continue
        }
        $enabledRaw = Get-ObjectPropertyValueCI -Object $relay -Name "enabled"
        if ($null -ne $enabledRaw -and -not [bool]$enabledRaw) {
            continue
        }
        $regionRaw = Get-ObjectPropertyValueCI -Object $relay -Name "region"
        if ($null -ne $regionRaw -and -not [string]::IsNullOrWhiteSpace([string]$regionRaw)) {
            $relayRegion = ([string]$regionRaw).Trim()
            if (
                -not [string]::Equals($relayRegion, "global", [System.StringComparison]::OrdinalIgnoreCase) -and
                -not [string]::Equals($relayRegion, $effectiveRegion, [System.StringComparison]::OrdinalIgnoreCase)
            ) {
                continue
            }
        }
        $health = 1.0
        $healthRaw = Get-ObjectPropertyValueCI -Object $relay -Name "health"
        if ($null -ne $healthRaw) {
            $parsed = 0.0
            if ([double]::TryParse([string]$healthRaw, [ref]$parsed)) {
                $health = $parsed
            }
        }
        $picked += [pscustomobject]@{
            id = $relayId
            health = $health
        }
    }
    if ($picked.Count -eq 0) {
        return @()
    }
    $maxCount = [Math]::Max(1, $Limit)
    return @($picked | Sort-Object -Property @{ Expression = "health"; Descending = $true }, @{ Expression = "id"; Descending = $false } | Select-Object -First $maxCount | ForEach-Object { [string]$_.id })
}

function Get-RelayHealthFromDirectoryForPenalty {
    param(
        [string]$DirectoryFile,
        [string]$Region,
        [string]$RelayId
    )
    if ([string]::IsNullOrWhiteSpace($DirectoryFile) -or [string]::IsNullOrWhiteSpace($RelayId)) {
        return $null
    }
    if (-not (Test-Path -LiteralPath $DirectoryFile)) {
        return $null
    }
    $raw = Get-Content -LiteralPath $DirectoryFile -Raw -ErrorAction SilentlyContinue
    if ([string]::IsNullOrWhiteSpace($raw)) {
        return $null
    }
    $obj = Try-ParseJsonObject -Raw $raw
    if ($null -eq $obj) {
        return $null
    }
    $relays = Get-ObjectPropertyValueCI -Object $obj -Name "relays"
    if ($null -eq $relays -or -not ($relays -is [System.Array])) {
        return $null
    }
    $effectiveRegion = "default"
    if (-not [string]::IsNullOrWhiteSpace($Region)) {
        $effectiveRegion = ([string]$Region).Trim()
    }
    foreach ($relay in $relays) {
        if ($null -eq $relay) {
            continue
        }
        $idRaw = Get-ObjectPropertyValueCI -Object $relay -Name "id"
        if ($null -eq $idRaw) {
            continue
        }
        $currentId = ([string]$idRaw).Trim()
        if (-not [string]::Equals($currentId, [string]$RelayId, [System.StringComparison]::OrdinalIgnoreCase)) {
            continue
        }
        $enabledRaw = Get-ObjectPropertyValueCI -Object $relay -Name "enabled"
        if ($null -ne $enabledRaw -and -not (Convert-ToBooleanLoose -Value $enabledRaw -Default $true)) {
            return $null
        }
        $regionRaw = Get-ObjectPropertyValueCI -Object $relay -Name "region"
        if ($null -ne $regionRaw -and -not [string]::IsNullOrWhiteSpace([string]$regionRaw)) {
            $relayRegion = ([string]$regionRaw).Trim()
            if (
                -not [string]::Equals($relayRegion, "global", [System.StringComparison]::OrdinalIgnoreCase) -and
                -not [string]::Equals($relayRegion, $effectiveRegion, [System.StringComparison]::OrdinalIgnoreCase)
            ) {
                return $null
            }
        }
        $health = 1.0
        $healthRaw = Get-ObjectPropertyValueCI -Object $relay -Name "health"
        if ($null -ne $healthRaw) {
            $parsed = 0.0
            if ([double]::TryParse([string]$healthRaw, [ref]$parsed)) {
                $health = [Math]::Min(1, [Math]::Max(0, $parsed))
            }
        }
        return $health
    }
    return $null
}

function Load-OverlayRouteRuntimeProfile {
    param(
        [string]$RuntimeFile,
        [string]$Profile
    )
    if ([string]::IsNullOrWhiteSpace($RuntimeFile)) {
        return $null
    }
    if (-not (Test-Path -LiteralPath $RuntimeFile)) {
        return $null
    }
    $raw = Get-Content -LiteralPath $RuntimeFile -Raw -ErrorAction SilentlyContinue
    if ([string]::IsNullOrWhiteSpace($raw)) {
        return $null
    }
    $root = Try-ParseJsonObject -Raw $raw
    if ($null -eq $root) {
        return $null
    }
    $profiles = Get-ObjectPropertyValueCI -Object $root -Name "profiles"
    if ($null -eq $profiles) {
        return $null
    }
    $profileName = "default"
    if (-not [string]::IsNullOrWhiteSpace($Profile)) {
        $profileName = ([string]$Profile).Trim()
    }
    $profileObj = Get-ObjectPropertyValueCI -Object $profiles -Name $profileName
    if ($null -eq $profileObj -and -not [string]::Equals($profileName, "default", [System.StringComparison]::OrdinalIgnoreCase)) {
        $profileObj = Get-ObjectPropertyValueCI -Object $profiles -Name "default"
    }
    return $profileObj
}

function Resolve-EntryRelayCandidatesForPenalty {
    param(
        [object]$Entry
    )
    $direct = Convert-CandidateLiteralToList -Value $Entry.overlay_route_relay_candidates
    if ($direct.Count -gt 0) {
        return $direct
    }
    if (-not [string]::IsNullOrWhiteSpace([string]$Entry.overlay_route_relay_candidates_by_region)) {
        $regionMap = Try-ParseJsonObject -Raw ([string]$Entry.overlay_route_relay_candidates_by_region)
        if ($null -ne $regionMap) {
            $regionRaw = [string]$Entry.region
            $regionCandidatesRaw = $null
            if (-not [string]::IsNullOrWhiteSpace($regionRaw)) {
                $regionCandidatesRaw = Get-ObjectPropertyValueCI -Object $regionMap -Name $regionRaw
            }
            if ($null -eq $regionCandidatesRaw) {
                $regionCandidatesRaw = Get-ObjectPropertyValueCI -Object $regionMap -Name "default"
            }
            $regionCandidates = Convert-CandidateLiteralToList -Value $regionCandidatesRaw
            if ($regionCandidates.Count -gt 0) {
                return $regionCandidates
            }
        }
    }
    if (-not [string]::IsNullOrWhiteSpace([string]$Entry.overlay_route_relay_candidates_by_role)) {
        $roleMap = Try-ParseJsonObject -Raw ([string]$Entry.overlay_route_relay_candidates_by_role)
        if ($null -ne $roleMap) {
            $roleRaw = ""
            if ($null -ne $Entry.plan -and $null -ne $Entry.plan.role_profile) {
                $roleRaw = [string]$Entry.plan.role_profile
            }
            $roleCandidatesRaw = $null
            if (-not [string]::IsNullOrWhiteSpace($roleRaw)) {
                $roleCandidatesRaw = Get-ObjectPropertyValueCI -Object $roleMap -Name $roleRaw
            }
            if ($null -eq $roleCandidatesRaw) {
                $roleCandidatesRaw = Get-ObjectPropertyValueCI -Object $roleMap -Name "default"
            }
            $roleCandidates = Convert-CandidateLiteralToList -Value $roleCandidatesRaw
            if ($roleCandidates.Count -gt 0) {
                return $roleCandidates
            }
        }
    }
    return (Get-CandidatesFromDirectoryForPenalty -DirectoryFile ([string]$Entry.overlay_route_relay_directory_file) -Region ([string]$Entry.region) -Limit 16)
}

function Merge-RelayPenaltyDeltaJson {
    param(
        [string]$Raw,
        [string]$RelayId,
        [double]$Delta
    )
    $map = @{}
    $existing = Try-ParseJsonObject -Raw $Raw
    if ($null -ne $existing) {
        if ($existing -is [System.Collections.IDictionary]) {
            foreach ($key in $existing.Keys) {
                $value = 0.0
                if ([double]::TryParse([string]$existing[$key], [ref]$value)) {
                    $map[[string]$key] = [Math]::Min(1, [Math]::Max(0, $value))
                }
            }
        } else {
            foreach ($prop in $existing.PSObject.Properties) {
                $value = 0.0
                if ([double]::TryParse([string]$prop.Value, [ref]$value)) {
                    $map[[string]$prop.Name] = [Math]::Min(1, [Math]::Max(0, $value))
                }
            }
        }
    }
    $base = 0.0
    if ($map.Contains([string]$RelayId)) {
        [double]::TryParse([string]$map[[string]$RelayId], [ref]$base) | Out-Null
    }
    $map[[string]$RelayId] = [Math]::Min(1, [Math]::Max(0, ($base + $Delta)))
    return ($map | ConvertTo-Json -Depth 6 -Compress)
}

function Try-ApplyAutoRelayPenalty {
    param(
        [object]$Entry
    )
    $enabled = Convert-ToBooleanLoose -Value $Entry.overlay_route_auto_penalty_enabled -Default $false
    if (-not $enabled) {
        return $null
    }
    $step = 0.2
    if ($null -ne $Entry.overlay_route_auto_penalty_step) {
        $step = [double]$Entry.overlay_route_auto_penalty_step
    }
    $step = [Math]::Min(1, [Math]::Max(0, $step))
    if ($step -le 0) {
        return $null
    }
    $candidates = Resolve-EntryRelayCandidatesForPenalty -Entry $Entry
    if ($candidates.Count -le 0) {
        return $null
    }
    $targetRelay = [string]$candidates[0]
    if ([string]::IsNullOrWhiteSpace($targetRelay)) {
        return $null
    }

    $attempt = 1
    $attemptParsed = 0
    if ($null -ne $Entry.attempt -and [int]::TryParse([string]$Entry.attempt, [ref]$attemptParsed) -and $attemptParsed -gt 0) {
        $attempt = $attemptParsed
    }
    $streakBoost = 1.0 + ([Math]::Min(4, [Math]::Max(0, ($attempt - 1))) * 0.25)

    $healthFactor = 1.0
    $relayHealth = Get-RelayHealthFromDirectoryForPenalty -DirectoryFile ([string]$Entry.overlay_route_relay_directory_file) -Region ([string]$Entry.region) -RelayId $targetRelay
    if ($null -ne $relayHealth) {
        $h = [double]$relayHealth
        if ($h -lt 0.3) {
            $healthFactor = 1.4
        } elseif ($h -lt 0.5) {
            $healthFactor = 1.2
        } elseif ($h -gt 0.85) {
            $healthFactor = 0.8
        }
    }

    $effectiveStep = [Math]::Min(1, [Math]::Max(0, ($step * $streakBoost * $healthFactor)))
    if ($effectiveStep -le 0) {
        return $null
    }

    $merged = Merge-RelayPenaltyDeltaJson -Raw ([string]$Entry.overlay_route_relay_penalty_delta) -RelayId $targetRelay -Delta $effectiveStep
    $Entry.overlay_route_relay_penalty_delta = $merged
    return [pscustomobject]@{
        relay_id = $targetRelay
        step = $effectiveStep
        base_step = $step
        streak_boost = $streakBoost
        health_factor = $healthFactor
        relay_health = $relayHealth
        delta_json = $merged
    }
}

function Try-DiscoverOverlayRelays {
    param(
        [string]$RepoRoot,
        [object]$Entry
    )
    $enabled = Convert-ToBooleanLoose -Value $Entry.overlay_route_relay_discovery_enabled -Default $false
    if (-not $enabled) {
        return $null
    }
    $directoryFile = [string]$Entry.overlay_route_relay_directory_file
    $discoveryFile = [string]$Entry.overlay_route_relay_discovery_file
    if ([string]::IsNullOrWhiteSpace($directoryFile)) {
        return [pscustomobject]@{
            invoked = $true
            ok = $false
            reason = "missing_directory"
            message = "overlay_route_relay_directory_file is empty"
        }
    }
    if ([string]::IsNullOrWhiteSpace($discoveryFile)) {
        return [pscustomobject]@{
            invoked = $true
            ok = $false
            reason = "missing_discovery_file"
            message = "overlay_route_relay_discovery_file is empty"
        }
    }

    $cooldownSeconds = 120
    if ($null -ne $Entry.overlay_route_relay_discovery_cooldown_seconds) {
        $parsedCooldown = 0
        if ([int]::TryParse([string]$Entry.overlay_route_relay_discovery_cooldown_seconds, [ref]$parsedCooldown)) {
            $cooldownSeconds = [Math]::Min(7200, [Math]::Max(1, $parsedCooldown))
        }
    }
    if ($null -eq $script:OverlayRelayDiscoveryLastRunByKey) {
        $script:OverlayRelayDiscoveryLastRunByKey = @{}
    }
    $key = ($directoryFile + "|" + $discoveryFile).ToLowerInvariant()
    $nowMs = [int64](Now-Ms)
    if ($script:OverlayRelayDiscoveryLastRunByKey.ContainsKey($key)) {
        $lastMs = [int64]$script:OverlayRelayDiscoveryLastRunByKey[$key]
        $cooldownMs = [int64]$cooldownSeconds * 1000
        if (($nowMs - $lastMs) -lt $cooldownMs) {
            return [pscustomobject]@{
                invoked = $false
                ok = $true
                reason = "cooldown"
                message = ("cooldown active, left_ms=" + [string]($cooldownMs - ($nowMs - $lastMs)))
            }
        }
    }

    $mergeScript = Join-Path $RepoRoot "scripts/novovm-overlay-relay-discovery-merge.ps1"
    $mergeBinaryCfg = Resolve-PolicyToolBinaryConfig -RepoRootPath $RepoRoot -RawBinaryFile ([string]$Entry.overlay_route_relay_discovery_binary_path) -ToolName "overlay-relay-discovery-merge" -LegacyBaseName "novovm-overlay-relay-discovery-merge"
    $mergeRustBinary = [string]$mergeBinaryCfg.binary_path

    $defaultHealth = 0.85
    if ($null -ne $Entry.overlay_route_relay_discovery_default_health) {
        $parsedDefaultHealth = 0.0
        if ([double]::TryParse([string]$Entry.overlay_route_relay_discovery_default_health, [ref]$parsedDefaultHealth)) {
            $defaultHealth = [Math]::Min(1, [Math]::Max(0, $parsedDefaultHealth))
        }
    }
    $defaultEnabled = $true
    if ($null -ne $Entry.overlay_route_relay_discovery_default_enabled) {
        $defaultEnabled = Convert-ToBooleanLoose -Value $Entry.overlay_route_relay_discovery_default_enabled -Default $defaultEnabled
    }
    $httpUrls = ""
    if ($null -ne $Entry.overlay_route_relay_discovery_http_urls -and -not [string]::IsNullOrWhiteSpace([string]$Entry.overlay_route_relay_discovery_http_urls)) {
        $httpUrls = [string]$Entry.overlay_route_relay_discovery_http_urls
    }
    $sourceWeightsJson = ""
    if ($null -ne $Entry.overlay_route_relay_discovery_source_weights -and -not [string]::IsNullOrWhiteSpace([string]$Entry.overlay_route_relay_discovery_source_weights)) {
        $sourceWeightsJson = [string]$Entry.overlay_route_relay_discovery_source_weights
    }
    $httpTimeoutMs = 1500
    if ($null -ne $Entry.overlay_route_relay_discovery_http_timeout_ms) {
        $parsedTimeout = 0
        if ([int]::TryParse([string]$Entry.overlay_route_relay_discovery_http_timeout_ms, [ref]$parsedTimeout)) {
            $httpTimeoutMs = [Math]::Min(20000, [Math]::Max(100, $parsedTimeout))
        }
    }
    $sourceReputationFile = ""
    if ($null -ne $Entry.overlay_route_relay_discovery_source_reputation_file -and -not [string]::IsNullOrWhiteSpace([string]$Entry.overlay_route_relay_discovery_source_reputation_file)) {
        $sourceReputationFile = [string]$Entry.overlay_route_relay_discovery_source_reputation_file
    }
    $sourceDecay = 0.05
    if ($null -ne $Entry.overlay_route_relay_discovery_source_decay) {
        $parsedSourceDecay = 0.0
        if ([double]::TryParse([string]$Entry.overlay_route_relay_discovery_source_decay, [ref]$parsedSourceDecay)) {
            $sourceDecay = [Math]::Min(1, [Math]::Max(0, $parsedSourceDecay))
        }
    }
    $sourcePenaltyOnFail = 0.2
    if ($null -ne $Entry.overlay_route_relay_discovery_source_penalty_on_fail) {
        $parsedSourcePenalty = 0.0
        if ([double]::TryParse([string]$Entry.overlay_route_relay_discovery_source_penalty_on_fail, [ref]$parsedSourcePenalty)) {
            $sourcePenaltyOnFail = [Math]::Min(1, [Math]::Max(0, $parsedSourcePenalty))
        }
    }
    $sourceRecoverOnSuccess = 0.03
    if ($null -ne $Entry.overlay_route_relay_discovery_source_recover_on_success) {
        $parsedSourceRecover = 0.0
        if ([double]::TryParse([string]$Entry.overlay_route_relay_discovery_source_recover_on_success, [ref]$parsedSourceRecover)) {
            $sourceRecoverOnSuccess = [Math]::Min(1, [Math]::Max(0, $parsedSourceRecover))
        }
    }
    $sourceBlacklistThreshold = 0.15
    if ($null -ne $Entry.overlay_route_relay_discovery_source_blacklist_threshold) {
        $parsedSourceBlacklist = 0.0
        if ([double]::TryParse([string]$Entry.overlay_route_relay_discovery_source_blacklist_threshold, [ref]$parsedSourceBlacklist)) {
            $sourceBlacklistThreshold = [Math]::Min(1, [Math]::Max(0, $parsedSourceBlacklist))
        }
    }
    $sourceDenylist = ""
    if ($null -ne $Entry.overlay_route_relay_discovery_source_denylist -and -not [string]::IsNullOrWhiteSpace([string]$Entry.overlay_route_relay_discovery_source_denylist)) {
        $sourceDenylist = [string]$Entry.overlay_route_relay_discovery_source_denylist
    }
    $httpUrlsFile = ""
    if ($null -ne $Entry.overlay_route_relay_discovery_http_urls_file -and -not [string]::IsNullOrWhiteSpace([string]$Entry.overlay_route_relay_discovery_http_urls_file)) {
        $httpUrlsFile = [string]$Entry.overlay_route_relay_discovery_http_urls_file
    }
    $seedRegion = ""
    if (-not [string]::IsNullOrWhiteSpace([string]$Entry.region)) {
        $seedRegion = [string]$Entry.region
    }
    if ($null -ne $Entry.overlay_route_relay_discovery_seed_region -and -not [string]::IsNullOrWhiteSpace([string]$Entry.overlay_route_relay_discovery_seed_region)) {
        $seedRegion = [string]$Entry.overlay_route_relay_discovery_seed_region
    }
    $seedMode = ""
    if (-not [string]::IsNullOrWhiteSpace([string]$Entry.overlay_route_mode)) {
        $seedMode = [string]$Entry.overlay_route_mode
    }
    if ($null -ne $Entry.overlay_route_relay_discovery_seed_mode -and -not [string]::IsNullOrWhiteSpace([string]$Entry.overlay_route_relay_discovery_seed_mode)) {
        $seedMode = [string]$Entry.overlay_route_relay_discovery_seed_mode
    }
    $seedProfile = ""
    if (-not [string]::IsNullOrWhiteSpace([string]$Entry.overlay_route_runtime_profile)) {
        $seedProfile = [string]$Entry.overlay_route_runtime_profile
    }
    if ($null -ne $Entry.overlay_route_relay_discovery_seed_profile -and -not [string]::IsNullOrWhiteSpace([string]$Entry.overlay_route_relay_discovery_seed_profile)) {
        $seedProfile = [string]$Entry.overlay_route_relay_discovery_seed_profile
    }
    $seedFailoverStateFile = ""
    if ($null -ne $Entry.overlay_route_relay_discovery_seed_failover_state_file -and -not [string]::IsNullOrWhiteSpace([string]$Entry.overlay_route_relay_discovery_seed_failover_state_file)) {
        $seedFailoverStateFile = [string]$Entry.overlay_route_relay_discovery_seed_failover_state_file
    }
    $seedPriorityJson = ""
    if ($null -ne $Entry.overlay_route_relay_discovery_seed_priority -and -not [string]::IsNullOrWhiteSpace([string]$Entry.overlay_route_relay_discovery_seed_priority)) {
        $seedPriorityJson = [string]$Entry.overlay_route_relay_discovery_seed_priority
    }
    $seedSuccessRateThreshold = 0.5
    if ($null -ne $Entry.overlay_route_relay_discovery_seed_success_rate_threshold) {
        $parsedSeedSuccessRate = 0.0
        if ([double]::TryParse([string]$Entry.overlay_route_relay_discovery_seed_success_rate_threshold, [ref]$parsedSeedSuccessRate)) {
            $seedSuccessRateThreshold = [Math]::Min(1, [Math]::Max(0, $parsedSeedSuccessRate))
        }
    }
    $seedCooldownSeconds = 120
    if ($null -ne $Entry.overlay_route_relay_discovery_seed_cooldown_seconds) {
        $parsedSeedCooldown = 0
        if ([int]::TryParse([string]$Entry.overlay_route_relay_discovery_seed_cooldown_seconds, [ref]$parsedSeedCooldown)) {
            $seedCooldownSeconds = [Math]::Min(86400, [Math]::Max(1, $parsedSeedCooldown))
        }
    }
    $seedMaxConsecutiveFailures = 3
    if ($null -ne $Entry.overlay_route_relay_discovery_seed_max_consecutive_failures) {
        $parsedSeedConsecutive = 0
        if ([int]::TryParse([string]$Entry.overlay_route_relay_discovery_seed_max_consecutive_failures, [ref]$parsedSeedConsecutive)) {
            $seedMaxConsecutiveFailures = [Math]::Min(100, [Math]::Max(1, $parsedSeedConsecutive))
        }
    }
    $regionPriorityJson = ""
    if ($null -ne $Entry.overlay_route_relay_discovery_region_priority -and -not [string]::IsNullOrWhiteSpace([string]$Entry.overlay_route_relay_discovery_region_priority)) {
        $regionPriorityJson = [string]$Entry.overlay_route_relay_discovery_region_priority
    }
    $regionFailoverThreshold = 0.5
    if ($null -ne $Entry.overlay_route_relay_discovery_region_failover_threshold) {
        $parsedRegionThreshold = 0.0
        if ([double]::TryParse([string]$Entry.overlay_route_relay_discovery_region_failover_threshold, [ref]$parsedRegionThreshold)) {
            $regionFailoverThreshold = [Math]::Min(1, [Math]::Max(0, $parsedRegionThreshold))
        }
    }
    $regionCooldownSeconds = 120
    if ($null -ne $Entry.overlay_route_relay_discovery_region_cooldown_seconds) {
        $parsedRegionCooldown = 0
        if ([int]::TryParse([string]$Entry.overlay_route_relay_discovery_region_cooldown_seconds, [ref]$parsedRegionCooldown)) {
            $regionCooldownSeconds = [Math]::Min(86400, [Math]::Max(1, $parsedRegionCooldown))
        }
    }
    $relayScoreSmoothingAlpha = 0.3
    if ($null -ne $Entry.overlay_route_relay_discovery_relay_score_smoothing_alpha) {
        $parsedRelayScoreSmoothingAlpha = 0.0
        if ([double]::TryParse([string]$Entry.overlay_route_relay_discovery_relay_score_smoothing_alpha, [ref]$parsedRelayScoreSmoothingAlpha)) {
            $relayScoreSmoothingAlpha = [Math]::Min(1, [Math]::Max(0.01, $parsedRelayScoreSmoothingAlpha))
        }
    }

    $rustArgs = @(
        "--directory-file", $directoryFile,
        "--discovery-file", $discoveryFile,
        "--discovery-http-urls", $httpUrls,
        "--discovery-http-urls-file", $httpUrlsFile,
        "--seed-region", $seedRegion,
        "--seed-mode", $seedMode,
        "--seed-profile", $seedProfile,
        "--seed-failover-state-file", $seedFailoverStateFile,
        "--seed-priority-json", $seedPriorityJson,
        "--seed-success-rate-threshold", [string]$seedSuccessRateThreshold,
        "--seed-cooldown-seconds", [string]$seedCooldownSeconds,
        "--seed-max-consecutive-failures", [string]$seedMaxConsecutiveFailures,
        "--region-priority-json", $regionPriorityJson,
        "--region-failover-threshold", [string]$regionFailoverThreshold,
        "--region-cooldown-seconds", [string]$regionCooldownSeconds,
        "--relay-score-smoothing-alpha", [string]$relayScoreSmoothingAlpha,
        "--source-weights-json", $sourceWeightsJson,
        "--http-timeout-ms", [string]$httpTimeoutMs,
        "--default-source-weight", [string]1.0,
        "--default-health", [string]$defaultHealth,
        "--default-enabled", [string]$defaultEnabled,
        "--source-reputation-file", $sourceReputationFile,
        "--source-reputation-decay", [string]$sourceDecay,
        "--source-penalty-on-http-fail", [string]$sourcePenaltyOnFail,
        "--source-recover-on-success", [string]$sourceRecoverOnSuccess,
        "--source-blacklist-threshold", [string]$sourceBlacklistThreshold,
        "--source-denylist", $sourceDenylist
    )
    $psArgs = @(
        "-ExecutionPolicy", "Bypass",
        "-File", $mergeScript,
        "-DirectoryFile", $directoryFile,
        "-DiscoveryFile", $discoveryFile,
        "-DiscoveryHttpUrls", $httpUrls,
        "-DiscoveryHttpUrlsFile", $httpUrlsFile,
        "-SeedRegion", $seedRegion,
        "-SeedMode", $seedMode,
        "-SeedProfile", $seedProfile,
        "-SeedFailoverStateFile", $seedFailoverStateFile,
        "-SeedPriorityJson", $seedPriorityJson,
        "-SeedSuccessRateThreshold", [string]$seedSuccessRateThreshold,
        "-SeedCooldownSeconds", [string]$seedCooldownSeconds,
        "-SeedMaxConsecutiveFailures", [string]$seedMaxConsecutiveFailures,
        "-RegionPriorityJson", $regionPriorityJson,
        "-RegionFailoverThreshold", [string]$regionFailoverThreshold,
        "-RegionCooldownSeconds", [string]$regionCooldownSeconds,
        "-RelayScoreSmoothingAlpha", [string]$relayScoreSmoothingAlpha,
        "-SourceWeightsJson", $sourceWeightsJson,
        "-HttpTimeoutMs", [string]$httpTimeoutMs,
        "-DefaultHealth", [string]$defaultHealth,
        "-DefaultEnabled", [string]$defaultEnabled,
        "-SourceReputationFile", $sourceReputationFile,
        "-SourceReputationDecay", [string]$sourceDecay,
        "-SourcePenaltyOnHttpFail", [string]$sourcePenaltyOnFail,
        "-SourceRecoverOnSuccess", [string]$sourceRecoverOnSuccess,
        "-SourceBlacklistThreshold", [string]$sourceBlacklistThreshold,
        "-SourceDenylist", $sourceDenylist
    )
    if ([string]::IsNullOrWhiteSpace($mergeRustBinary) -and -not (Test-Path -LiteralPath $mergeScript)) {
        return [pscustomobject]@{
            invoked = $true
            ok = $false
            reason = "missing_discovery_runtime"
            message = ("missing rust binary and script fallback: " + $mergeScript)
        }
    }
    try {
        if (-not [string]::IsNullOrWhiteSpace($mergeRustBinary) -and (Test-Path -LiteralPath $mergeRustBinary)) {
            $output = & $mergeRustBinary @rustArgs 2>&1
            $exitCode = $LASTEXITCODE
        } else {
            $output = & powershell @psArgs 2>&1
            $exitCode = $LASTEXITCODE
        }
    } catch {
        return [pscustomobject]@{
            invoked = $true
            ok = $false
            reason = "exception"
            message = $_.Exception.Message
            default_health = $defaultHealth
            default_enabled = $defaultEnabled
            http_urls = $httpUrls
            http_timeout_ms = $httpTimeoutMs
            http_urls_file = $httpUrlsFile
            seed_region = $seedRegion
            seed_mode = $seedMode
            seed_profile = $seedProfile
            source_reputation_file = $sourceReputationFile
            source_decay = $sourceDecay
            source_penalty_on_fail = $sourcePenaltyOnFail
            source_recover_on_success = $sourceRecoverOnSuccess
            source_blacklist_threshold = $sourceBlacklistThreshold
            source_denylist = $sourceDenylist
            seed_failover_state_file = $seedFailoverStateFile
            seed_priority = $seedPriorityJson
            seed_success_rate_threshold = $seedSuccessRateThreshold
            seed_cooldown_seconds = $seedCooldownSeconds
            seed_max_consecutive_failures = $seedMaxConsecutiveFailures
            region_priority = $regionPriorityJson
            region_failover_threshold = $regionFailoverThreshold
            region_cooldown_seconds = $regionCooldownSeconds
            relay_score_smoothing_alpha = $relayScoreSmoothingAlpha
            relay_selected = ""
            relay_score = -1
            region_failover_reason = ""
            region_recover_at_unix_ms = [int64]0
        }
    }
    $outputText = (($output | Out-String).Trim())
    $seedSelectedRuntime = ""
    $seedFailoverReasonRuntime = ""
    $seedRecoverAtUnixMsRuntime = [int64]0
    $seedCooldownSkipRuntime = 0
    $relaySelectedRuntime = ""
    $relayScoreRuntime = -1.0
    $regionFailoverReasonRuntime = ""
    $regionRecoverAtUnixMsRuntime = [int64]0
    if (-not [string]::IsNullOrWhiteSpace($outputText)) {
        if ($outputText -match "seed_selected=([^ ]+)") {
            $seedSelectedRuntime = [string]$matches[1]
            if ($seedSelectedRuntime -eq "none") { $seedSelectedRuntime = "" }
        }
        if ($outputText -match "seed_failover_reason=([^ ]+)") {
            $seedFailoverReasonRuntime = [string]$matches[1]
            if ($seedFailoverReasonRuntime -eq "none") { $seedFailoverReasonRuntime = "" }
        }
        if ($outputText -match "seed_recover_at_unix_ms=([0-9]+)") {
            [int64]::TryParse([string]$matches[1], [ref]$seedRecoverAtUnixMsRuntime) | Out-Null
        }
        if ($outputText -match "seed_cooldown_skip=([0-9]+)") {
            [int]::TryParse([string]$matches[1], [ref]$seedCooldownSkipRuntime) | Out-Null
        }
        if ($outputText -match "relay_selected=([^ ]+)") {
            $relaySelectedRuntime = [string]$matches[1]
            if ($relaySelectedRuntime -eq "none") { $relaySelectedRuntime = "" }
        }
        if ($outputText -match "relay_score=([-0-9.]+)") {
            [double]::TryParse([string]$matches[1], [ref]$relayScoreRuntime) | Out-Null
        }
        if ($outputText -match "region_failover_reason=([^ ]+)") {
            $regionFailoverReasonRuntime = [string]$matches[1]
            if ($regionFailoverReasonRuntime -eq "none") { $regionFailoverReasonRuntime = "" }
        }
        if ($outputText -match "region_recover_at_unix_ms=([0-9]+)") {
            [int64]::TryParse([string]$matches[1], [ref]$regionRecoverAtUnixMsRuntime) | Out-Null
        }
    }
    if ($exitCode -eq 0) {
        $script:OverlayRelayDiscoveryLastRunByKey[$key] = $nowMs
        return [pscustomobject]@{
            invoked = $true
            ok = $true
            reason = "ok"
            message = $outputText
            default_health = $defaultHealth
            default_enabled = $defaultEnabled
            cooldown_seconds = $cooldownSeconds
            http_urls = $httpUrls
            http_timeout_ms = $httpTimeoutMs
            http_urls_file = $httpUrlsFile
            seed_region = $seedRegion
            seed_mode = $seedMode
            seed_profile = $seedProfile
            source_reputation_file = $sourceReputationFile
            source_decay = $sourceDecay
            source_penalty_on_fail = $sourcePenaltyOnFail
            source_recover_on_success = $sourceRecoverOnSuccess
            source_blacklist_threshold = $sourceBlacklistThreshold
            source_denylist = $sourceDenylist
            seed_failover_state_file = $seedFailoverStateFile
            seed_priority = $seedPriorityJson
            seed_success_rate_threshold = $seedSuccessRateThreshold
            seed_cooldown_seconds = $seedCooldownSeconds
            seed_max_consecutive_failures = $seedMaxConsecutiveFailures
            region_priority = $regionPriorityJson
            region_failover_threshold = $regionFailoverThreshold
            region_cooldown_seconds = $regionCooldownSeconds
            relay_score_smoothing_alpha = $relayScoreSmoothingAlpha
            seed_selected = $seedSelectedRuntime
            seed_failover_reason = $seedFailoverReasonRuntime
            seed_recover_at_unix_ms = $seedRecoverAtUnixMsRuntime
            seed_cooldown_skip = $seedCooldownSkipRuntime
            relay_selected = $relaySelectedRuntime
            relay_score = $relayScoreRuntime
            region_failover_reason = $regionFailoverReasonRuntime
            region_recover_at_unix_ms = $regionRecoverAtUnixMsRuntime
        }
    }
    return [pscustomobject]@{
        invoked = $true
        ok = $false
        reason = "non_zero_exit"
        message = $outputText
        default_health = $defaultHealth
        default_enabled = $defaultEnabled
        cooldown_seconds = $cooldownSeconds
        http_urls = $httpUrls
        http_timeout_ms = $httpTimeoutMs
        http_urls_file = $httpUrlsFile
        seed_region = $seedRegion
        seed_mode = $seedMode
        seed_profile = $seedProfile
        source_reputation_file = $sourceReputationFile
        source_decay = $sourceDecay
        source_penalty_on_fail = $sourcePenaltyOnFail
        source_recover_on_success = $sourceRecoverOnSuccess
        source_blacklist_threshold = $sourceBlacklistThreshold
        source_denylist = $sourceDenylist
        seed_failover_state_file = $seedFailoverStateFile
        seed_priority = $seedPriorityJson
        seed_success_rate_threshold = $seedSuccessRateThreshold
        seed_cooldown_seconds = $seedCooldownSeconds
        seed_max_consecutive_failures = $seedMaxConsecutiveFailures
        region_priority = $regionPriorityJson
        region_failover_threshold = $regionFailoverThreshold
        region_cooldown_seconds = $regionCooldownSeconds
        relay_score_smoothing_alpha = $relayScoreSmoothingAlpha
        seed_selected = $seedSelectedRuntime
        seed_failover_reason = $seedFailoverReasonRuntime
        seed_recover_at_unix_ms = $seedRecoverAtUnixMsRuntime
        seed_cooldown_skip = $seedCooldownSkipRuntime
        relay_selected = $relaySelectedRuntime
        relay_score = $relayScoreRuntime
        region_failover_reason = $regionFailoverReasonRuntime
        region_recover_at_unix_ms = $regionRecoverAtUnixMsRuntime
    }
}

function Try-RefreshOverlayRelayDirectoryHealth {
    param(
        [string]$RepoRoot,
        [object]$Entry
    )
    $enabled = Convert-ToBooleanLoose -Value $Entry.overlay_route_relay_health_refresh_enabled -Default $false
    if (-not $enabled) {
        return $null
    }
    $directoryFile = [string]$Entry.overlay_route_relay_directory_file
    if ([string]::IsNullOrWhiteSpace($directoryFile)) {
        return [pscustomobject]@{
            invoked = $true
            ok = $false
            reason = "missing_directory"
            message = "overlay_route_relay_directory_file is empty"
        }
    }

    $cooldownSeconds = 30
    if ($null -ne $Entry.overlay_route_relay_health_refresh_cooldown_seconds) {
        $parsedCooldown = 0
        if ([int]::TryParse([string]$Entry.overlay_route_relay_health_refresh_cooldown_seconds, [ref]$parsedCooldown)) {
            $cooldownSeconds = [Math]::Min(3600, [Math]::Max(1, $parsedCooldown))
        }
    }
    if ($null -eq $script:OverlayRelayHealthRefreshLastRunByFile) {
        $script:OverlayRelayHealthRefreshLastRunByFile = @{}
    }
    $key = $directoryFile.ToLowerInvariant()
    $nowMs = [int64](Now-Ms)
    if ($script:OverlayRelayHealthRefreshLastRunByFile.ContainsKey($key)) {
        $lastMs = [int64]$script:OverlayRelayHealthRefreshLastRunByFile[$key]
        $cooldownMs = [int64]$cooldownSeconds * 1000
        if (($nowMs - $lastMs) -lt $cooldownMs) {
            return [pscustomobject]@{
                invoked = $false
                ok = $true
                reason = "cooldown"
                message = ("cooldown active, left_ms=" + [string]($cooldownMs - ($nowMs - $lastMs)))
            }
        }
    }

    $refreshScript = Join-Path $RepoRoot "scripts/novovm-overlay-relay-health-refresh.ps1"
    $refreshBinaryCfg = Resolve-PolicyToolBinaryConfig -RepoRootPath $RepoRoot -RawBinaryFile ([string]$Entry.overlay_route_relay_health_refresh_binary_path) -ToolName "overlay-relay-health-refresh" -LegacyBaseName "novovm-overlay-relay-health-refresh"
    $refreshRustBinary = [string]$refreshBinaryCfg.binary_path

    $mode = "auto"
    if (-not [string]::IsNullOrWhiteSpace([string]$Entry.overlay_route_relay_health_refresh_mode)) {
        $mode = ([string]$Entry.overlay_route_relay_health_refresh_mode).Trim().ToLowerInvariant()
    }
    $timeoutMs = 800
    if ($null -ne $Entry.overlay_route_relay_health_refresh_timeout_ms) {
        $parsedTimeout = 0
        if ([int]::TryParse([string]$Entry.overlay_route_relay_health_refresh_timeout_ms, [ref]$parsedTimeout)) {
            $timeoutMs = [Math]::Min(15000, [Math]::Max(100, $parsedTimeout))
        }
    }
    $alpha = 0.2
    if ($null -ne $Entry.overlay_route_relay_health_refresh_alpha) {
        $parsedAlpha = 0.0
        if ([double]::TryParse([string]$Entry.overlay_route_relay_health_refresh_alpha, [ref]$parsedAlpha)) {
            $alpha = [Math]::Min(1, [Math]::Max(0.01, $parsedAlpha))
        }
    }

    $rustArgs = @(
        "--directory-file", $directoryFile,
        "--mode", $mode,
        "--probe-timeout-ms", [string]$timeoutMs,
        "--alpha", [string]$alpha
    )
    $psArgs = @(
        "-ExecutionPolicy", "Bypass",
        "-File", $refreshScript,
        "-DirectoryFile", $directoryFile,
        "-Mode", $mode,
        "-ProbeTimeoutMs", [string]$timeoutMs,
        "-Alpha", [string]$alpha
    )
    if ([string]::IsNullOrWhiteSpace($refreshRustBinary) -and -not (Test-Path -LiteralPath $refreshScript)) {
        return [pscustomobject]@{
            invoked = $true
            ok = $false
            reason = "missing_health_runtime"
            message = ("missing rust binary and script fallback: " + $refreshScript)
            mode = $mode
            timeout_ms = $timeoutMs
            alpha = $alpha
        }
    }
    try {
        if (-not [string]::IsNullOrWhiteSpace($refreshRustBinary) -and (Test-Path -LiteralPath $refreshRustBinary)) {
            $output = & $refreshRustBinary @rustArgs 2>&1
            $exitCode = $LASTEXITCODE
        } else {
            $output = & powershell @psArgs 2>&1
            $exitCode = $LASTEXITCODE
        }
    } catch {
        return [pscustomobject]@{
            invoked = $true
            ok = $false
            reason = "exception"
            message = $_.Exception.Message
            mode = $mode
            timeout_ms = $timeoutMs
            alpha = $alpha
        }
    }
    $outputText = (($output | Out-String).Trim())
    if ($exitCode -eq 0) {
        $script:OverlayRelayHealthRefreshLastRunByFile[$key] = $nowMs
        return [pscustomobject]@{
            invoked = $true
            ok = $true
            reason = "ok"
            message = $outputText
            mode = $mode
            timeout_ms = $timeoutMs
            alpha = $alpha
        }
    }
    return [pscustomobject]@{
        invoked = $true
        ok = $false
        reason = "non_zero_exit"
        message = $outputText
        mode = $mode
        timeout_ms = $timeoutMs
        alpha = $alpha
    }
}

function New-AdaptiveState {
    return [pscustomobject]@{
        version = 1
        updated_at = ""
        regions = @{}
    }
}

function Load-AdaptiveState {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path)) {
        return (New-AdaptiveState)
    }
    $raw = Get-Content -LiteralPath $Path -Raw
    if ([string]::IsNullOrWhiteSpace($raw)) {
        return (New-AdaptiveState)
    }
    try {
        $obj = $raw | ConvertFrom-Json -ErrorAction Stop
    } catch {
        return (New-AdaptiveState)
    }
    $state = New-AdaptiveState
    if ($null -ne $obj.version) {
        $state.version = [int]$obj.version
    }
    if ($null -ne $obj.updated_at) {
        $state.updated_at = [string]$obj.updated_at
    }
    if ($null -ne $obj.regions) {
        $map = @{}
        foreach ($p in $obj.regions.PSObject.Properties) {
            $ema = 0.0
            $samples = 0
            if ($null -ne $p.Value.failure_rate_ema) {
                $ema = [double]$p.Value.failure_rate_ema
            }
            if ($null -ne $p.Value.samples) {
                $samples = [int]$p.Value.samples
            }
            $map[$p.Name] = [pscustomobject]@{
                failure_rate_ema = $ema
                samples = $samples
            }
        }
        $state.regions = $map
    }
    return $state
}

function Save-AdaptiveState {
    param(
        [string]$Path,
        [pscustomobject]$State
    )
    Ensure-ParentDir -PathValue $Path
    $State.updated_at = [DateTime]::UtcNow.ToString("o")
    $json = $State | ConvertTo-Json -Depth 12
    [System.IO.File]::WriteAllText($Path, $json, [System.Text.Encoding]::UTF8)
}

function Adaptive-RegionEntry {
    param([string]$Region)
    $k = Region-Key -Region $Region
    if (-not $script:AdaptiveState.regions.ContainsKey($k)) {
        $script:AdaptiveState.regions[$k] = [pscustomobject]@{
            failure_rate_ema = 0.0
            samples = 0
        }
    }
    return $script:AdaptiveState.regions[$k]
}

function Adaptive-Observe {
    param(
        [string]$Region,
        [bool]$Failed
    )
    if (-not $script:AdaptiveEnabled) {
        return
    }
    $entry = Adaptive-RegionEntry -Region $Region
    $obs = 0.0
    if ($Failed) {
        $obs = 1.0
    }
    $entry.failure_rate_ema = ($script:AdaptiveAlpha * $obs) + ((1.0 - $script:AdaptiveAlpha) * [double]$entry.failure_rate_ema)
    $entry.samples = [int]$entry.samples + 1
    $script:AdaptiveDirty = $true
}

function Adaptive-FailureEma {
    param([string]$Region)
    if (-not $script:AdaptiveEnabled) {
        return 0.0
    }
    $entry = Adaptive-RegionEntry -Region $Region
    return [double]$entry.failure_rate_ema
}

function Effective-RegionCap {
    param(
        [hashtable]$Caps,
        [string]$Region,
        [int]$Fallback,
        [int]$PendingCount
    )
    $baseCap = Region-Cap -Caps $Caps -Region $Region -Fallback $Fallback
    if (-not $script:AdaptiveEnabled) {
        return $baseCap
    }
    $ema = Adaptive-FailureEma -Region $Region
    $cap = $baseCap
    if ($ema -ge $script:AdaptiveHighFailureRate) {
        $cap = [Math]::Max(1, $baseCap - 1)
    } elseif ($ema -le $script:AdaptiveLowFailureRate -and $PendingCount -gt $baseCap) {
        $cap = [Math]::Min($Fallback, $baseCap + $script:AdaptiveMaxCapBoost)
    }
    return $cap
}

function Effective-RetryDelaySec {
    param(
        [int]$BaseDelaySec,
        [string]$Region
    )
    $baseDelay = [Math]::Max(1, $BaseDelaySec)
    if (-not $script:AdaptiveEnabled) {
        return $baseDelay
    }
    $ema = Adaptive-FailureEma -Region $Region
    $mult = 1.0 + [Math]::Min(2.0, ($ema * 2.0))
    return [int][Math]::Max(1, [Math]::Round($baseDelay * $mult))
}

function New-DedupeState {
    return [pscustomobject]@{
        version = 1
        updated_at = ""
        entries = @{}
    }
}

function Load-DedupeState {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path)) {
        return (New-DedupeState)
    }
    $raw = Get-Content -LiteralPath $Path -Raw
    if ([string]::IsNullOrWhiteSpace($raw)) {
        return (New-DedupeState)
    }
    try {
        $obj = $raw | ConvertFrom-Json -ErrorAction Stop
    } catch {
        return (New-DedupeState)
    }
    $state = New-DedupeState
    if ($null -ne $obj.version) { $state.version = [int]$obj.version }
    if ($null -ne $obj.updated_at) { $state.updated_at = [string]$obj.updated_at }
    if ($null -ne $obj.entries) {
        $map = @{}
        foreach ($p in $obj.entries.PSObject.Properties) {
            $map[$p.Name] = [pscustomobject]@{
                status = [string]$p.Value.status
                operation_id = [string]$p.Value.operation_id
                updated_unix_ms = [int64]$p.Value.updated_unix_ms
            }
        }
        $state.entries = $map
    }
    return $state
}

function Save-DedupeState {
    param(
        [string]$Path,
        [pscustomobject]$State
    )
    Ensure-ParentDir -PathValue $Path
    $State.updated_at = [DateTime]::UtcNow.ToString("o")
    $json = $State | ConvertTo-Json -Depth 12
    [System.IO.File]::WriteAllText($Path, $json, [System.Text.Encoding]::UTF8)
}

function Cleanup-DedupeState {
    param(
        [pscustomobject]$State,
        [int]$TtlSec
    )
    $now = Now-Ms
    $expired = @()
    foreach ($k in $State.entries.Keys) {
        $age = $now - [int64]$State.entries[$k].updated_unix_ms
        if ($age -gt ([int64]$TtlSec * 1000)) {
            $expired += $k
        }
    }
    foreach ($k in $expired) {
        $null = $State.entries.Remove($k)
    }
}

function Dedupe-Key {
    param([object]$Entry)
    return ("plan={0}|action={1}|file={2}|target={3}|rollback={4}" -f [string]$Entry.name, [string]$Entry.action, [string]$Entry.plan_file, [string]$Entry.target, [string]$Entry.rollback)
}

function Try-Reserve-Dedupe {
    param(
        [object]$Entry,
        [string]$ControlOpId
    )
    $key = Dedupe-Key -Entry $Entry
    Cleanup-DedupeState -State $script:DedupeState -TtlSec $script:DedupeTtlSec
    if ($script:DedupeState.entries.ContainsKey($key)) {
        $e = $script:DedupeState.entries[$key]
        if ([string]$e.operation_id -ne $ControlOpId -and ([string]$e.status -eq "in_progress" -or [string]$e.status -eq "done")) {
            return [pscustomobject]@{
                ok = $false
                key = $key
                reason = ("dedupe conflict status=" + [string]$e.status + " operation_id=" + [string]$e.operation_id)
            }
        }
    }
    $script:DedupeState.entries[$key] = [pscustomobject]@{
        status = "in_progress"
        operation_id = $ControlOpId
        updated_unix_ms = [int64](Now-Ms)
    }
    Save-DedupeState -Path $script:DedupePath -State $script:DedupeState
    return [pscustomobject]@{
        ok = $true
        key = $key
        reason = ""
    }
}

function Finalize-Dedupe {
    param(
        [object]$Entry,
        [string]$ControlOpId,
        [string]$Status
    )
    $key = Dedupe-Key -Entry $Entry
    Cleanup-DedupeState -State $script:DedupeState -TtlSec $script:DedupeTtlSec
    if (-not $script:DedupeState.entries.ContainsKey($key)) {
        return
    }
    $e = $script:DedupeState.entries[$key]
    if ([string]$e.operation_id -ne $ControlOpId) {
        return
    }
    $e.status = $Status
    $e.updated_unix_ms = [int64](Now-Ms)
    Save-DedupeState -Path $script:DedupePath -State $script:DedupeState
}

function Try-Acquire-LeaseLock {
    param([string]$Path)
    Ensure-ParentDir -PathValue $Path
    try {
        $stream = [System.IO.File]::Open($Path, [System.IO.FileMode]::OpenOrCreate, [System.IO.FileAccess]::ReadWrite, [System.IO.FileShare]::None)
        return $stream
    } catch {
        return $null
    }
}

function Write-LeaseHeartbeat {
    param(
        [System.IO.FileStream]$Stream,
        [string]$Controller,
        [string]$Operation,
        [int]$TtlSec
    )
    $now = Now-Ms
    $obj = [pscustomobject]@{
        version = 1
        controller_id = $Controller
        operation_id = $Operation
        updated_unix_ms = $now
        expires_unix_ms = $now + ([int64]$TtlSec * 1000)
    }
    $json = $obj | ConvertTo-Json -Compress -Depth 4
    $bytes = [System.Text.Encoding]::UTF8.GetBytes($json)
    $Stream.SetLength(0)
    $Stream.Position = 0
    $Stream.Write($bytes, 0, $bytes.Length)
    $Stream.Flush()
}

function New-SiteConsensusState {
    return [pscustomobject]@{
        version = 1
        updated_at = ""
        entries = @{}
    }
}

function Load-SiteConsensusState {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path)) {
        return (New-SiteConsensusState)
    }
    $raw = Get-Content -LiteralPath $Path -Raw
    if ([string]::IsNullOrWhiteSpace($raw)) {
        return (New-SiteConsensusState)
    }
    try {
        $obj = $raw | ConvertFrom-Json -ErrorAction Stop
    } catch {
        return (New-SiteConsensusState)
    }
    $state = New-SiteConsensusState
    if ($null -ne $obj.version) { $state.version = [int]$obj.version }
    if ($null -ne $obj.updated_at) { $state.updated_at = [string]$obj.updated_at }
    if ($null -ne $obj.entries) {
        $map = @{}
        foreach ($p in $obj.entries.PSObject.Properties) {
            $item = $p.Value
            $votes = @{}
            if ($null -ne $item.votes) {
                foreach ($v in $item.votes.PSObject.Properties) {
                    $votes[$v.Name] = [pscustomobject]@{
                        operation_id = [string]$v.Value.operation_id
                        priority = [int]$v.Value.priority
                        updated_unix_ms = [int64]$v.Value.updated_unix_ms
                    }
                }
            }
            $map[$p.Name] = [pscustomobject]@{
                committed_operation_id = [string]$item.committed_operation_id
                committed_site_id = [string]$item.committed_site_id
                committed_unix_ms = [int64]$item.committed_unix_ms
                votes = $votes
            }
        }
        $state.entries = $map
    }
    return $state
}

function Save-SiteConsensusState {
    param(
        [string]$Path,
        [pscustomobject]$State
    )
    Ensure-ParentDir -PathValue $Path
    $State.updated_at = [DateTime]::UtcNow.ToString("o")
    $json = $State | ConvertTo-Json -Depth 14
    [System.IO.File]::WriteAllText($Path, $json, [System.Text.Encoding]::UTF8)
}

function New-SiteConflictAccountabilityState {
    return [pscustomobject]@{
        version = 1
        updated_at = ""
        max_penalty_points = 0
        recovery_per_win = 0
        reputation_aging_enabled = $false
        reputation_aging_interval_seconds = 3600
        reputation_recover_points_per_interval = 1
        reputation_recover_idle_seconds = 1800
        last_aging_unix_ms = 0
        sites = @{}
    }
}

function Load-SiteConflictAccountabilityState {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path)) {
        return (New-SiteConflictAccountabilityState)
    }
    $raw = Get-Content -LiteralPath $Path -Raw
    if ([string]::IsNullOrWhiteSpace($raw)) {
        return (New-SiteConflictAccountabilityState)
    }
    try {
        $obj = $raw | ConvertFrom-Json -ErrorAction Stop
    } catch {
        return (New-SiteConflictAccountabilityState)
    }
    $state = New-SiteConflictAccountabilityState
    if ($null -ne $obj.version) { $state.version = [int]$obj.version }
    if ($null -ne $obj.updated_at) { $state.updated_at = [string]$obj.updated_at }
    if ($null -ne $obj.max_penalty_points) { $state.max_penalty_points = [Math]::Max(0, [int]$obj.max_penalty_points) }
    if ($null -ne $obj.recovery_per_win) { $state.recovery_per_win = [Math]::Max(0, [int]$obj.recovery_per_win) }
    if ($null -ne $obj.reputation_aging_enabled) { $state.reputation_aging_enabled = [bool]$obj.reputation_aging_enabled }
    if ($null -ne $obj.reputation_aging_interval_seconds) { $state.reputation_aging_interval_seconds = [Math]::Max(60, [int]$obj.reputation_aging_interval_seconds) }
    if ($null -ne $obj.reputation_recover_points_per_interval) { $state.reputation_recover_points_per_interval = [Math]::Max(0, [int]$obj.reputation_recover_points_per_interval) }
    if ($null -ne $obj.reputation_recover_idle_seconds) { $state.reputation_recover_idle_seconds = [Math]::Max(0, [int]$obj.reputation_recover_idle_seconds) }
    if ($null -ne $obj.last_aging_unix_ms) { $state.last_aging_unix_ms = [int64]$obj.last_aging_unix_ms }
    if ($null -ne $obj.sites) {
        $map = @{}
        foreach ($p in $obj.sites.PSObject.Properties) {
            $item = $p.Value
            $counts = @{}
            if ($null -ne $item.event_counts) {
                foreach ($ec in $item.event_counts.PSObject.Properties) {
                    $counts[$ec.Name] = [Math]::Max(0, [int]$ec.Value)
                }
            }
            $map[$p.Name] = [pscustomobject]@{
                penalty_points = [Math]::Max(0, [int]$item.penalty_points)
                reputation_score = [Math]::Max(0.0, [Math]::Min(100.0, [double]$item.reputation_score))
                last_event = [string]$item.last_event
                last_rule = [string]$item.last_rule
                last_reason = [string]$item.last_reason
                last_unix_ms = [int64]$item.last_unix_ms
                last_penalty_unix_ms = [int64]$item.last_penalty_unix_ms
                last_recover_unix_ms = [int64]$item.last_recover_unix_ms
                event_counts = $counts
            }
        }
        $state.sites = $map
    }
    return $state
}

function Save-SiteConflictAccountabilityState {
    param(
        [string]$Path,
        [pscustomobject]$State
    )
    Ensure-ParentDir -PathValue $Path
    $State.updated_at = [DateTime]::UtcNow.ToString("o")
    $json = $State | ConvertTo-Json -Depth 16
    [System.IO.File]::WriteAllText($Path, $json, [System.Text.Encoding]::UTF8)
}

function Resolve-SiteConflictPenaltyMatrix {
    param([object[]]$RawRules)
    $rules = @()
    $order = 0
    foreach ($r in @($RawRules)) {
        if ($null -eq $r) {
            continue
        }
        $order += 1
        $name = [string]$r.name
        if ([string]::IsNullOrWhiteSpace($name)) {
            $name = ("rule-" + $order)
        }
        $event = "*"
        if ($null -ne $r.event -and -not [string]::IsNullOrWhiteSpace([string]$r.event)) {
            $event = ([string]$r.event).ToLowerInvariant()
        }
        $role = "*"
        if ($null -ne $r.role -and -not [string]::IsNullOrWhiteSpace([string]$r.role)) {
            $role = ([string]$r.role).ToLowerInvariant()
        }
        $site = "*"
        if ($null -ne $r.site -and -not [string]::IsNullOrWhiteSpace([string]$r.site)) {
            $site = [string]$r.site
        }
        $delta = 0
        if ($null -ne $r.penalty_points) {
            $delta = [int]$r.penalty_points
        }
        $rules += [pscustomobject]@{
            order = $order
            name = $name
            event = $event
            role = $role
            site = $site
            penalty_points = $delta
        }
    }
    if ($rules.Count -eq 0) {
        $rules += [pscustomobject]@{ order = 1; name = "default-conflict-loser"; event = "consensus_conflict_loser"; role = "loser"; site = "*"; penalty_points = 5 }
        $rules += [pscustomobject]@{ order = 2; name = "default-committed-other"; event = "consensus_committed_other"; role = "self"; site = "*"; penalty_points = 3 }
        $rules += [pscustomobject]@{ order = 3; name = "default-winner-recover"; event = "consensus_winner"; role = "winner"; site = "*"; penalty_points = -1 }
    }
    return @($rules | Sort-Object @{ Expression = "order"; Descending = $false })
}

function Match-SiteConflictPenaltyRule {
    param(
        [object[]]$Rules,
        [string]$Event,
        [string]$Role,
        [string]$Site
    )
    $event = [string]$Event
    if ([string]::IsNullOrWhiteSpace($event)) { $event = "*" }
    $event = $event.ToLowerInvariant()
    $role = [string]$Role
    if ([string]::IsNullOrWhiteSpace($role)) { $role = "*" }
    $role = $role.ToLowerInvariant()
    foreach ($r in @($Rules)) {
        if ([string]$r.event -ne "*" -and [string]$r.event -ne $event) {
            continue
        }
        if ([string]$r.role -ne "*" -and [string]$r.role -ne $role) {
            continue
        }
        if ([string]$r.site -ne "*" -and [string]$r.site -ne $Site) {
            continue
        }
        return $r
    }
    return $null
}

function Ensure-SiteConflictStateRecord {
    param([string]$Site)
    if (-not $script:SiteConflictAccountabilityState.sites.ContainsKey($Site)) {
        $script:SiteConflictAccountabilityState.sites[$Site] = [pscustomobject]@{
            penalty_points = 0
            reputation_score = 100.0
            last_event = ""
            last_rule = ""
            last_reason = ""
            last_unix_ms = 0
            last_penalty_unix_ms = 0
            last_recover_unix_ms = 0
            event_counts = @{}
        }
    }
    return $script:SiteConflictAccountabilityState.sites[$Site]
}

function Resolve-SitePriorityMap {
    param(
        [object]$RawMap,
        [bool]$NormalizeRegion = $false
    )
    $map = @{}
    if ($null -eq $RawMap) {
        return $map
    }
    foreach ($p in $RawMap.PSObject.Properties) {
        $k = ([string]$p.Name).Trim()
        if ([string]::IsNullOrWhiteSpace($k)) { continue }
        if ($NormalizeRegion) {
            $k = $k.ToUpperInvariant()
        }
        try {
            $map[$k] = [int]$p.Value
        } catch {
            continue
        }
    }
    return $map
}

function Site-BasePriority {
    param([string]$Site)
    $site = ""
    if ($null -ne $Site) {
        $site = ([string]$Site).Trim()
    }
    if (-not [string]::IsNullOrWhiteSpace($site) -and $null -ne $script:SitePriorityMapBySite -and $script:SitePriorityMapBySite.ContainsKey($site)) {
        return [int]$script:SitePriorityMapBySite[$site]
    }
    $region = ""
    if (-not [string]::IsNullOrWhiteSpace($site) -and $null -ne $script:SiteRegionMap -and $script:SiteRegionMap.ContainsKey($site)) {
        $region = [string]$script:SiteRegionMap[$site]
    }
    if (-not [string]::IsNullOrWhiteSpace($region) -and $null -ne $script:SitePriorityMapByRegion -and $script:SitePriorityMapByRegion.ContainsKey($region)) {
        return [int]$script:SitePriorityMapByRegion[$region]
    }
    if ($null -ne $script:SitePriorityDefault) {
        return [int]$script:SitePriorityDefault
    }
    return 0
}

function Apply-SiteConflictPenaltyEvent {
    param(
        [string]$Site,
        [string]$Event,
        [string]$Role,
        [string]$Reason
    )
    $record = Ensure-SiteConflictStateRecord -Site $Site
    $rule = Match-SiteConflictPenaltyRule -Rules $script:SiteConflictPenaltyMatrix -Event $Event -Role $Role -Site $Site
    $delta = 0
    $ruleName = "no_rule"
    if ($null -ne $rule) {
        $delta = [int]$rule.penalty_points
        $ruleName = [string]$rule.name
    } elseif ([string]$Event -eq "consensus_winner") {
        $delta = -[Math]::Max(0, [int]$script:SiteConflictRecoveryPerWin)
        $ruleName = "auto-winner-recover"
    }
    $oldPenalty = [Math]::Max(0, [int]$record.penalty_points)
    $newPenalty = $oldPenalty + $delta
    if ($newPenalty -lt 0) { $newPenalty = 0 }
    $maxPenalty = [Math]::Max(0, [int]$script:SiteConflictMaxPenaltyPoints)
    if ($newPenalty -gt $maxPenalty) { $newPenalty = $maxPenalty }
    $counts = $record.event_counts
    if (-not $counts.ContainsKey($Event)) {
        $counts[$Event] = 0
    }
    $counts[$Event] = [int]$counts[$Event] + 1
    $now = [int64](Now-Ms)
    $record.penalty_points = $newPenalty
    $maxPenaltySafe = [Math]::Max(1, $maxPenalty)
    $score = 100.0 - (([double]$newPenalty / [double]$maxPenaltySafe) * 100.0)
    if ($score -lt 0.0) { $score = 0.0 }
    if ($score -gt 100.0) { $score = 100.0 }
    $record.reputation_score = [Math]::Round($score, 4)
    $record.last_event = $Event
    $record.last_rule = $ruleName
    $record.last_reason = $Reason
    $record.last_unix_ms = $now
    if ($delta -gt 0) {
        $record.last_penalty_unix_ms = $now
    } elseif ($delta -lt 0) {
        $record.last_recover_unix_ms = $now
    }
    $record.event_counts = $counts
    $basePriority = Site-BasePriority -Site $Site
    $effectivePriority = $basePriority - $newPenalty
    if ($effectivePriority -lt 0) { $effectivePriority = 0 }
    return [pscustomobject]@{
        site_id = $Site
        event = $Event
        role = $Role
        rule = $ruleName
        delta = $delta
        old_penalty = $oldPenalty
        new_penalty = $newPenalty
        base_priority = $basePriority
        effective_priority = $effectivePriority
        reputation_score = [double]$record.reputation_score
        reason = $Reason
    }
}

function Apply-SiteConsensusAccountability {
    param(
        [string]$Key,
        [object]$Winner,
        [hashtable]$Votes
    )
    $events = @()
    if (-not $script:SiteConflictAccountabilityEnabled) {
        return $events
    }
    foreach ($site in @($Votes.Keys)) {
        $v = $Votes[$site]
        $siteRole = "follower"
        $event = "consensus_follow_winner"
        if ([string]$site -eq [string]$Winner.site_id) {
            $siteRole = "winner"
            $event = "consensus_winner"
        } elseif ([string]$v.operation_id -ne [string]$Winner.operation_id) {
            $siteRole = "loser"
            $event = "consensus_conflict_loser"
        }
        $reason = ("key=" + $Key + " winner_site=" + [string]$Winner.site_id + " winner_operation_id=" + [string]$Winner.operation_id)
        $events += Apply-SiteConflictPenaltyEvent -Site $site -Event $event -Role $siteRole -Reason $reason
    }
    Save-SiteConflictAccountabilityState -Path $script:SiteConflictAccountabilityPath -State $script:SiteConflictAccountabilityState
    return $events
}

function Apply-SiteConflictReputationAging {
    if (-not $script:SiteConflictAccountabilityEnabled) {
        return @()
    }
    if (-not $script:SiteConflictReputationAgingEnabled) {
        return @()
    }
    $intervalSec = [Math]::Max(60, [int]$script:SiteConflictReputationAgingIntervalSec)
    $now = [int64](Now-Ms)
    if ($script:SiteConflictAccountabilityState.last_aging_unix_ms -gt 0) {
        $elapsed = $now - [int64]$script:SiteConflictAccountabilityState.last_aging_unix_ms
        if ($elapsed -lt ([int64]$intervalSec * 1000)) {
            return @()
        }
    }
    $events = @()
    $recoverPoints = [Math]::Max(0, [int]$script:SiteConflictReputationRecoverPointsPerInterval)
    $idleMs = [int64]([Math]::Max(0, [int]$script:SiteConflictReputationRecoverIdleSec) * 1000)
    if ($recoverPoints -le 0) {
        $script:SiteConflictAccountabilityState.last_aging_unix_ms = $now
        Save-SiteConflictAccountabilityState -Path $script:SiteConflictAccountabilityPath -State $script:SiteConflictAccountabilityState
        return @()
    }
    foreach ($site in @($script:SiteConflictAccountabilityState.sites.Keys)) {
        $record = $script:SiteConflictAccountabilityState.sites[$site]
        $oldPenalty = [Math]::Max(0, [int]$record.penalty_points)
        if ($oldPenalty -le 0) {
            continue
        }
        $lastPenaltyMs = [int64]$record.last_penalty_unix_ms
        if ($lastPenaltyMs -gt 0 -and ($now - $lastPenaltyMs) -lt $idleMs) {
            continue
        }
        $newPenalty = $oldPenalty - $recoverPoints
        if ($newPenalty -lt 0) { $newPenalty = 0 }
        if ($newPenalty -eq $oldPenalty) {
            continue
        }
        $record.penalty_points = $newPenalty
        $maxPenaltySafe = [Math]::Max(1, [int]$script:SiteConflictMaxPenaltyPoints)
        $score = 100.0 - (([double]$newPenalty / [double]$maxPenaltySafe) * 100.0)
        if ($score -lt 0.0) { $score = 0.0 }
        if ($score -gt 100.0) { $score = 100.0 }
        $record.reputation_score = [Math]::Round($score, 4)
        $record.last_recover_unix_ms = $now
        $record.last_event = "reputation_aging_recover"
        $record.last_rule = "reputation-aging"
        $record.last_reason = ("idle_recover points=" + $recoverPoints)
        $record.last_unix_ms = $now
        $basePriority = Site-BasePriority -Site $site
        $effectivePriority = $basePriority - $newPenalty
        if ($effectivePriority -lt 0) { $effectivePriority = 0 }
        $events += [pscustomobject]@{
            site_id = $site
            old_penalty = $oldPenalty
            new_penalty = $newPenalty
            recover_points = $recoverPoints
            base_priority = $basePriority
            effective_priority = $effectivePriority
            reputation_score = [double]$record.reputation_score
        }
    }
    $script:SiteConflictAccountabilityState.last_aging_unix_ms = $now
    Save-SiteConflictAccountabilityState -Path $script:SiteConflictAccountabilityPath -State $script:SiteConflictAccountabilityState
    return $events
}

function New-SiteConflictRiskState {
    return [pscustomobject]@{
        version = 1
        updated_at = ""
        ema_alpha = 0.2
        last_predict_unix_ms = 0
        sites = @{}
        summary = [pscustomobject]@{
            worst_site_id = ""
            worst_level = "green"
            worst_score = 0.0
            total_sites = 0
        }
    }
}

function Load-SiteConflictRiskState {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path)) {
        return (New-SiteConflictRiskState)
    }
    $raw = Get-Content -LiteralPath $Path -Raw
    if ([string]::IsNullOrWhiteSpace($raw)) {
        return (New-SiteConflictRiskState)
    }
    try {
        $obj = $raw | ConvertFrom-Json -ErrorAction Stop
    } catch {
        return (New-SiteConflictRiskState)
    }
    $state = New-SiteConflictRiskState
    if ($null -ne $obj.version) { $state.version = [int]$obj.version }
    if ($null -ne $obj.updated_at) { $state.updated_at = [string]$obj.updated_at }
    if ($null -ne $obj.ema_alpha) { $state.ema_alpha = [double]$obj.ema_alpha }
    if ($null -ne $obj.last_predict_unix_ms) { $state.last_predict_unix_ms = [int64]$obj.last_predict_unix_ms }
    if ($null -ne $obj.sites) {
        $map = @{}
        foreach ($p in $obj.sites.PSObject.Properties) {
            $it = $p.Value
            $map[$p.Name] = [pscustomobject]@{
                raw_score = [double]$it.raw_score
                ema_score = [double]$it.ema_score
                trend = [double]$it.trend
                level = [string]$it.level
                updated_unix_ms = [int64]$it.updated_unix_ms
            }
        }
        $state.sites = $map
    }
    if ($null -ne $obj.summary) {
        $state.summary.worst_site_id = [string]$obj.summary.worst_site_id
        $state.summary.worst_level = [string]$obj.summary.worst_level
        $state.summary.worst_score = [double]$obj.summary.worst_score
        $state.summary.total_sites = [int]$obj.summary.total_sites
    }
    return $state
}

function Save-SiteConflictRiskState {
    param(
        [string]$Path,
        [pscustomobject]$State
    )
    Ensure-ParentDir -PathValue $Path
    $State.updated_at = [DateTime]::UtcNow.ToString("o")
    $json = $State | ConvertTo-Json -Depth 16
    [System.IO.File]::WriteAllText($Path, $json, [System.Text.Encoding]::UTF8)
}

function Score-ToRiskLevel {
    param([double]$Score)
    if ($Score -ge 85.0) { return "red" }
    if ($Score -ge 70.0) { return "orange" }
    if ($Score -ge 45.0) { return "yellow" }
    return "green"
}

function Resolve-DefaultRolloutPolicyCliBinaryPath {
    param(
        [string]$RepoRootPath
    )
    if (-not [string]::IsNullOrWhiteSpace([string]$script:RolloutPolicyCliBinaryPath) -and (Test-Path -LiteralPath $script:RolloutPolicyCliBinaryPath)) {
        return [string]$script:RolloutPolicyCliBinaryPath
    }
    foreach ($candidate in @(
            (Join-Path $RepoRootPath "target/release/novovm-rollout-policy.exe"),
            (Join-Path $RepoRootPath "target/release/novovm-rollout-policy"),
            (Join-Path $RepoRootPath "target/debug/novovm-rollout-policy.exe"),
            (Join-Path $RepoRootPath "target/debug/novovm-rollout-policy")
        )) {
        if (Test-Path -LiteralPath $candidate) {
            try {
                return (Resolve-Path -LiteralPath $candidate -ErrorAction Stop).Path
            } catch {
                return $candidate
            }
        }
    }
    $cmd = Get-Command -Name "novovm-rollout-policy" -ErrorAction SilentlyContinue
    if ($null -ne $cmd) {
        $cmdPath = [string]$cmd.Source
        if ([string]::IsNullOrWhiteSpace($cmdPath)) {
            $cmdPath = [string]$cmd.Path
        }
        if (-not [string]::IsNullOrWhiteSpace($cmdPath)) {
            return $cmdPath
        }
    }
    return ""
}

function Resolve-PolicyToolBinaryConfig {
    param(
        [string]$RepoRootPath,
        [string]$RawBinaryFile,
        [string]$ToolName,
        [string]$LegacyBaseName
    )
    $result = [pscustomobject]@{
        binary_path = ""
        config_signature = ""
    }
    $raw = ([string]$RawBinaryFile).Trim()
    if ([string]::IsNullOrWhiteSpace($raw)) {
        $defaultPolicyCli = Resolve-DefaultRolloutPolicyCliBinaryPath -RepoRootPath $RepoRootPath
        if (-not [string]::IsNullOrWhiteSpace($defaultPolicyCli)) {
            $result.binary_path = [string]$defaultPolicyCli
            $result.config_signature = ("binary={0}|tool={1}" -f [string]$defaultPolicyCli, $ToolName)
            return $result
        }
        $result.config_signature = ("missing_default|tool={0}|legacy={1}" -f $ToolName, $LegacyBaseName)
        return $result
    }

    $candidate = $raw
    if (-not [System.IO.Path]::IsPathRooted($candidate)) {
        $candidate = Join-Path $RepoRootPath $candidate
    }
    $result.config_signature = ("binary_file={0}" -f $raw)
    if (Test-Path -LiteralPath $candidate) {
        try {
            $result.binary_path = (Resolve-Path -LiteralPath $candidate -ErrorAction Stop).Path
        } catch {
            $result.binary_path = $candidate
        }
    }
    return $result
}
function Apply-RolloutPolicyCliRuntimeConfig {
    param(
        [string]$RepoRoot
    )
    $script:RolloutPolicyCliBinaryPath = ""
    $script:RolloutPolicyCliConfigSignature = ""
    $raw = ([string]$script:RolloutPolicyCliBinaryFileRaw).Trim()
    if ([string]::IsNullOrWhiteSpace($raw)) {
        $resolved = Resolve-DefaultRolloutPolicyCliBinaryPath -RepoRootPath $RepoRoot
        if (-not [string]::IsNullOrWhiteSpace($resolved)) {
            $script:RolloutPolicyCliBinaryPath = $resolved
            $script:RolloutPolicyCliConfigSignature = ("auto_discovered={0}" -f $resolved)
        }
        return
    }
    $candidate = $raw
    if (-not [System.IO.Path]::IsPathRooted($candidate)) {
        $candidate = Join-Path $RepoRoot $candidate
    }
    $script:RolloutPolicyCliConfigSignature = ("binary_file={0}" -f $raw)
    if (Test-Path -LiteralPath $candidate) {
        try {
            $script:RolloutPolicyCliBinaryPath = (Resolve-Path -LiteralPath $candidate -ErrorAction Stop).Path
        } catch {
            $script:RolloutPolicyCliBinaryPath = $candidate
        }
    }
}

function Invoke-RolloutPolicyTool {
    param(
        [string]$BinaryPath,
        [string]$ToolName,
        [string[]]$Args,
        [switch]$CaptureOutput
    )
    $invokeArgs = @()
    if (-not [string]::IsNullOrWhiteSpace([string]$script:RolloutPolicyCliBinaryPath) -and ([string]$BinaryPath -eq [string]$script:RolloutPolicyCliBinaryPath)) {
        $invokeArgs += [string]$ToolName
    }
    if ($null -ne $Args) {
        $invokeArgs += @($Args)
    }
    if ($CaptureOutput) {
        return (& $BinaryPath @invokeArgs 2>&1)
    }
    & $BinaryPath @invokeArgs | Out-Null
    return $null
}

function Apply-RolloutPolicyCliOverrides {
    if ([string]::IsNullOrWhiteSpace([string]$script:RolloutPolicyCliBinaryPath)) {
        return
    }
    $pairs = @(
        @{ raw = [string]$script:DecisionDashboardExportBinaryFileRaw; pathVar = "DecisionDashboardExportBinaryPath"; sigVar = "DecisionDashboardExportConfigSignature"; tool = "rollout-decision-dashboard-export" },
        @{ raw = [string]$script:DecisionDashboardConsumerBinaryFileRaw; pathVar = "DecisionDashboardConsumerBinaryPath"; sigVar = "DecisionDashboardConsumerConfigSignature"; tool = "rollout-decision-dashboard-consumer" },
        @{ raw = [string]$script:DecisionRouteBinaryFileRaw; pathVar = "DecisionRouteBinaryPath"; sigVar = "DecisionRouteConfigSignature"; tool = "rollout-decision-route" },
        @{ raw = [string]$script:DecisionDeliveryBinaryFileRaw; pathVar = "DecisionDeliveryBinaryPath"; sigVar = "DecisionDeliveryConfigSignature"; tool = "rollout-decision-delivery" },
        @{ raw = [string]$script:RiskActionEvalBinaryFileRaw; pathVar = "RiskActionEvalBinaryPath"; sigVar = "RiskActionEvalConfigSignature"; tool = "risk-action-eval" },
        @{ raw = [string]$script:RiskActionMatrixBuildBinaryFileRaw; pathVar = "RiskActionMatrixBuildBinaryPath"; sigVar = "RiskActionMatrixBuildConfigSignature"; tool = "risk-action-matrix-build" },
        @{ raw = [string]$script:RiskMatrixSelectBinaryFileRaw; pathVar = "RiskMatrixSelectBinaryPath"; sigVar = "RiskMatrixSelectConfigSignature"; tool = "risk-matrix-select" },
        @{ raw = [string]$script:RiskBlockedSelectBinaryFileRaw; pathVar = "RiskBlockedSelectBinaryPath"; sigVar = "RiskBlockedSelectConfigSignature"; tool = "risk-blocked-select" },
        @{ raw = [string]$script:RiskBlockedMapBuildBinaryFileRaw; pathVar = "RiskBlockedMapBuildBinaryPath"; sigVar = "RiskBlockedMapBuildConfigSignature"; tool = "risk-blocked-map-build" },
        @{ raw = [string]$script:RiskLevelSetBinaryFileRaw; pathVar = "RiskLevelSetBinaryPath"; sigVar = "RiskLevelSetConfigSignature"; tool = "risk-level-set" },
        @{ raw = [string]$script:RiskPolicyProfileSelectBinaryFileRaw; pathVar = "RiskPolicyProfileSelectBinaryPath"; sigVar = "RiskPolicyProfileSelectConfigSignature"; tool = "risk-policy-profile-select" },
        @{ raw = [string]$script:FailoverPolicyMatrixBuildBinaryFileRaw; pathVar = "FailoverPolicyMatrixBuildBinaryPath"; sigVar = "FailoverPolicyMatrixBuildConfigSignature"; tool = "failover-policy-matrix-build" }
    )
    foreach ($it in @($pairs)) {
        if (-not [string]::IsNullOrWhiteSpace([string]$it.raw)) {
            continue
        }
        Set-Variable -Scope Script -Name ([string]$it.pathVar) -Value ([string]$script:RolloutPolicyCliBinaryPath)
        Set-Variable -Scope Script -Name ([string]$it.sigVar) -Value ("binary={0}|tool={1}" -f [string]$script:RolloutPolicyCliBinaryPath, [string]$it.tool)
    }
}

function Apply-RiskLevelSetRuntimeConfig {
    param(
        [string]$RepoRoot
    )
    $script:RiskLevelSetBinaryPath = ""
    $script:RiskLevelSetConfigSignature = ""
    $binaryCfg = Resolve-PolicyToolBinaryConfig -RepoRootPath $RepoRoot -RawBinaryFile ([string]$script:RiskLevelSetBinaryFileRaw) -ToolName "risk-level-set" -LegacyBaseName "novovm-risk-level-set"
    $script:RiskLevelSetBinaryPath = [string]$binaryCfg.binary_path
    $script:RiskLevelSetConfigSignature = [string]$binaryCfg.config_signature
}

function Parse-RiskLevelSet {
    param([object]$Raw)
    $set = @{}
    if ($null -eq $Raw) {
        return $set
    }
    $hasRiskLevelSetBinary = (-not [string]::IsNullOrWhiteSpace([string]$script:RiskLevelSetBinaryPath)) -and (Test-Path -LiteralPath $script:RiskLevelSetBinaryPath)
    if ($hasRiskLevelSetBinary) {
        try {
            $rawJson = "null"
            if ($null -ne $Raw) {
                try {
                    $rawJson = ConvertTo-Json -InputObject $Raw -Depth 16 -Compress
                } catch {
                    $rawJson = "null"
                }
                if ([string]::IsNullOrWhiteSpace([string]$rawJson)) {
                    $rawJson = "null"
                }
            }
            $outJson = Invoke-RolloutPolicyTool -BinaryPath $script:RiskLevelSetBinaryPath -ToolName "risk-level-set" -Args @("--raw-set-json", $rawJson) -CaptureOutput
            if ($LASTEXITCODE -eq 0 -and -not [string]::IsNullOrWhiteSpace([string]$outJson)) {
                $parsed = $outJson | ConvertFrom-Json -Depth 16
                if ($null -ne $parsed -and $null -ne $parsed.levels) {
                    foreach ($it in @($parsed.levels)) {
                        if ($null -eq $it) { continue }
                        $v = ([string]$it).Trim().ToLowerInvariant()
                        if ($v -eq "green" -or $v -eq "yellow" -or $v -eq "orange" -or $v -eq "red") {
                            $set[$v] = $true
                        }
                    }
                    if ($set.Count -gt 0) {
                        return $set
                    }
                }
            }
        } catch {
        }
    }
    if ($Raw -is [System.Array]) {
        foreach ($it in @($Raw)) {
            if ($null -eq $it) { continue }
            $v = ([string]$it).Trim().ToLowerInvariant()
            if ($v -eq "green" -or $v -eq "yellow" -or $v -eq "orange" -or $v -eq "red") {
                $set[$v] = $true
            }
        }
        return $set
    }
    $s = [string]$Raw
    foreach ($it in @($s.Split(",", [System.StringSplitOptions]::RemoveEmptyEntries))) {
        $v = ([string]$it).Trim().ToLowerInvariant()
        if ($v -eq "green" -or $v -eq "yellow" -or $v -eq "orange" -or $v -eq "red") {
            $set[$v] = $true
        }
    }
    return $set
}

function Resolve-RiskActionMatrix {
    param(
        [object[]]$RawRules,
        [int]$YellowCap,
        [int]$YellowPause,
        [int]$OrangeCap,
        [int]$OrangePause,
        [bool]$RedBlock
    )
    $hasRiskActionMatrixBuildBinary = (-not [string]::IsNullOrWhiteSpace([string]$script:RiskActionMatrixBuildBinaryPath)) -and (Test-Path -LiteralPath $script:RiskActionMatrixBuildBinaryPath)
    if ($hasRiskActionMatrixBuildBinary) {
        try {
            $rawRulesJson = (@($RawRules) | ConvertTo-Json -Depth 24 -Compress)
            if ([string]::IsNullOrWhiteSpace([string]$rawRulesJson)) {
                $rawRulesJson = "[]"
            }
            $rustArgs = @(
                "--raw-rules-json", $rawRulesJson,
                "--yellow-cap", [string][int]$YellowCap,
                "--yellow-pause", [string][int]$YellowPause,
                "--orange-cap", [string][int]$OrangeCap,
                "--orange-pause", [string][int]$OrangePause,
                "--red-block", (([string][bool]$RedBlock).ToLowerInvariant())
            )
            $rustOutput = Invoke-RolloutPolicyTool -BinaryPath $script:RiskActionMatrixBuildBinaryPath -ToolName "risk-action-matrix-build" -Args $rustArgs -CaptureOutput
            if ($LASTEXITCODE -eq 0) {
                $rustText = (($rustOutput | Out-String).Trim())
                if (-not [string]::IsNullOrWhiteSpace($rustText)) {
                    $rustLine = ($rustText -split "`r?`n" | Select-Object -Last 1)
                    $rustObj = $rustLine | ConvertFrom-Json
                    if ($null -ne $rustObj -and $null -ne $rustObj.matrix) {
                        return @($rustObj.matrix)
                    }
                }
            }
        } catch {
        }
    }
    # emergency fallback only:
    # keep a conservative global baseline and stop reconstructing the full
    # source/min_site_priority override matrix inside PowerShell.
    return @(
        [pscustomobject]@{ source = "*"; level = "green"; cap_concurrent = 0; pause_seconds = 0; block_dispatch = $false; min_site_priority = [int]::MinValue },
        [pscustomobject]@{ source = "*"; level = "yellow"; cap_concurrent = [Math]::Max(1, [int]$YellowCap); pause_seconds = [Math]::Max(0, [int]$YellowPause); block_dispatch = $false; min_site_priority = [int]::MinValue },
        [pscustomobject]@{ source = "*"; level = "orange"; cap_concurrent = [Math]::Max(1, [int]$OrangeCap); pause_seconds = [Math]::Max(0, [int]$OrangePause); block_dispatch = $false; min_site_priority = [int]::MinValue },
        [pscustomobject]@{ source = "*"; level = "red"; cap_concurrent = [Math]::Max(1, [int]$OrangeCap); pause_seconds = [Math]::Max(0, [int]$OrangePause); block_dispatch = [bool]$RedBlock; min_site_priority = [int]::MinValue }
    )
}

function Get-ConfigPropertyValue {
    param(
        [object]$Object,
        [string]$Name
    )
    if ($null -eq $Object) {
        return $null
    }
    $prop = $Object.PSObject.Properties[$Name]
    if ($null -eq $prop) {
        return $null
    }
    return $prop.Value
}

function Resolve-RiskPolicyProfileSelection {
    param(
        [object]$RiskPolicy
    )
    $requested = ""
    $active = "base"
    $profile = $null
    $resolved = $true
    $requestedRaw = Get-ConfigPropertyValue -Object $RiskPolicy -Name "active_profile"
    if ($null -ne $requestedRaw) {
        $requested = ([string]$requestedRaw).Trim()
    }
    if (-not [string]::IsNullOrWhiteSpace($requested)) {
        $profiles = Get-ConfigPropertyValue -Object $RiskPolicy -Name "policy_profiles"
        if ($null -ne $profiles) {
            $profileProp = $profiles.PSObject.Properties[$requested]
            if ($null -ne $profileProp) {
                $profile = $profileProp.Value
                $active = $requested
            } else {
                $resolved = $false
            }
        } else {
            $resolved = $false
        }
    }
    if ([string]::IsNullOrWhiteSpace($requested)) {
        $requested = "base"
    }
    $fallback = [pscustomobject]@{
        requested = $requested
        active = $active
        resolved = [bool]$resolved
        profile = $profile
    }
    $script:RiskPolicyProfileSelectBinaryPath = ""
    $script:RiskPolicyProfileSelectConfigSignature = ""
    $cfg = Get-ConfigPropertyValue -Object $RiskPolicy -Name "profile_select"
    $raw = ""
    if ($null -ne $cfg -and $null -ne $cfg.binary_file) {
        $raw = ([string]$cfg.binary_file).Trim()
    }
    $binaryCfg = Resolve-PolicyToolBinaryConfig -RepoRootPath $RepoRoot -RawBinaryFile $raw -ToolName "risk-policy-profile-select" -LegacyBaseName "novovm-risk-policy-profile-select"
    $script:RiskPolicyProfileSelectBinaryPath = [string]$binaryCfg.binary_path
    $script:RiskPolicyProfileSelectConfigSignature = [string]$binaryCfg.config_signature
    if ([string]::IsNullOrWhiteSpace([string]$script:RiskPolicyProfileSelectBinaryPath) -or -not (Test-Path -LiteralPath $script:RiskPolicyProfileSelectBinaryPath)) {
        return $fallback
    }
    try {
        $riskPolicyJson = ConvertTo-Json -InputObject $RiskPolicy -Depth 40 -Compress
        if ([string]::IsNullOrWhiteSpace([string]$riskPolicyJson)) {
            return $fallback
        }
        $outJson = Invoke-RolloutPolicyTool -BinaryPath $script:RiskPolicyProfileSelectBinaryPath -ToolName "risk-policy-profile-select" -Args @("--risk-policy-json", $riskPolicyJson, "--requested-profile", $requested) -CaptureOutput
        if ($LASTEXITCODE -eq 0 -and -not [string]::IsNullOrWhiteSpace([string]$outJson)) {
            $parsed = $outJson | ConvertFrom-Json -Depth 32
            if ($null -ne $parsed -and -not [string]::IsNullOrWhiteSpace([string]$parsed.active)) {
                $selProfile = $null
                if ($null -ne $parsed.profile) {
                    $selProfile = $parsed.profile
                }
                return [pscustomobject]@{
                    requested = [string]$parsed.requested
                    active = [string]$parsed.active
                    resolved = [bool]$parsed.resolved
                    profile = $selProfile
                }
            }
        }
    } catch {
    }
    return $fallback
}

function Get-RiskPolicyFieldValue {
    param(
        [object]$RiskPolicy,
        [object]$Profile,
        [string]$FieldName
    )
    $profileValue = Get-ConfigPropertyValue -Object $Profile -Name $FieldName
    if ($null -ne $profileValue) {
        return $profileValue
    }
    return (Get-ConfigPropertyValue -Object $RiskPolicy -Name $FieldName)
}

function Apply-RiskPolicyConfigFromQueue {
    param(
        [object]$RiskPolicy
    )
    $script:UnifiedRiskBlockedRaw = @("red")
    $script:RoleRiskWinnerBlockedRaw = $null
    $script:RoleRiskFailoverBlockedRaw = $null
    $script:RiskActionMatrixRaw = @()
    $script:RiskSiteRegionMapRaw = $null
    $script:RiskActionMatrixSiteOverrideRaw = $null
    $script:RiskActionMatrixRegionOverrideRaw = $null
    $script:RiskWinnerBlockedBySiteRaw = $null
    $script:RiskWinnerBlockedByRegionRaw = $null
    $script:RiskFailoverBlockedBySiteRaw = $null
    $script:RiskFailoverBlockedByRegionRaw = $null
    $script:DecisionAlertTargetMap = @{
        "l1-pager" = "oncall:l1:finality"
        "l2-oncall" = "oncall:l2:execution"
        "l3-oncall" = "oncall:l3:edge"
        "ops-oncall" = "oncall:ops:default"
        "l1-observe" = "observe:l1:finality"
        "l2-observe" = "observe:l2:execution"
        "l3-observe" = "observe:l3:edge"
        "ops-observe" = "observe:ops:default"
    }
    $script:DecisionAlertDeliveryTypeMap = @{
        "oncall:l1:finality" = "webhook"
        "oncall:l2:execution" = "webhook"
        "oncall:l3:edge" = "webhook"
        "oncall:ops:default" = "webhook"
        "observe:l1:finality" = "im"
        "observe:l2:execution" = "im"
        "observe:l3:edge" = "im"
        "observe:ops:default" = "im"
    }
    $script:DecisionDeliveryEndpointMap = @{}
    $script:DecisionDeliveryEmailTargetMap = @{}
    $script:DecisionDeliveryEmailSmtpServer = ""
    $script:DecisionDeliveryEmailSmtpPort = 25
    $script:DecisionDeliveryEmailFrom = ""
    $script:DecisionDeliveryEmailUseSsl = $false
    $script:DecisionDeliveryEmailSmtpUser = ""
    $script:DecisionDeliveryEmailSmtpPasswordEnv = ""
    $script:RiskPolicyRequestedProfile = "base"
    $script:RiskPolicyActiveProfile = "base"
    $script:RiskPolicyProfileResolved = $true
    $script:RiskPolicyHotReloadEnabled = $true
    $script:RiskPolicyHotReloadCheckSeconds = 2
    $script:DecisionDashboardExportEnabled = $false
    $script:DecisionDashboardExportCheckSeconds = 30
    $script:DecisionDashboardExportMode = "both"
    $script:DecisionDashboardExportTail = 2000
    $script:DecisionDashboardExportSinceUtc = ""
    $script:DecisionDashboardExportAuditFileRaw = ""
    $script:DecisionDashboardExportOutputFileRaw = ""
    $script:DecisionDashboardExportBinaryFileRaw = ""
    $script:DecisionDashboardExportScriptFileRaw = ""
    $script:DecisionDashboardConsumerEnabled = $false
    $script:DecisionDashboardConsumerCheckSeconds = 30
    $script:DecisionDashboardConsumerMode = "all"
    $script:DecisionDashboardConsumerTail = 2000
    $script:DecisionDashboardConsumerInputFileRaw = ""
    $script:DecisionDashboardConsumerOutputFileRaw = ""
    $script:DecisionDashboardConsumerAlertsFileRaw = ""
    $script:DecisionDashboardConsumerBinaryFileRaw = ""
    $script:DecisionDashboardConsumerScriptFileRaw = ""
    $script:DecisionRouteBinaryFileRaw = ""
    $script:DecisionRouteBinaryPath = ""
    $script:DecisionRouteConfigSignature = ""
    $script:RiskBlockedMapBuildBinaryFileRaw = ""
    $script:RiskBlockedMapBuildBinaryPath = ""
    $script:RiskBlockedMapBuildConfigSignature = ""
    $script:RiskBlockedSelectBinaryFileRaw = ""
    $script:RiskBlockedSelectBinaryPath = ""
    $script:RiskBlockedSelectConfigSignature = ""
    $script:RiskMatrixSelectBinaryFileRaw = ""
    $script:RiskMatrixSelectBinaryPath = ""
    $script:RiskMatrixSelectConfigSignature = ""
    $script:RiskActionEvalBinaryFileRaw = ""
    $script:RiskActionEvalBinaryPath = ""
    $script:RiskActionEvalConfigSignature = ""
    $script:RiskActionMatrixBuildBinaryFileRaw = ""
    $script:RiskActionMatrixBuildBinaryPath = ""
    $script:RiskActionMatrixBuildConfigSignature = ""
    $script:FailoverPolicyMatrixBuildBinaryFileRaw = ""
    $script:FailoverPolicyMatrixBuildBinaryPath = ""
    $script:FailoverPolicyMatrixBuildConfigSignature = ""
    $script:DecisionDeliveryBinaryFileRaw = ""
    $script:DecisionDeliveryBinaryPath = ""
    $script:DecisionDeliveryConfigSignature = ""
    $script:RolloutPolicyCliBinaryFileRaw = ""
    $script:RolloutPolicyCliBinaryPath = ""
    $script:RolloutPolicyCliConfigSignature = ""

    if ($null -eq $RiskPolicy) {
        return
    }

    $selection = Resolve-RiskPolicyProfileSelection -RiskPolicy $RiskPolicy
    $profile = $selection.profile
    $script:RiskPolicyRequestedProfile = [string]$selection.requested
    $script:RiskPolicyActiveProfile = [string]$selection.active
    $script:RiskPolicyProfileResolved = [bool]$selection.resolved

    $hotReloadCfg = Get-ConfigPropertyValue -Object $RiskPolicy -Name "hot_reload"
    if ($null -ne $hotReloadCfg) {
        if ($null -ne $hotReloadCfg.enabled) { $script:RiskPolicyHotReloadEnabled = [bool]$hotReloadCfg.enabled }
        if ($null -ne $hotReloadCfg.check_seconds) { $script:RiskPolicyHotReloadCheckSeconds = [Math]::Max(1, [int]$hotReloadCfg.check_seconds) }
    }
    $policyCliCfgBase = Get-ConfigPropertyValue -Object $RiskPolicy -Name "policy_cli"
    $policyCliCfgProfile = Get-ConfigPropertyValue -Object $profile -Name "policy_cli"
    foreach ($policyCliCfg in @($policyCliCfgBase, $policyCliCfgProfile)) {
        if ($null -eq $policyCliCfg) { continue }
        if ($null -ne $policyCliCfg.binary_file) { $script:RolloutPolicyCliBinaryFileRaw = ([string]$policyCliCfg.binary_file).Trim() }
    }
    $script:RiskLevelSetBinaryFileRaw = ""
    $riskLevelSetCfgBase = Get-ConfigPropertyValue -Object $RiskPolicy -Name "risk_level_set"
    $riskLevelSetCfgProfile = Get-ConfigPropertyValue -Object $profile -Name "risk_level_set"
    foreach ($riskLevelSetCfg in @($riskLevelSetCfgBase, $riskLevelSetCfgProfile)) {
        if ($null -eq $riskLevelSetCfg) { continue }
        if ($null -ne $riskLevelSetCfg.binary_file) { $script:RiskLevelSetBinaryFileRaw = ([string]$riskLevelSetCfg.binary_file).Trim() }
    }
    $dashboardExportCfgBase = Get-ConfigPropertyValue -Object $RiskPolicy -Name "decision_dashboard_export"
    $dashboardExportCfgProfile = Get-ConfigPropertyValue -Object $profile -Name "decision_dashboard_export"
    foreach ($dashboardExportCfg in @($dashboardExportCfgBase, $dashboardExportCfgProfile)) {
        if ($null -eq $dashboardExportCfg) { continue }
        if ($null -ne $dashboardExportCfg.enabled) { $script:DecisionDashboardExportEnabled = [bool]$dashboardExportCfg.enabled }
        if ($null -ne $dashboardExportCfg.check_seconds) { $script:DecisionDashboardExportCheckSeconds = [Math]::Max(2, [int]$dashboardExportCfg.check_seconds) }
        if ($null -ne $dashboardExportCfg.mode) {
            $modeRaw = ([string]$dashboardExportCfg.mode).Trim().ToLowerInvariant()
            if ($modeRaw -eq "delivery" -or $modeRaw -eq "summary" -or $modeRaw -eq "both") {
                $script:DecisionDashboardExportMode = $modeRaw
            }
        }
        if ($null -ne $dashboardExportCfg.tail) { $script:DecisionDashboardExportTail = [Math]::Max(0, [int]$dashboardExportCfg.tail) }
        if ($null -ne $dashboardExportCfg.since_utc) { $script:DecisionDashboardExportSinceUtc = ([string]$dashboardExportCfg.since_utc).Trim() }
        if ($null -ne $dashboardExportCfg.audit_file) { $script:DecisionDashboardExportAuditFileRaw = ([string]$dashboardExportCfg.audit_file).Trim() }
        if ($null -ne $dashboardExportCfg.output_file) { $script:DecisionDashboardExportOutputFileRaw = ([string]$dashboardExportCfg.output_file).Trim() }
        if ($null -ne $dashboardExportCfg.binary_file) { $script:DecisionDashboardExportBinaryFileRaw = ([string]$dashboardExportCfg.binary_file).Trim() }
        if ($null -ne $dashboardExportCfg.script_file) { $script:DecisionDashboardExportScriptFileRaw = ([string]$dashboardExportCfg.script_file).Trim() }
    }
    $dashboardConsumerCfgBase = Get-ConfigPropertyValue -Object $RiskPolicy -Name "decision_dashboard_consumer"
    $dashboardConsumerCfgProfile = Get-ConfigPropertyValue -Object $profile -Name "decision_dashboard_consumer"
    foreach ($dashboardConsumerCfg in @($dashboardConsumerCfgBase, $dashboardConsumerCfgProfile)) {
        if ($null -eq $dashboardConsumerCfg) { continue }
        if ($null -ne $dashboardConsumerCfg.enabled) { $script:DecisionDashboardConsumerEnabled = [bool]$dashboardConsumerCfg.enabled }
        if ($null -ne $dashboardConsumerCfg.check_seconds) { $script:DecisionDashboardConsumerCheckSeconds = [Math]::Max(2, [int]$dashboardConsumerCfg.check_seconds) }
        if ($null -ne $dashboardConsumerCfg.mode) {
            $consumerModeRaw = ([string]$dashboardConsumerCfg.mode).Trim().ToLowerInvariant()
            if ($consumerModeRaw -eq "all" -or $consumerModeRaw -eq "blocked") {
                $script:DecisionDashboardConsumerMode = $consumerModeRaw
            }
        }
        if ($null -ne $dashboardConsumerCfg.tail) { $script:DecisionDashboardConsumerTail = [Math]::Max(0, [int]$dashboardConsumerCfg.tail) }
        if ($null -ne $dashboardConsumerCfg.input_file) { $script:DecisionDashboardConsumerInputFileRaw = ([string]$dashboardConsumerCfg.input_file).Trim() }
        if ($null -ne $dashboardConsumerCfg.output_file) { $script:DecisionDashboardConsumerOutputFileRaw = ([string]$dashboardConsumerCfg.output_file).Trim() }
        if ($null -ne $dashboardConsumerCfg.alerts_file) { $script:DecisionDashboardConsumerAlertsFileRaw = ([string]$dashboardConsumerCfg.alerts_file).Trim() }
        if ($null -ne $dashboardConsumerCfg.binary_file) { $script:DecisionDashboardConsumerBinaryFileRaw = ([string]$dashboardConsumerCfg.binary_file).Trim() }
        if ($null -ne $dashboardConsumerCfg.script_file) { $script:DecisionDashboardConsumerScriptFileRaw = ([string]$dashboardConsumerCfg.script_file).Trim() }
    }
    $decisionDeliveryCfgBase = Get-ConfigPropertyValue -Object $RiskPolicy -Name "decision_delivery"
    $decisionDeliveryCfgProfile = Get-ConfigPropertyValue -Object $profile -Name "decision_delivery"
    foreach ($decisionDeliveryCfg in @($decisionDeliveryCfgBase, $decisionDeliveryCfgProfile)) {
        if ($null -eq $decisionDeliveryCfg) { continue }
        if ($null -ne $decisionDeliveryCfg.binary_file) { $script:DecisionDeliveryBinaryFileRaw = ([string]$decisionDeliveryCfg.binary_file).Trim() }
    }
    $decisionRouteCfgBase = Get-ConfigPropertyValue -Object $RiskPolicy -Name "decision_route"
    $decisionRouteCfgProfile = Get-ConfigPropertyValue -Object $profile -Name "decision_route"
    foreach ($decisionRouteCfg in @($decisionRouteCfgBase, $decisionRouteCfgProfile)) {
        if ($null -eq $decisionRouteCfg) { continue }
        if ($null -ne $decisionRouteCfg.binary_file) { $script:DecisionRouteBinaryFileRaw = ([string]$decisionRouteCfg.binary_file).Trim() }
    }
    $riskBlockedSelectCfgBase = Get-ConfigPropertyValue -Object $RiskPolicy -Name "risk_blocked_select"
    $riskBlockedSelectCfgProfile = Get-ConfigPropertyValue -Object $profile -Name "risk_blocked_select"
    foreach ($riskBlockedSelectCfg in @($riskBlockedSelectCfgBase, $riskBlockedSelectCfgProfile)) {
        if ($null -eq $riskBlockedSelectCfg) { continue }
        if ($null -ne $riskBlockedSelectCfg.binary_file) { $script:RiskBlockedSelectBinaryFileRaw = ([string]$riskBlockedSelectCfg.binary_file).Trim() }
    }
    $riskBlockedMapBuildCfgBase = Get-ConfigPropertyValue -Object $RiskPolicy -Name "risk_blocked_map_build"
    $riskBlockedMapBuildCfgProfile = Get-ConfigPropertyValue -Object $profile -Name "risk_blocked_map_build"
    foreach ($riskBlockedMapBuildCfg in @($riskBlockedMapBuildCfgBase, $riskBlockedMapBuildCfgProfile)) {
        if ($null -eq $riskBlockedMapBuildCfg) { continue }
        if ($null -ne $riskBlockedMapBuildCfg.binary_file) { $script:RiskBlockedMapBuildBinaryFileRaw = ([string]$riskBlockedMapBuildCfg.binary_file).Trim() }
    }
    $riskActionEvalCfgBase = Get-ConfigPropertyValue -Object $RiskPolicy -Name "risk_action_eval"
    $riskActionEvalCfgProfile = Get-ConfigPropertyValue -Object $profile -Name "risk_action_eval"
    foreach ($riskActionEvalCfg in @($riskActionEvalCfgBase, $riskActionEvalCfgProfile)) {
        if ($null -eq $riskActionEvalCfg) { continue }
        if ($null -ne $riskActionEvalCfg.binary_file) { $script:RiskActionEvalBinaryFileRaw = ([string]$riskActionEvalCfg.binary_file).Trim() }
    }
    $riskActionMatrixBuildCfgBase = Get-ConfigPropertyValue -Object $RiskPolicy -Name "risk_action_matrix_build"
    $riskActionMatrixBuildCfgProfile = Get-ConfigPropertyValue -Object $profile -Name "risk_action_matrix_build"
    foreach ($riskActionMatrixBuildCfg in @($riskActionMatrixBuildCfgBase, $riskActionMatrixBuildCfgProfile)) {
        if ($null -eq $riskActionMatrixBuildCfg) { continue }
        if ($null -ne $riskActionMatrixBuildCfg.binary_file) { $script:RiskActionMatrixBuildBinaryFileRaw = ([string]$riskActionMatrixBuildCfg.binary_file).Trim() }
    }
    $failoverPolicyMatrixBuildCfgBase = Get-ConfigPropertyValue -Object $RiskPolicy -Name "failover_policy_matrix_build"
    $failoverPolicyMatrixBuildCfgProfile = Get-ConfigPropertyValue -Object $profile -Name "failover_policy_matrix_build"
    foreach ($failoverPolicyMatrixBuildCfg in @($failoverPolicyMatrixBuildCfgBase, $failoverPolicyMatrixBuildCfgProfile)) {
        if ($null -eq $failoverPolicyMatrixBuildCfg) { continue }
        if ($null -ne $failoverPolicyMatrixBuildCfg.binary_file) { $script:FailoverPolicyMatrixBuildBinaryFileRaw = ([string]$failoverPolicyMatrixBuildCfg.binary_file).Trim() }
    }
    $riskMatrixSelectCfgBase = Get-ConfigPropertyValue -Object $RiskPolicy -Name "risk_matrix_select"
    $riskMatrixSelectCfgProfile = Get-ConfigPropertyValue -Object $profile -Name "risk_matrix_select"
    foreach ($riskMatrixSelectCfg in @($riskMatrixSelectCfgBase, $riskMatrixSelectCfgProfile)) {
        if ($null -eq $riskMatrixSelectCfg) { continue }
        if ($null -ne $riskMatrixSelectCfg.binary_file) { $script:RiskMatrixSelectBinaryFileRaw = ([string]$riskMatrixSelectCfg.binary_file).Trim() }
    }

    $field = Get-RiskPolicyFieldValue -RiskPolicy $RiskPolicy -Profile $profile -FieldName "blocked_levels"
    if ($null -ne $field) { $script:UnifiedRiskBlockedRaw = $field }
    $field = Get-RiskPolicyFieldValue -RiskPolicy $RiskPolicy -Profile $profile -FieldName "winner_guard_blocked_levels"
    if ($null -ne $field) { $script:RoleRiskWinnerBlockedRaw = $field }
    $field = Get-RiskPolicyFieldValue -RiskPolicy $RiskPolicy -Profile $profile -FieldName "failover_risk_link_blocked_levels"
    if ($null -ne $field) { $script:RoleRiskFailoverBlockedRaw = $field }
    $field = Get-RiskPolicyFieldValue -RiskPolicy $RiskPolicy -Profile $profile -FieldName "action_matrix"
    if ($null -ne $field) { $script:RiskActionMatrixRaw = @($field) }
    $field = Get-RiskPolicyFieldValue -RiskPolicy $RiskPolicy -Profile $profile -FieldName "site_region_map"
    if ($null -ne $field) { $script:RiskSiteRegionMapRaw = $field }
    $field = Get-RiskPolicyFieldValue -RiskPolicy $RiskPolicy -Profile $profile -FieldName "site_action_matrix_overrides"
    if ($null -ne $field) { $script:RiskActionMatrixSiteOverrideRaw = $field }
    $field = Get-RiskPolicyFieldValue -RiskPolicy $RiskPolicy -Profile $profile -FieldName "region_action_matrix_overrides"
    if ($null -ne $field) { $script:RiskActionMatrixRegionOverrideRaw = $field }
    $field = Get-RiskPolicyFieldValue -RiskPolicy $RiskPolicy -Profile $profile -FieldName "site_winner_guard_blocked_levels"
    if ($null -ne $field) { $script:RiskWinnerBlockedBySiteRaw = $field }
    $field = Get-RiskPolicyFieldValue -RiskPolicy $RiskPolicy -Profile $profile -FieldName "region_winner_guard_blocked_levels"
    if ($null -ne $field) { $script:RiskWinnerBlockedByRegionRaw = $field }
    $field = Get-RiskPolicyFieldValue -RiskPolicy $RiskPolicy -Profile $profile -FieldName "site_failover_risk_link_blocked_levels"
    if ($null -ne $field) { $script:RiskFailoverBlockedBySiteRaw = $field }
    $field = Get-RiskPolicyFieldValue -RiskPolicy $RiskPolicy -Profile $profile -FieldName "region_failover_risk_link_blocked_levels"
    if ($null -ne $field) { $script:RiskFailoverBlockedByRegionRaw = $field }

    $alertMapBase = Get-ConfigPropertyValue -Object $RiskPolicy -Name "alert_channel_targets"
    if ($null -ne $alertMapBase) {
        foreach ($p in $alertMapBase.PSObject.Properties) {
            $k = ([string]$p.Name).Trim().ToLowerInvariant()
            $v = ([string]$p.Value).Trim()
            if (-not [string]::IsNullOrWhiteSpace($k) -and -not [string]::IsNullOrWhiteSpace($v)) {
                $script:DecisionAlertTargetMap[$k] = $v
            }
        }
    }
    $alertMapProfile = Get-ConfigPropertyValue -Object $profile -Name "alert_channel_targets"
    if ($null -ne $alertMapProfile) {
        foreach ($p in $alertMapProfile.PSObject.Properties) {
            $k = ([string]$p.Name).Trim().ToLowerInvariant()
            $v = ([string]$p.Value).Trim()
            if (-not [string]::IsNullOrWhiteSpace($k) -and -not [string]::IsNullOrWhiteSpace($v)) {
                $script:DecisionAlertTargetMap[$k] = $v
            }
        }
    }

    $deliveryTypeBase = Get-ConfigPropertyValue -Object $RiskPolicy -Name "alert_target_delivery_types"
    if ($null -ne $deliveryTypeBase) {
        foreach ($p in $deliveryTypeBase.PSObject.Properties) {
            $k = ([string]$p.Name).Trim().ToLowerInvariant()
            $v = ([string]$p.Value).Trim().ToLowerInvariant()
            if (-not [string]::IsNullOrWhiteSpace($k) -and -not [string]::IsNullOrWhiteSpace($v)) {
                $script:DecisionAlertDeliveryTypeMap[$k] = $v
            }
        }
    }
    $deliveryTypeProfile = Get-ConfigPropertyValue -Object $profile -Name "alert_target_delivery_types"
    if ($null -ne $deliveryTypeProfile) {
        foreach ($p in $deliveryTypeProfile.PSObject.Properties) {
            $k = ([string]$p.Name).Trim().ToLowerInvariant()
            $v = ([string]$p.Value).Trim().ToLowerInvariant()
            if (-not [string]::IsNullOrWhiteSpace($k) -and -not [string]::IsNullOrWhiteSpace($v)) {
                $script:DecisionAlertDeliveryTypeMap[$k] = $v
            }
        }
    }

    $webhookBase = Get-ConfigPropertyValue -Object $RiskPolicy -Name "delivery_webhook_endpoints"
    if ($null -ne $webhookBase) {
        foreach ($p in $webhookBase.PSObject.Properties) {
            $k = ([string]$p.Name).Trim().ToLowerInvariant()
            $v = ([string]$p.Value).Trim()
            if (-not [string]::IsNullOrWhiteSpace($k) -and -not [string]::IsNullOrWhiteSpace($v)) {
                $script:DecisionDeliveryEndpointMap[$k] = $v
            }
        }
    }
    $webhookProfile = Get-ConfigPropertyValue -Object $profile -Name "delivery_webhook_endpoints"
    if ($null -ne $webhookProfile) {
        foreach ($p in $webhookProfile.PSObject.Properties) {
            $k = ([string]$p.Name).Trim().ToLowerInvariant()
            $v = ([string]$p.Value).Trim()
            if (-not [string]::IsNullOrWhiteSpace($k) -and -not [string]::IsNullOrWhiteSpace($v)) {
                $script:DecisionDeliveryEndpointMap[$k] = $v
            }
        }
    }

    $imBase = Get-ConfigPropertyValue -Object $RiskPolicy -Name "delivery_im_endpoints"
    if ($null -ne $imBase) {
        foreach ($p in $imBase.PSObject.Properties) {
            $k = ([string]$p.Name).Trim().ToLowerInvariant()
            $v = ([string]$p.Value).Trim()
            if (-not [string]::IsNullOrWhiteSpace($k) -and -not [string]::IsNullOrWhiteSpace($v)) {
                $script:DecisionDeliveryEndpointMap[$k] = $v
            }
        }
    }
    $imProfile = Get-ConfigPropertyValue -Object $profile -Name "delivery_im_endpoints"
    if ($null -ne $imProfile) {
        foreach ($p in $imProfile.PSObject.Properties) {
            $k = ([string]$p.Name).Trim().ToLowerInvariant()
            $v = ([string]$p.Value).Trim()
            if (-not [string]::IsNullOrWhiteSpace($k) -and -not [string]::IsNullOrWhiteSpace($v)) {
                $script:DecisionDeliveryEndpointMap[$k] = $v
            }
        }
    }

    $emailTargetBase = Get-ConfigPropertyValue -Object $RiskPolicy -Name "delivery_email_targets"
    if ($null -ne $emailTargetBase) {
        foreach ($p in $emailTargetBase.PSObject.Properties) {
            $k = ([string]$p.Name).Trim().ToLowerInvariant()
            $v = ([string]$p.Value).Trim()
            if (-not [string]::IsNullOrWhiteSpace($k) -and -not [string]::IsNullOrWhiteSpace($v)) {
                $script:DecisionDeliveryEmailTargetMap[$k] = $v
            }
        }
    }
    $emailTargetProfile = Get-ConfigPropertyValue -Object $profile -Name "delivery_email_targets"
    if ($null -ne $emailTargetProfile) {
        foreach ($p in $emailTargetProfile.PSObject.Properties) {
            $k = ([string]$p.Name).Trim().ToLowerInvariant()
            $v = ([string]$p.Value).Trim()
            if (-not [string]::IsNullOrWhiteSpace($k) -and -not [string]::IsNullOrWhiteSpace($v)) {
                $script:DecisionDeliveryEmailTargetMap[$k] = $v
            }
        }
    }

    $emailCfgBase = Get-ConfigPropertyValue -Object $RiskPolicy -Name "delivery_email"
    if ($null -ne $emailCfgBase) {
        if ($null -ne $emailCfgBase.smtp_server) { $script:DecisionDeliveryEmailSmtpServer = ([string]$emailCfgBase.smtp_server).Trim() }
        if ($null -ne $emailCfgBase.smtp_port) { $script:DecisionDeliveryEmailSmtpPort = [Math]::Max(1, [int]$emailCfgBase.smtp_port) }
        if ($null -ne $emailCfgBase.from) { $script:DecisionDeliveryEmailFrom = ([string]$emailCfgBase.from).Trim() }
        if ($null -ne $emailCfgBase.use_ssl) { $script:DecisionDeliveryEmailUseSsl = [bool]$emailCfgBase.use_ssl }
        if ($null -ne $emailCfgBase.smtp_user) { $script:DecisionDeliveryEmailSmtpUser = ([string]$emailCfgBase.smtp_user).Trim() }
        if ($null -ne $emailCfgBase.smtp_password_env) { $script:DecisionDeliveryEmailSmtpPasswordEnv = ([string]$emailCfgBase.smtp_password_env).Trim() }
    }
    $emailCfgProfile = Get-ConfigPropertyValue -Object $profile -Name "delivery_email"
    if ($null -ne $emailCfgProfile) {
        if ($null -ne $emailCfgProfile.smtp_server) { $script:DecisionDeliveryEmailSmtpServer = ([string]$emailCfgProfile.smtp_server).Trim() }
        if ($null -ne $emailCfgProfile.smtp_port) { $script:DecisionDeliveryEmailSmtpPort = [Math]::Max(1, [int]$emailCfgProfile.smtp_port) }
        if ($null -ne $emailCfgProfile.from) { $script:DecisionDeliveryEmailFrom = ([string]$emailCfgProfile.from).Trim() }
        if ($null -ne $emailCfgProfile.use_ssl) { $script:DecisionDeliveryEmailUseSsl = [bool]$emailCfgProfile.use_ssl }
        if ($null -ne $emailCfgProfile.smtp_user) { $script:DecisionDeliveryEmailSmtpUser = ([string]$emailCfgProfile.smtp_user).Trim() }
        if ($null -ne $emailCfgProfile.smtp_password_env) { $script:DecisionDeliveryEmailSmtpPasswordEnv = ([string]$emailCfgProfile.smtp_password_env).Trim() }
    }
}

function Apply-DecisionDashboardExportRuntimeConfig {
    param(
        [string]$RepoRoot,
        [string]$AuditPathDefault
    )
    $scriptPath = Join-Path $RepoRoot "scripts/novovm-rollout-decision-dashboard-export.ps1"
    if (-not [string]::IsNullOrWhiteSpace([string]$script:DecisionDashboardExportScriptFileRaw)) {
        $scriptPath = Resolve-FullPath -Root $RepoRoot -Value ([string]$script:DecisionDashboardExportScriptFileRaw)
    }
    $binaryCfg = Resolve-PolicyToolBinaryConfig -RepoRootPath $RepoRoot -RawBinaryFile ([string]$script:DecisionDashboardExportBinaryFileRaw) -ToolName \"rollout-decision-dashboard-export\" -LegacyBaseName \"novovm-rollout-decision-dashboard-export\"
    $binaryPath = [string]$binaryCfg.binary_path

    $auditPath = $AuditPathDefault
    if (-not [string]::IsNullOrWhiteSpace([string]$script:DecisionDashboardExportAuditFileRaw)) {
        $auditPath = Resolve-FullPath -Root $RepoRoot -Value ([string]$script:DecisionDashboardExportAuditFileRaw)
    }

    $outputPath = Resolve-FullPath -Root $RepoRoot -Value "artifacts/runtime/rollout/control-plane-decision-dashboard.jsonl"
    if (-not [string]::IsNullOrWhiteSpace([string]$script:DecisionDashboardExportOutputFileRaw)) {
        $outputPath = Resolve-FullPath -Root $RepoRoot -Value ([string]$script:DecisionDashboardExportOutputFileRaw)
    }

    $modeNorm = ([string]$script:DecisionDashboardExportMode).Trim().ToLowerInvariant()
    if ($modeNorm -ne "delivery" -and $modeNorm -ne "summary" -and $modeNorm -ne "both") {
        $modeNorm = "both"
    }
    $script:DecisionDashboardExportMode = $modeNorm
    $script:DecisionDashboardExportCheckSeconds = [Math]::Max(2, [int]$script:DecisionDashboardExportCheckSeconds)
    $script:DecisionDashboardExportTail = [Math]::Max(0, [int]$script:DecisionDashboardExportTail)
    $script:DecisionDashboardExportScriptPath = $scriptPath
    $script:DecisionDashboardExportBinaryPath = $binaryPath
    $script:DecisionDashboardExportAuditPath = $auditPath
    $script:DecisionDashboardExportOutputPath = $outputPath
    $script:DecisionDashboardExportConfigSignature = ("enabled={0}|check={1}|mode={2}|tail={3}|since={4}|audit={5}|output={6}|binary={7}|script={8}" -f [bool]$script:DecisionDashboardExportEnabled, [int]$script:DecisionDashboardExportCheckSeconds, [string]$script:DecisionDashboardExportMode, [int]$script:DecisionDashboardExportTail, [string]$script:DecisionDashboardExportSinceUtc, [string]$script:DecisionDashboardExportAuditPath, [string]$script:DecisionDashboardExportOutputPath, [string]$script:DecisionDashboardExportBinaryPath, [string]$script:DecisionDashboardExportScriptPath)
}

function Invoke-DecisionDashboardExportIfDue {
    param(
        [string]$Source,
        [string]$AuditPath,
        [string]$QueuePath,
        [string]$PlanAction,
        [string]$ControlOpId,
        [string]$ControllerId
    )
    if (-not [bool]$script:DecisionDashboardExportEnabled) {
        return
    }
    $nowMs = Now-Ms
    if ($script:DecisionDashboardExportNextRunMs -gt 0 -and $nowMs -lt $script:DecisionDashboardExportNextRunMs) {
        return
    }
    $script:DecisionDashboardExportNextRunMs = $nowMs + ([int64]$script:DecisionDashboardExportCheckSeconds * 1000)

    $hasBinary = (-not [string]::IsNullOrWhiteSpace([string]$script:DecisionDashboardExportBinaryPath)) -and (Test-Path -LiteralPath $script:DecisionDashboardExportBinaryPath)
    $hasScript = (-not [string]::IsNullOrWhiteSpace([string]$script:DecisionDashboardExportScriptPath)) -and (Test-Path -LiteralPath $script:DecisionDashboardExportScriptPath)
    if (-not $hasBinary -and -not $hasScript) {
        Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                timestamp_utc = [DateTime]::UtcNow.ToString("o")
                control_operation_id = $ControlOpId
                controller_id = $ControllerId
                queue_file = $QueuePath
                action = $PlanAction
                result = "decision_dashboard_export_error"
                source = ([string]$Source).ToLowerInvariant()
                export_mode = [string]$script:DecisionDashboardExportMode
                export_tail = [int]$script:DecisionDashboardExportTail
                export_output_file = [string]$script:DecisionDashboardExportOutputPath
                error = "export runtime not found (binary+script missing)"
            })
        return
    }

    $rustArgs = @(
        "--audit-file", [string]$script:DecisionDashboardExportAuditPath,
        "--output-file", [string]$script:DecisionDashboardExportOutputPath,
        "--mode", [string]$script:DecisionDashboardExportMode
    )
    $psArgs = @(
        "-ExecutionPolicy", "Bypass",
        "-File", [string]$script:DecisionDashboardExportScriptPath,
        "-AuditFile", [string]$script:DecisionDashboardExportAuditPath,
        "-OutputFile", [string]$script:DecisionDashboardExportOutputPath,
        "-Mode", [string]$script:DecisionDashboardExportMode
    )
    if ([int]$script:DecisionDashboardExportTail -gt 0) {
        $rustArgs += @("--tail", [string][int]$script:DecisionDashboardExportTail)
        $psArgs += @("-Tail", [string][int]$script:DecisionDashboardExportTail)
    }
    if (-not [string]::IsNullOrWhiteSpace([string]$script:DecisionDashboardExportSinceUtc)) {
        $rustArgs += @("--since-utc", [string]$script:DecisionDashboardExportSinceUtc)
        $psArgs += @("-SinceUtc", [string]$script:DecisionDashboardExportSinceUtc)
    }

    try {
        if ($hasBinary) {
            Invoke-RolloutPolicyTool -BinaryPath $script:DecisionDashboardExportBinaryPath -ToolName "rollout-decision-dashboard-export" -Args $rustArgs | Out-Null
        } else {
            & powershell @psArgs | Out-Null
        }
        Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                timestamp_utc = [DateTime]::UtcNow.ToString("o")
                control_operation_id = $ControlOpId
                controller_id = $ControllerId
                queue_file = $QueuePath
                action = $PlanAction
                result = "decision_dashboard_export"
                source = ([string]$Source).ToLowerInvariant()
                export_mode = [string]$script:DecisionDashboardExportMode
                export_tail = [int]$script:DecisionDashboardExportTail
                export_output_file = [string]$script:DecisionDashboardExportOutputPath
                error = ""
            })
    } catch {
        Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                timestamp_utc = [DateTime]::UtcNow.ToString("o")
                control_operation_id = $ControlOpId
                controller_id = $ControllerId
                queue_file = $QueuePath
                action = $PlanAction
                result = "decision_dashboard_export_error"
                source = ([string]$Source).ToLowerInvariant()
                export_mode = [string]$script:DecisionDashboardExportMode
                export_tail = [int]$script:DecisionDashboardExportTail
                export_output_file = [string]$script:DecisionDashboardExportOutputPath
                error = $_.Exception.Message
            })
    }
}

function Apply-DecisionDashboardConsumerRuntimeConfig {
    param(
        [string]$RepoRoot
    )
    $scriptPath = Join-Path $RepoRoot "scripts/novovm-rollout-decision-dashboard-consumer.ps1"
    if (-not [string]::IsNullOrWhiteSpace([string]$script:DecisionDashboardConsumerScriptFileRaw)) {
        $scriptPath = Resolve-FullPath -Root $RepoRoot -Value ([string]$script:DecisionDashboardConsumerScriptFileRaw)
    }
    $binaryCfg = Resolve-PolicyToolBinaryConfig -RepoRootPath $RepoRoot -RawBinaryFile ([string]$script:DecisionDashboardConsumerBinaryFileRaw) -ToolName \"rollout-decision-dashboard-consumer\" -LegacyBaseName \"novovm-rollout-decision-dashboard-consumer\"
    $binaryPath = [string]$binaryCfg.binary_path

    $inputPath = ""
    if (-not [string]::IsNullOrWhiteSpace([string]$script:DecisionDashboardConsumerInputFileRaw)) {
        $inputPath = Resolve-FullPath -Root $RepoRoot -Value ([string]$script:DecisionDashboardConsumerInputFileRaw)
    } elseif (-not [string]::IsNullOrWhiteSpace([string]$script:DecisionDashboardExportOutputPath)) {
        $inputPath = [string]$script:DecisionDashboardExportOutputPath
    } else {
        $inputPath = Resolve-FullPath -Root $RepoRoot -Value "artifacts/runtime/rollout/control-plane-decision-dashboard.jsonl"
    }

    $outputPath = Resolve-FullPath -Root $RepoRoot -Value "artifacts/runtime/rollout/control-plane-decision-dashboard-state.json"
    if (-not [string]::IsNullOrWhiteSpace([string]$script:DecisionDashboardConsumerOutputFileRaw)) {
        $outputPath = Resolve-FullPath -Root $RepoRoot -Value ([string]$script:DecisionDashboardConsumerOutputFileRaw)
    }
    $alertsPath = Resolve-FullPath -Root $RepoRoot -Value "artifacts/runtime/rollout/control-plane-decision-dashboard-alerts.jsonl"
    if (-not [string]::IsNullOrWhiteSpace([string]$script:DecisionDashboardConsumerAlertsFileRaw)) {
        $alertsPath = Resolve-FullPath -Root $RepoRoot -Value ([string]$script:DecisionDashboardConsumerAlertsFileRaw)
    }

    $modeNorm = ([string]$script:DecisionDashboardConsumerMode).Trim().ToLowerInvariant()
    if ($modeNorm -ne "all" -and $modeNorm -ne "blocked") {
        $modeNorm = "all"
    }
    $script:DecisionDashboardConsumerMode = $modeNorm
    $script:DecisionDashboardConsumerCheckSeconds = [Math]::Max(2, [int]$script:DecisionDashboardConsumerCheckSeconds)
    $script:DecisionDashboardConsumerTail = [Math]::Max(0, [int]$script:DecisionDashboardConsumerTail)
    $script:DecisionDashboardConsumerScriptPath = $scriptPath
    $script:DecisionDashboardConsumerBinaryPath = $binaryPath
    $script:DecisionDashboardConsumerInputPath = $inputPath
    $script:DecisionDashboardConsumerOutputPath = $outputPath
    $script:DecisionDashboardConsumerAlertsPath = $alertsPath
    $script:DecisionDashboardConsumerConfigSignature = ("enabled={0}|check={1}|mode={2}|tail={3}|input={4}|output={5}|alerts={6}|binary={7}|script={8}" -f [bool]$script:DecisionDashboardConsumerEnabled, [int]$script:DecisionDashboardConsumerCheckSeconds, [string]$script:DecisionDashboardConsumerMode, [int]$script:DecisionDashboardConsumerTail, [string]$script:DecisionDashboardConsumerInputPath, [string]$script:DecisionDashboardConsumerOutputPath, [string]$script:DecisionDashboardConsumerAlertsPath, [string]$script:DecisionDashboardConsumerBinaryPath, [string]$script:DecisionDashboardConsumerScriptPath)
}

function Apply-DecisionDeliveryRuntimeConfig {
    param(
        [string]$RepoRoot
    )
    $binaryCfg = Resolve-PolicyToolBinaryConfig -RepoRootPath $RepoRoot -RawBinaryFile ([string]$script:DecisionDeliveryBinaryFileRaw) -ToolName \"rollout-decision-delivery\" -LegacyBaseName \"novovm-rollout-decision-delivery\"
    $binaryPath = [string]$binaryCfg.binary_path
    $script:DecisionDeliveryBinaryPath = $binaryPath
    $script:DecisionDeliveryConfigSignature = ("binary={0}" -f [string]$script:DecisionDeliveryBinaryPath)
}

function Apply-DecisionRouteRuntimeConfig {
    param(
        [string]$RepoRoot
    )
    $binaryCfg = Resolve-PolicyToolBinaryConfig -RepoRootPath $RepoRoot -RawBinaryFile ([string]$script:DecisionRouteBinaryFileRaw) -ToolName \"rollout-decision-route\" -LegacyBaseName \"novovm-rollout-decision-route\"
    $binaryPath = [string]$binaryCfg.binary_path
    $script:DecisionRouteBinaryPath = $binaryPath
    $script:DecisionRouteConfigSignature = ("binary={0}" -f [string]$script:DecisionRouteBinaryPath)
}

function Apply-RiskBlockedSelectRuntimeConfig {
    param(
        [string]$RepoRoot
    )
    $binaryCfg = Resolve-PolicyToolBinaryConfig -RepoRootPath $RepoRoot -RawBinaryFile ([string]$script:RiskBlockedSelectBinaryFileRaw) -ToolName \"risk-blocked-select\" -LegacyBaseName \"novovm-risk-blocked-select\"
    $binaryPath = [string]$binaryCfg.binary_path
    $script:RiskBlockedSelectBinaryPath = $binaryPath
    $script:RiskBlockedSelectConfigSignature = ("binary={0}" -f [string]$script:RiskBlockedSelectBinaryPath)
}

function Apply-RiskBlockedMapBuildRuntimeConfig {
    param(
        [string]$RepoRoot
    )
    $binaryCfg = Resolve-PolicyToolBinaryConfig -RepoRootPath $RepoRoot -RawBinaryFile ([string]$script:RiskBlockedMapBuildBinaryFileRaw) -ToolName \"risk-blocked-map-build\" -LegacyBaseName \"novovm-risk-blocked-map-build\"
    $binaryPath = [string]$binaryCfg.binary_path
    $script:RiskBlockedMapBuildBinaryPath = $binaryPath
    $script:RiskBlockedMapBuildConfigSignature = ("binary={0}" -f [string]$script:RiskBlockedMapBuildBinaryPath)
}

function Apply-RiskActionEvalRuntimeConfig {
    param(
        [string]$RepoRoot
    )
    $binaryCfg = Resolve-PolicyToolBinaryConfig -RepoRootPath $RepoRoot -RawBinaryFile ([string]$script:RiskActionEvalBinaryFileRaw) -ToolName \"risk-action-eval\" -LegacyBaseName \"novovm-risk-action-eval\"
    $binaryPath = [string]$binaryCfg.binary_path
    $script:RiskActionEvalBinaryPath = $binaryPath
    $script:RiskActionEvalConfigSignature = ("binary={0}" -f [string]$script:RiskActionEvalBinaryPath)
}

function Apply-RiskActionMatrixBuildRuntimeConfig {
    param(
        [string]$RepoRoot
    )
    $binaryCfg = Resolve-PolicyToolBinaryConfig -RepoRootPath $RepoRoot -RawBinaryFile ([string]$script:RiskActionMatrixBuildBinaryFileRaw) -ToolName \"risk-action-matrix-build\" -LegacyBaseName \"novovm-risk-action-matrix-build\"
    $binaryPath = [string]$binaryCfg.binary_path
    $script:RiskActionMatrixBuildBinaryPath = $binaryPath
    $script:RiskActionMatrixBuildConfigSignature = ("binary={0}" -f [string]$script:RiskActionMatrixBuildBinaryPath)
}

function Apply-FailoverPolicyMatrixBuildRuntimeConfig {
    param(
        [string]$RepoRoot
    )
    $binaryCfg = Resolve-PolicyToolBinaryConfig -RepoRootPath $RepoRoot -RawBinaryFile ([string]$script:FailoverPolicyMatrixBuildBinaryFileRaw) -ToolName \"failover-policy-matrix-build\" -LegacyBaseName \"novovm-failover-policy-matrix-build\"
    $binaryPath = [string]$binaryCfg.binary_path
    $script:FailoverPolicyMatrixBuildBinaryPath = $binaryPath
    $script:FailoverPolicyMatrixBuildConfigSignature = ("binary={0}" -f [string]$script:FailoverPolicyMatrixBuildBinaryPath)
}

function Apply-RiskMatrixSelectRuntimeConfig {
    param(
        [string]$RepoRoot
    )
    $binaryCfg = Resolve-PolicyToolBinaryConfig -RepoRootPath $RepoRoot -RawBinaryFile ([string]$script:RiskMatrixSelectBinaryFileRaw) -ToolName \"risk-matrix-select\" -LegacyBaseName \"novovm-risk-matrix-select\"
    $binaryPath = [string]$binaryCfg.binary_path
    $script:RiskMatrixSelectBinaryPath = $binaryPath
    $script:RiskMatrixSelectConfigSignature = ("binary={0}" -f [string]$script:RiskMatrixSelectBinaryPath)
}

function Invoke-DecisionDashboardConsumerIfDue {
    param(
        [string]$Source,
        [string]$AuditPath,
        [string]$QueuePath,
        [string]$PlanAction,
        [string]$ControlOpId,
        [string]$ControllerId
    )
    if (-not [bool]$script:DecisionDashboardConsumerEnabled) {
        return
    }
    $nowMs = Now-Ms
    if ($script:DecisionDashboardConsumerNextRunMs -gt 0 -and $nowMs -lt $script:DecisionDashboardConsumerNextRunMs) {
        return
    }
    $script:DecisionDashboardConsumerNextRunMs = $nowMs + ([int64]$script:DecisionDashboardConsumerCheckSeconds * 1000)

    $hasBinary = (-not [string]::IsNullOrWhiteSpace([string]$script:DecisionDashboardConsumerBinaryPath)) -and (Test-Path -LiteralPath $script:DecisionDashboardConsumerBinaryPath)
    $hasScript = (-not [string]::IsNullOrWhiteSpace([string]$script:DecisionDashboardConsumerScriptPath)) -and (Test-Path -LiteralPath $script:DecisionDashboardConsumerScriptPath)
    if (-not $hasBinary -and -not $hasScript) {
        Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                timestamp_utc = [DateTime]::UtcNow.ToString("o")
                control_operation_id = $ControlOpId
                controller_id = $ControllerId
                queue_file = $QueuePath
                action = $PlanAction
                result = "decision_dashboard_consumer_error"
                source = ([string]$Source).ToLowerInvariant()
                consumer_mode = [string]$script:DecisionDashboardConsumerMode
                consumer_tail = [int]$script:DecisionDashboardConsumerTail
                consumer_output_file = [string]$script:DecisionDashboardConsumerOutputPath
                consumer_alerts_file = [string]$script:DecisionDashboardConsumerAlertsPath
                error = "consumer runtime not found (binary+script missing)"
            })
        return
    }

    $rustArgs = @(
        "--input-file", [string]$script:DecisionDashboardConsumerInputPath,
        "--output-file", [string]$script:DecisionDashboardConsumerOutputPath,
        "--alerts-file", [string]$script:DecisionDashboardConsumerAlertsPath,
        "--mode", [string]$script:DecisionDashboardConsumerMode
    )
    $psArgs = @(
        "-ExecutionPolicy", "Bypass",
        "-File", [string]$script:DecisionDashboardConsumerScriptPath,
        "-InputFile", [string]$script:DecisionDashboardConsumerInputPath,
        "-OutputFile", [string]$script:DecisionDashboardConsumerOutputPath,
        "-AlertsFile", [string]$script:DecisionDashboardConsumerAlertsPath,
        "-Mode", [string]$script:DecisionDashboardConsumerMode
    )
    if ([int]$script:DecisionDashboardConsumerTail -gt 0) {
        $rustArgs += @("--tail", [string][int]$script:DecisionDashboardConsumerTail)
        $psArgs += @("-Tail", [string][int]$script:DecisionDashboardConsumerTail)
    }

    try {
        if ($hasBinary) {
            Invoke-RolloutPolicyTool -BinaryPath $script:DecisionDashboardConsumerBinaryPath -ToolName "rollout-decision-dashboard-consumer" -Args $rustArgs | Out-Null
        } else {
            & powershell @psArgs | Out-Null
        }
        Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                timestamp_utc = [DateTime]::UtcNow.ToString("o")
                control_operation_id = $ControlOpId
                controller_id = $ControllerId
                queue_file = $QueuePath
                action = $PlanAction
                result = "decision_dashboard_consumer"
                source = ([string]$Source).ToLowerInvariant()
                consumer_mode = [string]$script:DecisionDashboardConsumerMode
                consumer_tail = [int]$script:DecisionDashboardConsumerTail
                consumer_output_file = [string]$script:DecisionDashboardConsumerOutputPath
                consumer_alerts_file = [string]$script:DecisionDashboardConsumerAlertsPath
                error = ""
            })
    } catch {
        Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                timestamp_utc = [DateTime]::UtcNow.ToString("o")
                control_operation_id = $ControlOpId
                controller_id = $ControllerId
                queue_file = $QueuePath
                action = $PlanAction
                result = "decision_dashboard_consumer_error"
                source = ([string]$Source).ToLowerInvariant()
                consumer_mode = [string]$script:DecisionDashboardConsumerMode
                consumer_tail = [int]$script:DecisionDashboardConsumerTail
                consumer_output_file = [string]$script:DecisionDashboardConsumerOutputPath
                consumer_alerts_file = [string]$script:DecisionDashboardConsumerAlertsPath
                error = $_.Exception.Message
            })
    }
}

function Resolve-SiteConflictRiskThrottlePolicy {
    param(
        [object]$Risk,
        [string]$Source = "cycle",
        [string]$SiteId = ""
    )
    $sitePriority = Site-BasePriority -Site $SiteId
    $policy = [pscustomobject]@{
        level = "green"
        cap_concurrent = 0
        pause_seconds = 0
        block_dispatch = $false
        scope = "global"
        site_priority = [int]$sitePriority
        rule_source = ""
        min_site_priority = [int]::MinValue
        priority_gate = "disabled"
    }
    if (-not $script:SiteConflictRiskAutoThrottleEnabled -or -not [bool]$Risk.enabled) {
        return $policy
    }
    $level = ([string]$Risk.worst_level).ToLowerInvariant()
    if ($level -ne "green" -and $level -ne "yellow" -and $level -ne "orange" -and $level -ne "red") {
        $level = "green"
    }
    $sourceNorm = ([string]$Source).Trim().ToLowerInvariant()
    if ($sourceNorm -ne "startup" -and $sourceNorm -ne "cycle") {
        $sourceNorm = "cycle"
    }
    $selected = Select-RiskActionMatrix -SiteId $SiteId
    $policy.scope = [string]$selected.scope
    $entry = $null
    $fallbackEntry = $null
    $hasRiskActionEvalBinary = (-not [string]::IsNullOrWhiteSpace([string]$script:RiskActionEvalBinaryPath)) -and (Test-Path -LiteralPath $script:RiskActionEvalBinaryPath)
    if ($hasRiskActionEvalBinary -and $null -ne $selected -and $null -ne $selected.matrix) {
        try {
            $matrixJson = (@($selected.matrix) | ConvertTo-Json -Depth 20 -Compress)
            if ([string]::IsNullOrWhiteSpace($matrixJson)) {
                $matrixJson = "[]"
            }
            $rustArgs = @(
                "--source", $sourceNorm,
                "--level", $level,
                "--site-priority", ([string][int]$sitePriority),
                "--scope", ([string]$selected.scope),
                "--matrix-json", $matrixJson
            )
            $rustOutput = Invoke-RolloutPolicyTool -BinaryPath $script:RiskActionEvalBinaryPath -ToolName "risk-action-eval" -Args $rustArgs -CaptureOutput
            if ($LASTEXITCODE -eq 0) {
                $rustText = (($rustOutput | Out-String).Trim())
                if (-not [string]::IsNullOrWhiteSpace($rustText)) {
                    $rustLine = ($rustText -split "`r?`n" | Select-Object -Last 1)
                    $rustObj = $rustLine | ConvertFrom-Json
                    if ($null -ne $rustObj.level) { $policy.level = [string]$rustObj.level }
                    if ($null -ne $rustObj.cap_concurrent) { $policy.cap_concurrent = [Math]::Max(0, [int]$rustObj.cap_concurrent) }
                    if ($null -ne $rustObj.pause_seconds) { $policy.pause_seconds = [Math]::Max(0, [int]$rustObj.pause_seconds) }
                    if ($null -ne $rustObj.block_dispatch) { $policy.block_dispatch = [bool]$rustObj.block_dispatch }
                    if ($null -ne $rustObj.scope -and -not [string]::IsNullOrWhiteSpace([string]$rustObj.scope)) { $policy.scope = [string]$rustObj.scope }
                    if ($null -ne $rustObj.rule_source) { $policy.rule_source = [string]$rustObj.rule_source }
                    if ($null -ne $rustObj.min_site_priority) { $policy.min_site_priority = [int]$rustObj.min_site_priority }
                    if ($null -ne $rustObj.priority_gate -and -not [string]::IsNullOrWhiteSpace([string]$rustObj.priority_gate)) { $policy.priority_gate = [string]$rustObj.priority_gate }
                    return $policy
                }
            }
        } catch {
        }
    }
    if ($null -ne $selected -and $null -ne $selected.matrix) {
        $sourceCandidates = @()
        $wildcardCandidates = @()
        foreach ($it in @($selected.matrix)) {
            if ([string]$it.level -ne $level) {
                continue
            }
            if ([string]$it.source -eq $sourceNorm) {
                if ($null -eq $fallbackEntry) { $fallbackEntry = $it }
                if ([int]$it.min_site_priority -le $sitePriority) {
                    $sourceCandidates += $it
                }
                continue
            }
            if ([string]$it.source -eq "*") {
                if ($null -eq $fallbackEntry) { $fallbackEntry = $it }
                if ([int]$it.min_site_priority -le $sitePriority) {
                    $wildcardCandidates += $it
                }
            }
        }
        if ($sourceCandidates.Count -gt 0) {
            $entry = @($sourceCandidates | Sort-Object @{ Expression = { [int]$_.min_site_priority }; Descending = $true })[0]
        } elseif ($wildcardCandidates.Count -gt 0) {
            $entry = @($wildcardCandidates | Sort-Object @{ Expression = { [int]$_.min_site_priority }; Descending = $true })[0]
        }
    }
    if ($null -ne $entry) {
        $policy.level = [string]$entry.level
        $policy.cap_concurrent = [Math]::Max(0, [int]$entry.cap_concurrent)
        $policy.pause_seconds = [Math]::Max(0, [int]$entry.pause_seconds)
        $policy.block_dispatch = [bool]$entry.block_dispatch
        $policy.rule_source = [string]$entry.source
        $policy.min_site_priority = [int]$entry.min_site_priority
        $policy.priority_gate = "pass"
        return $policy
    }
    if ($null -ne $fallbackEntry) {
        $policy.level = [string]$fallbackEntry.level
        $policy.cap_concurrent = [Math]::Max(0, [int]$fallbackEntry.cap_concurrent)
        $policy.pause_seconds = [Math]::Max(0, [int]$fallbackEntry.pause_seconds)
        $policy.block_dispatch = $true
        $policy.rule_source = [string]$fallbackEntry.source
        $policy.min_site_priority = [int]$fallbackEntry.min_site_priority
        $policy.priority_gate = "blocked_by_site_priority"
        return $policy
    }
    return $policy
}

function Emit-SiteRiskThrottlePolicyAuditIfChanged {
    param(
        [string]$Source,
        [object]$Risk,
        [object]$Policy,
        [string]$AuditPath,
        [string]$QueuePath,
        [string]$PlanAction,
        [string]$ControlOpId,
        [string]$ControllerId
    )
    if ($null -eq $Policy) {
        return
    }
    if ($null -eq $script:SiteRiskThrottlePolicyFingerprintMap) {
        $script:SiteRiskThrottlePolicyFingerprintMap = @{}
    }
    $src = ([string]$Source).Trim().ToLowerInvariant()
    if ([string]::IsNullOrWhiteSpace($src)) {
        $src = "cycle"
    }
    $worstSite = ""
    $worstLevel = "green"
    $worstScore = 0.0
    if ($null -ne $Risk) {
        if ($null -ne $Risk.worst_site_id) { $worstSite = [string]$Risk.worst_site_id }
        if ($null -ne $Risk.worst_level -and -not [string]::IsNullOrWhiteSpace([string]$Risk.worst_level)) { $worstLevel = [string]$Risk.worst_level }
        if ($null -ne $Risk.worst_score) { $worstScore = [double]$Risk.worst_score }
    }
    $fp = [string]::Join("|", @(
            $src,
            [string]$Policy.scope,
            [string]$Policy.level,
            [string]$Policy.rule_source,
            [string]([int]$Policy.site_priority),
            [string]([int]$Policy.min_site_priority),
            [string]([int]$Policy.cap_concurrent),
            [string]([int]$Policy.pause_seconds),
            [string]([bool]$Policy.block_dispatch),
            [string]$Policy.priority_gate,
            $worstSite,
            $worstLevel
        ))
    if ($script:SiteRiskThrottlePolicyFingerprintMap.ContainsKey($src) -and [string]$script:SiteRiskThrottlePolicyFingerprintMap[$src] -eq $fp) {
        return
    }
    $script:SiteRiskThrottlePolicyFingerprintMap[$src] = $fp
    Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
            timestamp_utc = [DateTime]::UtcNow.ToString("o")
            control_operation_id = $ControlOpId
            controller_id = $ControllerId
            queue_file = $QueuePath
            action = $PlanAction
            result = "site_risk_throttle_policy"
            source = $src
            worst_site_id = $worstSite
            worst_level = $worstLevel
            worst_score = [Math]::Round($worstScore, 4)
            risk_policy_scope = [string]$Policy.scope
            risk_policy_rule_source = [string]$Policy.rule_source
            risk_policy_site_priority = [int]$Policy.site_priority
            risk_policy_min_site_priority = [int]$Policy.min_site_priority
            risk_policy_priority_gate = [string]$Policy.priority_gate
            risk_policy_cap_concurrent = [int]$Policy.cap_concurrent
            risk_policy_pause_seconds = [int]$Policy.pause_seconds
            risk_policy_block_dispatch = [bool]$Policy.block_dispatch
            error = ""
        })
}

function Emit-RolloutDecisionSummaryIfChanged {
    param(
        [string]$Source,
        [object]$Risk,
        [object]$RiskPolicy,
        [string]$Role,
        [int]$EffectiveConcurrent,
        [int]$EffectivePauseSeconds,
        [bool]$DispatchBlocked,
        [string]$AuditPath,
        [string]$QueuePath,
        [string]$PlanAction,
        [string]$ControlOpId,
        [string]$ControllerId
    )
    function Resolve-RolloutDecisionAlertLevel {
        param(
            [string]$RoleValue,
            [bool]$Blocked
        )
        if (-not $Blocked) {
            return "info"
        }
        return "high"
    }
    function Resolve-RolloutDecisionAlertChannel {
        param(
            [string]$RoleValue,
            [string]$AlertLevelValue,
            [bool]$Blocked
        )
        if (-not $Blocked) {
            return "ops-observe"
        }
        return "ops-oncall"
    }
    function Resolve-RolloutDecisionAlertTarget {
        param([string]$AlertChannelValue)
        $k = ([string]$AlertChannelValue).Trim().ToLowerInvariant()
        if ([string]::IsNullOrWhiteSpace($k)) {
            return ""
        }
        if ($k -eq "ops-oncall") {
            return "ops-oncall"
        }
        return "ops-observe"
    }
    function Resolve-RolloutDecisionDeliveryType {
        param([string]$AlertTargetValue)
        $targetNorm = ([string]$AlertTargetValue).Trim().ToLowerInvariant()
        if ([string]::IsNullOrWhiteSpace($targetNorm)) {
            return "webhook"
        }
        if ($targetNorm -eq "ops-observe") { return "im" }
        return "webhook"
    }
    function Resolve-RolloutDecisionDeliveryEndpoint {
        param([string]$AlertTargetValue)
        $targetNorm = ([string]$AlertTargetValue).Trim().ToLowerInvariant()
        if ([string]::IsNullOrWhiteSpace($targetNorm)) {
            return ""
        }
        if ($null -ne $script:DecisionDeliveryEndpointMap -and $script:DecisionDeliveryEndpointMap.ContainsKey($targetNorm)) {
            return [string]$script:DecisionDeliveryEndpointMap[$targetNorm]
        }
        return ""
    }
    function Resolve-RolloutDecisionRouting {
        param(
            [string]$RoleValue,
            [bool]$Blocked
        )
        $alertLevel = Resolve-RolloutDecisionAlertLevel -RoleValue $RoleValue -Blocked $Blocked
        $alertChannel = Resolve-RolloutDecisionAlertChannel -RoleValue $RoleValue -AlertLevelValue $alertLevel -Blocked $Blocked
        $alertTarget = Resolve-RolloutDecisionAlertTarget -AlertChannelValue $alertChannel
        $deliveryType = Resolve-RolloutDecisionDeliveryType -AlertTargetValue $alertTarget
        $deliveryAction = ("dispatch:" + $deliveryType + ":" + $alertTarget)

        $hasRouteBinary = (-not [string]::IsNullOrWhiteSpace([string]$script:DecisionRouteBinaryPath)) -and (Test-Path -LiteralPath $script:DecisionRouteBinaryPath)
        if ($hasRouteBinary) {
            try {
                $targetMapJson = "{}"
                if ($null -ne $script:DecisionAlertTargetMap -and $script:DecisionAlertTargetMap.Count -gt 0) {
                    $targetMapJson = ($script:DecisionAlertTargetMap | ConvertTo-Json -Compress -Depth 10)
                }
                $deliveryMapJson = "{}"
                if ($null -ne $script:DecisionAlertDeliveryTypeMap -and $script:DecisionAlertDeliveryTypeMap.Count -gt 0) {
                    $deliveryMapJson = ($script:DecisionAlertDeliveryTypeMap | ConvertTo-Json -Compress -Depth 10)
                }
                $rustArgs = @(
                    "--role", ([string]$RoleValue),
                    "--blocked", ([string][bool]$Blocked),
                    "--alert-target-map-json", $targetMapJson,
                    "--alert-delivery-type-map-json", $deliveryMapJson
                )
                $rustOutput = Invoke-RolloutPolicyTool -BinaryPath $script:DecisionRouteBinaryPath -ToolName "rollout-decision-route" -Args $rustArgs -CaptureOutput
                if ($LASTEXITCODE -eq 0) {
                    $rustText = (($rustOutput | Out-String).Trim())
                    if (-not [string]::IsNullOrWhiteSpace($rustText)) {
                        $rustLine = ($rustText -split "`r?`n" | Select-Object -Last 1)
                        $rustObj = $rustLine | ConvertFrom-Json
                        if ($null -ne $rustObj.decision_alert_level -and -not [string]::IsNullOrWhiteSpace([string]$rustObj.decision_alert_level)) { $alertLevel = [string]$rustObj.decision_alert_level }
                        if ($null -ne $rustObj.decision_alert_channel -and -not [string]::IsNullOrWhiteSpace([string]$rustObj.decision_alert_channel)) { $alertChannel = [string]$rustObj.decision_alert_channel }
                        if ($null -ne $rustObj.decision_alert_target -and -not [string]::IsNullOrWhiteSpace([string]$rustObj.decision_alert_target)) { $alertTarget = [string]$rustObj.decision_alert_target }
                        if ($null -ne $rustObj.decision_delivery_type -and -not [string]::IsNullOrWhiteSpace([string]$rustObj.decision_delivery_type)) { $deliveryType = [string]$rustObj.decision_delivery_type }
                        if ($null -ne $rustObj.decision_delivery_action -and -not [string]::IsNullOrWhiteSpace([string]$rustObj.decision_delivery_action)) { $deliveryAction = [string]$rustObj.decision_delivery_action }
                    }
                }
            } catch {
            }
        }
        return [pscustomobject]@{
            alert_level = $alertLevel
            alert_channel = $alertChannel
            alert_target = $alertTarget
            delivery_type = $deliveryType
            delivery_action = $deliveryAction
        }
    }
    function Send-RolloutDecisionDelivery {
        param(
            [string]$DeliveryTypeValue,
            [string]$DeliveryActionValue,
            [string]$AlertLevelValue,
            [string]$AlertChannelValue,
            [string]$AlertTargetValue
        )
        $typeNorm = ([string]$DeliveryTypeValue).Trim().ToLowerInvariant()
        if ([string]::IsNullOrWhiteSpace($typeNorm)) { $typeNorm = "webhook" }
        $status = "skipped"
        $ok = $false
        $targetNorm = ([string]$AlertTargetValue).Trim().ToLowerInvariant()
        $endpoint = Resolve-RolloutDecisionDeliveryEndpoint -AlertTargetValue $AlertTargetValue
        $recipient = ""
        if ($typeNorm -eq "email") {
            if ($null -ne $script:DecisionDeliveryEmailTargetMap -and $script:DecisionDeliveryEmailTargetMap.ContainsKey($targetNorm)) {
                $recipient = [string]$script:DecisionDeliveryEmailTargetMap[$targetNorm]
            } elseif (([string]$AlertTargetValue).Contains("@")) {
                $recipient = [string]$AlertTargetValue
            }
        }
        $err = ""
        $payload = [pscustomobject][ordered]@{
            action = $DeliveryActionValue
            source = $src
            decision_alert_level = $AlertLevelValue
            decision_alert_channel = $AlertChannelValue
            decision_alert_target = $AlertTargetValue
            controller_role = $roleText
            dispatch_blocked = [bool]$DispatchBlocked
            worst_site_id = $worstSite
            worst_level = $worstLevel
            worst_score = [Math]::Round($worstScore, 4)
            control_operation_id = $ControlOpId
            controller_id = $ControllerId
            queue_file = $QueuePath
            plan_action = $PlanAction
            risk_policy_active_profile = $riskActiveProfile
            state_recovery_enabled = [bool]$script:StateRecoveryEnabled
            failover_mode = [bool]$script:StateReplicaFailoverMode
            failover_converge_scope = $convergeScope
            failover_converge_config_enabled = [bool]$convergeConfiguredEnabled
            failover_converge_enabled = [bool]$convergeEffectiveEnabled
            failover_converge_max_concurrent = [int]$convergeMaxConcurrent
            failover_converge_min_dispatch_pause_seconds = [int]$convergeMinPause
            failover_converge_block_on_snapshot_red = [bool]$convergeBlockSnapshotRed
            failover_converge_block_on_replay_red = [bool]$convergeBlockReplayRed
            timestamp_utc = [DateTime]::UtcNow.ToString("o")
        }
        $payloadBodyCompact = $payload | ConvertTo-Json -Depth 8 -Compress
        $payloadBodyPretty = $payload | ConvertTo-Json -Depth 8
        $rustHandled = $false
        $hasRustBinary = (-not [string]::IsNullOrWhiteSpace([string]$script:DecisionDeliveryBinaryPath)) -and (Test-Path -LiteralPath $script:DecisionDeliveryBinaryPath)
        if ($hasRustBinary) {
            try {
                $rustArgs = @(
                    "--delivery-type", $typeNorm,
                    "--endpoint", $endpoint,
                    "--recipient", $recipient,
                    "--payload-json", $payloadBodyCompact,
                    "--alert-level", ([string]$AlertLevelValue),
                    "--source", ([string]$src),
                    "--smtp-server", ([string]$script:DecisionDeliveryEmailSmtpServer),
                    "--smtp-port", ([string][int]$script:DecisionDeliveryEmailSmtpPort),
                    "--smtp-from", ([string]$script:DecisionDeliveryEmailFrom),
                    "--smtp-use-ssl", ([string][bool]$script:DecisionDeliveryEmailUseSsl),
                    "--smtp-user", ([string]$script:DecisionDeliveryEmailSmtpUser),
                    "--smtp-password-env", ([string]$script:DecisionDeliveryEmailSmtpPasswordEnv),
                    "--timeout-seconds", "5"
                )
                $rustOutput = Invoke-RolloutPolicyTool -BinaryPath $script:DecisionDeliveryBinaryPath -ToolName "rollout-decision-delivery" -Args $rustArgs -CaptureOutput
                $rustExit = $LASTEXITCODE
                if ($rustExit -eq 0) {
                    $rustText = (($rustOutput | Out-String).Trim())
                    if (-not [string]::IsNullOrWhiteSpace($rustText)) {
                        $rustLine = ($rustText -split "`r?`n" | Select-Object -Last 1)
                        $rustObj = $rustLine | ConvertFrom-Json
                        $status = [string]$rustObj.status
                        if ([string]::IsNullOrWhiteSpace($status)) { $status = "failed" }
                        $ok = [bool]$rustObj.ok
                        if ($null -ne $rustObj.endpoint -and -not [string]::IsNullOrWhiteSpace([string]$rustObj.endpoint)) { $endpoint = [string]$rustObj.endpoint }
                        if ($null -ne $rustObj.recipient -and -not [string]::IsNullOrWhiteSpace([string]$rustObj.recipient)) { $recipient = [string]$rustObj.recipient }
                        $err = [string]$rustObj.error
                        $rustHandled = $true
                    }
                }
            } catch {
                $rustHandled = $false
            }
        }
        if (-not $rustHandled) {
            $err = ""
            if ($typeNorm -eq "webhook" -or $typeNorm -eq "im") {
                if ([string]::IsNullOrWhiteSpace($endpoint)) {
                    $status = "no_endpoint"
                } elseif ((-not $endpoint.StartsWith("http://", [System.StringComparison]::OrdinalIgnoreCase)) -and (-not $endpoint.StartsWith("https://", [System.StringComparison]::OrdinalIgnoreCase))) {
                    $status = "invalid_endpoint"
                    $err = "emergency fallback only supports explicit http(s) endpoint"
                } else {
                    try {
                        Invoke-RestMethod -Method Post -Uri $endpoint -ContentType "application/json" -Body $payloadBodyCompact -TimeoutSec 5 | Out-Null
                        $status = "sent"
                        $ok = $true
                    } catch {
                        $status = "failed"
                        $err = $_.Exception.Message
                    }
                }
            } elseif ($typeNorm -eq "email") {
                $status = "fallback_email_disabled"
                $err = "emergency fallback does not send smtp email; rust decision-delivery required"
            } else {
                $status = "unsupported_type"
                $err = "emergency fallback only supports webhook/im with explicit endpoint"
            }
        }
        Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                timestamp_utc = [DateTime]::UtcNow.ToString("o")
                control_operation_id = $ControlOpId
                controller_id = $ControllerId
                queue_file = $QueuePath
                action = $PlanAction
                result = "rollout_decision_delivery"
                source = $src
                decision_alert_level = $AlertLevelValue
                decision_alert_channel = $AlertChannelValue
                decision_alert_target = $AlertTargetValue
                decision_delivery_type = $typeNorm
                decision_delivery_action = $DeliveryActionValue
                delivery_status = $status
                delivery_ok = [bool]$ok
                delivery_endpoint = $endpoint
                delivery_recipient = $recipient
                risk_policy_active_profile = $riskActiveProfile
                state_recovery_enabled = [bool]$script:StateRecoveryEnabled
                failover_mode = [bool]$script:StateReplicaFailoverMode
                failover_converge_scope = $convergeScope
                failover_converge_config_enabled = [bool]$convergeConfiguredEnabled
                failover_converge_enabled = [bool]$convergeEffectiveEnabled
                failover_converge_max_concurrent = [int]$convergeMaxConcurrent
                failover_converge_min_dispatch_pause_seconds = [int]$convergeMinPause
                failover_converge_block_on_snapshot_red = [bool]$convergeBlockSnapshotRed
                failover_converge_block_on_replay_red = [bool]$convergeBlockReplayRed
                error = $err
            })
    }
    if ($null -eq $script:RolloutDecisionSummaryFingerprintMap) {
        $script:RolloutDecisionSummaryFingerprintMap = @{}
    }
    $src = ([string]$Source).Trim().ToLowerInvariant()
    if ([string]::IsNullOrWhiteSpace($src)) { $src = "cycle" }
    $worstSite = ""
    $worstLevel = "green"
    $worstScore = 0.0
    if ($null -ne $Risk) {
        if ($null -ne $Risk.worst_site_id) { $worstSite = [string]$Risk.worst_site_id }
        if ($null -ne $Risk.worst_level -and -not [string]::IsNullOrWhiteSpace([string]$Risk.worst_level)) { $worstLevel = [string]$Risk.worst_level }
        if ($null -ne $Risk.worst_score) { $worstScore = [double]$Risk.worst_score }
    }
    $policyScope = "global"
    $policyRuleSource = ""
    $policySitePriority = 0
    $policyMinSitePriority = [int]::MinValue
    $policyPriorityGate = "disabled"
    $policyCap = 0
    $policyPause = 0
    $policyBlock = $false
    if ($null -ne $RiskPolicy) {
        if ($null -ne $RiskPolicy.scope) { $policyScope = [string]$RiskPolicy.scope }
        if ($null -ne $RiskPolicy.rule_source) { $policyRuleSource = [string]$RiskPolicy.rule_source }
        if ($null -ne $RiskPolicy.site_priority) { $policySitePriority = [int]$RiskPolicy.site_priority }
        if ($null -ne $RiskPolicy.min_site_priority) { $policyMinSitePriority = [int]$RiskPolicy.min_site_priority }
        if ($null -ne $RiskPolicy.priority_gate) { $policyPriorityGate = [string]$RiskPolicy.priority_gate }
        if ($null -ne $RiskPolicy.cap_concurrent) { $policyCap = [int]$RiskPolicy.cap_concurrent }
        if ($null -ne $RiskPolicy.pause_seconds) { $policyPause = [int]$RiskPolicy.pause_seconds }
        if ($null -ne $RiskPolicy.block_dispatch) { $policyBlock = [bool]$RiskPolicy.block_dispatch }
    }
    $riskActiveProfile = [string]$script:RiskPolicyActiveProfile
    if ([string]::IsNullOrWhiteSpace($riskActiveProfile)) { $riskActiveProfile = "base" }
    $convergeSite = $worstSite
    if ([string]::IsNullOrWhiteSpace($convergeSite) -and -not [string]::IsNullOrWhiteSpace([string]$script:SiteId)) {
        $convergeSite = [string]$script:SiteId
    }
    $convergeScope = "global"
    $convergeProfile = $script:StateReplicaFailoverConvergeProfileGlobal
    if ($null -eq $convergeProfile) {
        $convergeProfile = [pscustomobject]@{
            enabled = $true
            max_concurrent_plans = 1
            min_dispatch_pause_seconds = [Math]::Max(1, [int]$script:StateReplicaFailoverCooldownSec)
            block_on_snapshot_red = $true
            block_on_replay_red = $true
        }
    }
    $convergeResolved = Select-FailoverConvergeProfile -SiteId $convergeSite
    if ($null -ne $convergeResolved) {
        if ($null -ne $convergeResolved.scope -and -not [string]::IsNullOrWhiteSpace([string]$convergeResolved.scope)) { $convergeScope = [string]$convergeResolved.scope }
        if ($null -ne $convergeResolved.profile) { $convergeProfile = $convergeResolved.profile }
    }
    $convergeConfiguredEnabled = [bool]$convergeProfile.enabled
    $convergeEffectiveEnabled = ([bool]$script:StateRecoveryEnabled -and [bool]$script:StateReplicaFailoverMode -and [bool]$convergeConfiguredEnabled)
    $convergeMaxConcurrent = [Math]::Max(1, [int]$convergeProfile.max_concurrent_plans)
    $convergeMinPause = [Math]::Max([int]$convergeProfile.min_dispatch_pause_seconds, [int]$script:StateReplicaFailoverCooldownSec)
    $convergeBlockSnapshotRed = [bool]$convergeProfile.block_on_snapshot_red
    $convergeBlockReplayRed = [bool]$convergeProfile.block_on_replay_red
    $roleText = [string]$Role
    $routing = Resolve-RolloutDecisionRouting -RoleValue $roleText -Blocked ([bool]$DispatchBlocked
    )
    $alertLevel = [string]$routing.alert_level
    $alertChannel = [string]$routing.alert_channel
    $alertTarget = [string]$routing.alert_target
    $deliveryType = [string]$routing.delivery_type
    $deliveryAction = [string]$routing.delivery_action
    $fp = [string]::Join("|", @(
            $src,
            $worstSite,
            $worstLevel,
            [string][Math]::Round($worstScore, 4),
            $policyScope,
            $policyRuleSource,
            [string]$policySitePriority,
            [string]$policyMinSitePriority,
            $policyPriorityGate,
            [string]$policyCap,
            [string]$policyPause,
            [string]$policyBlock,
            $roleText,
            $alertLevel,
            $alertChannel,
            $alertTarget,
            $deliveryType,
            $deliveryAction,
            [string]([Math]::Max(1, [int]$EffectiveConcurrent)),
            [string]([Math]::Max(0, [int]$EffectivePauseSeconds)),
            [string]([bool]$DispatchBlocked),
            $riskActiveProfile,
            [string]$convergeScope,
            [string]$convergeConfiguredEnabled,
            [string]$convergeEffectiveEnabled,
            [string]$convergeMaxConcurrent,
            [string]$convergeMinPause,
            [string]$convergeBlockSnapshotRed,
            [string]$convergeBlockReplayRed
        ))
    if ($script:RolloutDecisionSummaryFingerprintMap.ContainsKey($src) -and [string]$script:RolloutDecisionSummaryFingerprintMap[$src] -eq $fp) {
        return
    }
    $script:RolloutDecisionSummaryFingerprintMap[$src] = $fp
    Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
            timestamp_utc = [DateTime]::UtcNow.ToString("o")
            control_operation_id = $ControlOpId
            controller_id = $ControllerId
            queue_file = $QueuePath
            action = $PlanAction
            result = "rollout_decision_summary"
            source = $src
            worst_site_id = $worstSite
            worst_level = $worstLevel
            worst_score = [Math]::Round($worstScore, 4)
            risk_policy_scope = $policyScope
            risk_policy_rule_source = $policyRuleSource
            risk_policy_site_priority = $policySitePriority
            risk_policy_min_site_priority = $policyMinSitePriority
            risk_policy_priority_gate = $policyPriorityGate
            risk_policy_cap_concurrent = $policyCap
            risk_policy_pause_seconds = $policyPause
            risk_policy_block_dispatch = $policyBlock
            controller_role = $roleText
            decision_alert_level = $alertLevel
            decision_alert_channel = $alertChannel
            decision_alert_target = $alertTarget
            decision_delivery_type = $deliveryType
            decision_delivery_action = $deliveryAction
            effective_max_concurrent = [Math]::Max(1, [int]$EffectiveConcurrent)
            effective_pause_seconds = [Math]::Max(0, [int]$EffectivePauseSeconds)
            dispatch_blocked = [bool]$DispatchBlocked
            risk_policy_active_profile = $riskActiveProfile
            state_recovery_enabled = [bool]$script:StateRecoveryEnabled
            failover_mode = [bool]$script:StateReplicaFailoverMode
            failover_converge_scope = $convergeScope
            failover_converge_config_enabled = [bool]$convergeConfiguredEnabled
            failover_converge_enabled = [bool]$convergeEffectiveEnabled
            failover_converge_max_concurrent = [int]$convergeMaxConcurrent
            failover_converge_min_dispatch_pause_seconds = [int]$convergeMinPause
            failover_converge_block_on_snapshot_red = [bool]$convergeBlockSnapshotRed
            failover_converge_block_on_replay_red = [bool]$convergeBlockReplayRed
            error = ""
        })
    Send-RolloutDecisionDelivery -DeliveryTypeValue $deliveryType -DeliveryActionValue $deliveryAction -AlertLevelValue $alertLevel -AlertChannelValue $alertChannel -AlertTargetValue $alertTarget
}

function Select-RiskActionMatrix {
    param([string]$SiteId)
    $site = ""
    if ($null -ne $SiteId) {
        $site = ([string]$SiteId).Trim()
    }
    $hasRiskMatrixSelectBinary = (-not [string]::IsNullOrWhiteSpace([string]$script:RiskMatrixSelectBinaryPath)) -and (Test-Path -LiteralPath $script:RiskMatrixSelectBinaryPath)
    if ($hasRiskMatrixSelectBinary) {
        try {
            $siteRegionMapJson = "{}"
            if ($null -ne $script:SiteRegionMap -and $script:SiteRegionMap.Count -gt 0) {
                $siteRegionMapJson = ($script:SiteRegionMap | ConvertTo-Json -Depth 20 -Compress)
            }
            $siteMatrixJson = "{}"
            if ($null -ne $script:SiteConflictRiskActionMatrixBySite -and $script:SiteConflictRiskActionMatrixBySite.Count -gt 0) {
                $siteMatrixJson = ($script:SiteConflictRiskActionMatrixBySite | ConvertTo-Json -Depth 20 -Compress)
            }
            $regionMatrixJson = "{}"
            if ($null -ne $script:SiteConflictRiskActionMatrixByRegion -and $script:SiteConflictRiskActionMatrixByRegion.Count -gt 0) {
                $regionMatrixJson = ($script:SiteConflictRiskActionMatrixByRegion | ConvertTo-Json -Depth 20 -Compress)
            }
            $globalMatrixJson = "[]"
            if ($null -ne $script:SiteConflictRiskActionMatrixGlobal) {
                $globalMatrixJson = (@($script:SiteConflictRiskActionMatrixGlobal) | ConvertTo-Json -Depth 20 -Compress)
            }
            $rustArgs = @(
                "--site-id", $site,
                "--site-region-map-json", $siteRegionMapJson,
                "--site-matrix-json", $siteMatrixJson,
                "--region-matrix-json", $regionMatrixJson,
                "--global-matrix-json", $globalMatrixJson
            )
            $rustOutput = Invoke-RolloutPolicyTool -BinaryPath $script:RiskMatrixSelectBinaryPath -ToolName "risk-matrix-select" -Args $rustArgs -CaptureOutput
            if ($LASTEXITCODE -eq 0) {
                $rustText = (($rustOutput | Out-String).Trim())
                if (-not [string]::IsNullOrWhiteSpace($rustText)) {
                    $rustLine = ($rustText -split "`r?`n" | Select-Object -Last 1)
                    $rustObj = $rustLine | ConvertFrom-Json
                    if ($null -ne $rustObj -and $null -ne $rustObj.matrix) {
                        return [pscustomobject]@{
                            scope = [string]$rustObj.scope
                            matrix = @($rustObj.matrix)
                        }
                    }
                }
            }
        } catch {
        }
    }
    # emergency fallback only: do not keep layered site/region matrix selection
    # logic in PowerShell once unified Rust selection is unavailable.
    return [pscustomobject]@{
        scope = "global-emergency"
        matrix = $script:SiteConflictRiskActionMatrixGlobal
    }
}

function Get-SiteConflictRiskLevel {
    param([string]$Site)
    if (-not $script:SiteConflictRiskPredictorEnabled) {
        return "green"
    }
    if ($null -eq $script:SiteConflictRiskState -or $null -eq $script:SiteConflictRiskState.sites) {
        return "green"
    }
    if (-not $script:SiteConflictRiskState.sites.ContainsKey($Site)) {
        return "green"
    }
    $lvl = ([string]$script:SiteConflictRiskState.sites[$Site].level).ToLowerInvariant()
    if ($lvl -ne "green" -and $lvl -ne "yellow" -and $lvl -ne "orange" -and $lvl -ne "red") {
        return "green"
    }
    return $lvl
}

function Is-SiteConflictRiskBlocked {
    param(
        [string]$Site,
        [hashtable]$BlockedLevels
    )
    if ($null -eq $BlockedLevels -or $BlockedLevels.Count -eq 0) {
        return $false
    }
    $lvl = Get-SiteConflictRiskLevel -Site $Site
    return [bool]$BlockedLevels.ContainsKey($lvl)
}

function Copy-RiskLevelSet {
    param(
        [hashtable]$Source
    )
    $target = @{}
    if ($null -eq $Source) {
        return $target
    }
    foreach ($k in @($Source.Keys)) {
        $target[[string]$k] = $true
    }
    return $target
}

function Resolve-RiskBlockedSetMap {
    param(
        [object]$RawMap,
        [hashtable]$FallbackSet,
        [bool]$NormalizeRegion = $false
    )
    $map = @{}
    if ($null -eq $RawMap) {
        return $map
    }
    $hasRiskBlockedMapBuildBinary = (-not [string]::IsNullOrWhiteSpace([string]$script:RiskBlockedMapBuildBinaryPath)) -and (Test-Path -LiteralPath $script:RiskBlockedMapBuildBinaryPath)
    if ($hasRiskBlockedMapBuildBinary) {
        try {
            $rawMapJson = ($RawMap | ConvertTo-Json -Depth 20 -Compress)
            if ([string]::IsNullOrWhiteSpace($rawMapJson)) { $rawMapJson = "{}" }
            $fallbackSetJson = "{}"
            if ($null -ne $FallbackSet -and $FallbackSet.Count -gt 0) {
                $fallbackSetJson = ($FallbackSet | ConvertTo-Json -Depth 20 -Compress)
            }
            $rustArgs = @(
                "--raw-map-json", $rawMapJson,
                "--fallback-set-json", $fallbackSetJson,
                "--normalize-region", ([string][bool]$NormalizeRegion)
            )
            $rustOutput = Invoke-RolloutPolicyTool -BinaryPath $script:RiskBlockedMapBuildBinaryPath -ToolName "risk-blocked-map-build" -Args $rustArgs -CaptureOutput
            if ($LASTEXITCODE -eq 0) {
                $rustText = (($rustOutput | Out-String).Trim())
                if (-not [string]::IsNullOrWhiteSpace($rustText)) {
                    $rustLine = ($rustText -split "`r?`n" | Select-Object -Last 1)
                    $rustObj = $rustLine | ConvertFrom-Json
                    if ($null -ne $rustObj -and $null -ne $rustObj.map) {
                        foreach ($p in $rustObj.map.PSObject.Properties) {
                            $k = ([string]$p.Name).Trim()
                            if ([string]::IsNullOrWhiteSpace($k)) { continue }
                            $set = @{}
                            foreach ($lvl in @($p.Value)) {
                                $v = ([string]$lvl).Trim().ToLowerInvariant()
                                if ($v -eq "green" -or $v -eq "yellow" -or $v -eq "orange" -or $v -eq "red") {
                                    $set[$v] = $true
                                }
                            }
                            if ($set.Count -eq 0) {
                                $set = Copy-RiskLevelSet -Source $FallbackSet
                            }
                            $map[$k] = $set
                        }
                        return $map
                    }
                }
            }
        } catch {
        }
    }
    # emergency fallback only: do not rebuild site/region blocked override maps
    # in PowerShell. Keep empty map so selection falls back to the role default set.
    return $map
}

function Rebuild-RiskPolicyDerivedState {
    $script:SiteConflictRiskWinnerBlockedRaw = if ($null -ne $script:RoleRiskWinnerBlockedRaw) { $script:RoleRiskWinnerBlockedRaw } else { $script:UnifiedRiskBlockedRaw }
    $script:StateReplicaFailoverRiskBlockedRaw = if ($null -ne $script:RoleRiskFailoverBlockedRaw) { $script:RoleRiskFailoverBlockedRaw } else { $script:UnifiedRiskBlockedRaw }
    $script:UnifiedRiskBlockedSet = Parse-RiskLevelSet -Raw $script:UnifiedRiskBlockedRaw
    if ($script:UnifiedRiskBlockedSet.Count -eq 0) { $script:UnifiedRiskBlockedSet["red"] = $true }
    $script:SiteConflictRiskWinnerBlockedSet = Parse-RiskLevelSet -Raw $script:SiteConflictRiskWinnerBlockedRaw
    if ($script:SiteConflictRiskWinnerBlockedSet.Count -eq 0) { $script:SiteConflictRiskWinnerBlockedSet = Copy-RiskLevelSet -Source $script:UnifiedRiskBlockedSet }
    $script:StateReplicaFailoverRiskBlockedSet = Parse-RiskLevelSet -Raw $script:StateReplicaFailoverRiskBlockedRaw
    if ($script:StateReplicaFailoverRiskBlockedSet.Count -eq 0) { $script:StateReplicaFailoverRiskBlockedSet = Copy-RiskLevelSet -Source $script:UnifiedRiskBlockedSet }
    $script:SiteConflictRiskActionMatrixGlobal = Resolve-RiskActionMatrix -RawRules $script:RiskActionMatrixRaw -YellowCap $script:SiteConflictRiskYellowMaxConcurrent -YellowPause $script:SiteConflictRiskYellowPauseSeconds -OrangeCap $script:SiteConflictRiskOrangeMaxConcurrent -OrangePause $script:SiteConflictRiskOrangePauseSeconds -RedBlock ([bool]$script:SiteConflictRiskRedBlock)
    $script:SiteConflictRiskActionMatrix = $script:SiteConflictRiskActionMatrixGlobal
    $script:SiteRegionMap = @{}
    if ($null -ne $script:RiskSiteRegionMapRaw) {
        foreach ($p in $script:RiskSiteRegionMapRaw.PSObject.Properties) {
            $siteK = ([string]$p.Name).Trim()
            $regionV = ([string]$p.Value).Trim().ToUpperInvariant()
            if (-not [string]::IsNullOrWhiteSpace($siteK) -and -not [string]::IsNullOrWhiteSpace($regionV)) {
                $script:SiteRegionMap[$siteK] = $regionV
            }
        }
    }
    $script:SiteConflictRiskActionMatrixBySite = @{}
    if ($null -ne $script:RiskActionMatrixSiteOverrideRaw) {
        foreach ($p in $script:RiskActionMatrixSiteOverrideRaw.PSObject.Properties) {
            $siteK = ([string]$p.Name).Trim()
            if ([string]::IsNullOrWhiteSpace($siteK)) { continue }
            $script:SiteConflictRiskActionMatrixBySite[$siteK] = Resolve-RiskActionMatrix -RawRules @($p.Value) -YellowCap $script:SiteConflictRiskYellowMaxConcurrent -YellowPause $script:SiteConflictRiskYellowPauseSeconds -OrangeCap $script:SiteConflictRiskOrangeMaxConcurrent -OrangePause $script:SiteConflictRiskOrangePauseSeconds -RedBlock ([bool]$script:SiteConflictRiskRedBlock)
        }
    }
    $script:SiteConflictRiskActionMatrixByRegion = @{}
    if ($null -ne $script:RiskActionMatrixRegionOverrideRaw) {
        foreach ($p in $script:RiskActionMatrixRegionOverrideRaw.PSObject.Properties) {
            $regionK = ([string]$p.Name).Trim().ToUpperInvariant()
            if ([string]::IsNullOrWhiteSpace($regionK)) { continue }
            $script:SiteConflictRiskActionMatrixByRegion[$regionK] = Resolve-RiskActionMatrix -RawRules @($p.Value) -YellowCap $script:SiteConflictRiskYellowMaxConcurrent -YellowPause $script:SiteConflictRiskYellowPauseSeconds -OrangeCap $script:SiteConflictRiskOrangeMaxConcurrent -OrangePause $script:SiteConflictRiskOrangePauseSeconds -RedBlock ([bool]$script:SiteConflictRiskRedBlock)
        }
    }
    $script:SiteConflictRiskWinnerBlockedSetBySite = Resolve-RiskBlockedSetMap -RawMap $script:RiskWinnerBlockedBySiteRaw -FallbackSet $script:SiteConflictRiskWinnerBlockedSet -NormalizeRegion:$false
    $script:SiteConflictRiskWinnerBlockedSetByRegion = Resolve-RiskBlockedSetMap -RawMap $script:RiskWinnerBlockedByRegionRaw -FallbackSet $script:SiteConflictRiskWinnerBlockedSet -NormalizeRegion:$true
    $script:StateReplicaFailoverRiskBlockedSetBySite = Resolve-RiskBlockedSetMap -RawMap $script:RiskFailoverBlockedBySiteRaw -FallbackSet $script:StateReplicaFailoverRiskBlockedSet -NormalizeRegion:$false
    $script:StateReplicaFailoverRiskBlockedSetByRegion = Resolve-RiskBlockedSetMap -RawMap $script:RiskFailoverBlockedByRegionRaw -FallbackSet $script:StateReplicaFailoverRiskBlockedSet -NormalizeRegion:$true
}

function Apply-FailoverConvergeConfigFromStateRecovery {
    param(
        [object]$StateRecovery,
        [object]$RiskPolicy = $null
    )
    $script:StateReplicaFailoverConvergeEnabled = $true
    $script:StateReplicaFailoverConvergeMaxConcurrent = 1
    $script:StateReplicaFailoverConvergeMinPauseSeconds = [Math]::Max(1, [int]$script:StateReplicaFailoverCooldownSec)
    $script:StateReplicaFailoverConvergeBlockOnSnapshotRed = $true
    $script:StateReplicaFailoverConvergeBlockOnReplayRed = $true
    $script:FailoverConvergeBySiteRaw = @{}
    $script:FailoverConvergeByRegionRaw = @{}

    if ($null -ne $StateRecovery) {
        $cfg = Get-ConfigPropertyValue -Object $StateRecovery -Name "failover_converge"
        if ($null -ne $cfg) {
            if ($null -ne $cfg.enabled) { $script:StateReplicaFailoverConvergeEnabled = [bool]$cfg.enabled }
            if ($null -ne $cfg.max_concurrent_plans) { $script:StateReplicaFailoverConvergeMaxConcurrent = [Math]::Max(1, [int]$cfg.max_concurrent_plans) }
            if ($null -ne $cfg.min_dispatch_pause_seconds) { $script:StateReplicaFailoverConvergeMinPauseSeconds = [Math]::Max(0, [int]$cfg.min_dispatch_pause_seconds) }
            if ($null -ne $cfg.block_on_snapshot_red) { $script:StateReplicaFailoverConvergeBlockOnSnapshotRed = [bool]$cfg.block_on_snapshot_red }
            if ($null -ne $cfg.block_on_replay_red) { $script:StateReplicaFailoverConvergeBlockOnReplayRed = [bool]$cfg.block_on_replay_red }
            if ($null -ne $cfg.site_overrides) { $script:FailoverConvergeBySiteRaw = $cfg.site_overrides }
            if ($null -ne $cfg.region_overrides) { $script:FailoverConvergeByRegionRaw = $cfg.region_overrides }
        }
    }

    if ($null -ne $RiskPolicy) {
        $selection = Resolve-RiskPolicyProfileSelection -RiskPolicy $RiskPolicy
        $profile = $selection.profile
        $profileCfg = Get-RiskPolicyFieldValue -RiskPolicy $RiskPolicy -Profile $profile -FieldName "failover_converge"
        if ($null -ne $profileCfg) {
            if ($null -ne $profileCfg.enabled) { $script:StateReplicaFailoverConvergeEnabled = [bool]$profileCfg.enabled }
            if ($null -ne $profileCfg.max_concurrent_plans) { $script:StateReplicaFailoverConvergeMaxConcurrent = [Math]::Max(1, [int]$profileCfg.max_concurrent_plans) }
            if ($null -ne $profileCfg.min_dispatch_pause_seconds) { $script:StateReplicaFailoverConvergeMinPauseSeconds = [Math]::Max(0, [int]$profileCfg.min_dispatch_pause_seconds) }
            if ($null -ne $profileCfg.block_on_snapshot_red) { $script:StateReplicaFailoverConvergeBlockOnSnapshotRed = [bool]$profileCfg.block_on_snapshot_red }
            if ($null -ne $profileCfg.block_on_replay_red) { $script:StateReplicaFailoverConvergeBlockOnReplayRed = [bool]$profileCfg.block_on_replay_red }
            if ($null -ne $profileCfg.site_overrides) { $script:FailoverConvergeBySiteRaw = $profileCfg.site_overrides }
            if ($null -ne $profileCfg.region_overrides) { $script:FailoverConvergeByRegionRaw = $profileCfg.region_overrides }
        }
    }
}

function Rebuild-FailoverConvergeDerivedState {
    if ($script:StateReplicaFailoverConvergeMaxConcurrent -lt 1) { $script:StateReplicaFailoverConvergeMaxConcurrent = 1 }
    if ($script:StateReplicaFailoverConvergeMinPauseSeconds -lt 0) { $script:StateReplicaFailoverConvergeMinPauseSeconds = 0 }
    if ($script:StateReplicaFailoverConvergeMinPauseSeconds -lt [int]$script:StateReplicaFailoverCooldownSec) { $script:StateReplicaFailoverConvergeMinPauseSeconds = [int]$script:StateReplicaFailoverCooldownSec }
    if ($null -eq $script:FailoverConvergeBySiteRaw) { $script:FailoverConvergeBySiteRaw = @{} }
    if ($null -eq $script:FailoverConvergeByRegionRaw) { $script:FailoverConvergeByRegionRaw = @{} }
    $script:StateReplicaFailoverConvergeProfileGlobal = [pscustomobject]@{
        enabled = [bool]$script:StateReplicaFailoverConvergeEnabled
        max_concurrent_plans = [int]$script:StateReplicaFailoverConvergeMaxConcurrent
        min_dispatch_pause_seconds = [int]$script:StateReplicaFailoverConvergeMinPauseSeconds
        block_on_snapshot_red = [bool]$script:StateReplicaFailoverConvergeBlockOnSnapshotRed
        block_on_replay_red = [bool]$script:StateReplicaFailoverConvergeBlockOnReplayRed
    }
    $script:StateReplicaFailoverConvergeProfileBySite = Resolve-FailoverConvergeProfileMap -RawMap $script:FailoverConvergeBySiteRaw -FallbackProfile $script:StateReplicaFailoverConvergeProfileGlobal -NormalizeRegion:$false
    $script:StateReplicaFailoverConvergeProfileByRegion = Resolve-FailoverConvergeProfileMap -RawMap $script:FailoverConvergeByRegionRaw -FallbackProfile $script:StateReplicaFailoverConvergeProfileGlobal -NormalizeRegion:$true
    $script:StateReplicaConvergeLastSignature = ""
}

function Select-RiskBlockedSet {
    param(
        [string]$Role,
        [string]$SiteId
    )
    $roleNorm = ([string]$Role).Trim().ToLowerInvariant()
    $site = ""
    if ($null -ne $SiteId) {
        $site = ([string]$SiteId).Trim()
    }
    $defaultSet = $script:UnifiedRiskBlockedSet
    $siteMap = $null
    $regionMap = $null
    if ($roleNorm -eq "winner_guard") {
        $defaultSet = $script:SiteConflictRiskWinnerBlockedSet
        $siteMap = $script:SiteConflictRiskWinnerBlockedSetBySite
        $regionMap = $script:SiteConflictRiskWinnerBlockedSetByRegion
    } elseif ($roleNorm -eq "failover_risk_link") {
        $defaultSet = $script:StateReplicaFailoverRiskBlockedSet
        $siteMap = $script:StateReplicaFailoverRiskBlockedSetBySite
        $regionMap = $script:StateReplicaFailoverRiskBlockedSetByRegion
    }
    $hasRiskBlockedSelectBinary = (-not [string]::IsNullOrWhiteSpace([string]$script:RiskBlockedSelectBinaryPath)) -and (Test-Path -LiteralPath $script:RiskBlockedSelectBinaryPath)
    if ($hasRiskBlockedSelectBinary) {
        try {
            $defaultSetJson = "{}"
            if ($null -ne $defaultSet -and $defaultSet.Count -gt 0) {
                $defaultSetJson = ($defaultSet | ConvertTo-Json -Depth 20 -Compress)
            }
            $siteMapJson = "{}"
            if ($null -ne $siteMap -and $siteMap.Count -gt 0) {
                $siteMapJson = ($siteMap | ConvertTo-Json -Depth 20 -Compress)
            }
            $regionMapJson = "{}"
            if ($null -ne $regionMap -and $regionMap.Count -gt 0) {
                $regionMapJson = ($regionMap | ConvertTo-Json -Depth 20 -Compress)
            }
            $siteRegionMapJson = "{}"
            if ($null -ne $script:SiteRegionMap -and $script:SiteRegionMap.Count -gt 0) {
                $siteRegionMapJson = ($script:SiteRegionMap | ConvertTo-Json -Depth 20 -Compress)
            }
            $rustArgs = @(
                "--site-id", $site,
                "--default-set-json", $defaultSetJson,
                "--site-map-json", $siteMapJson,
                "--region-map-json", $regionMapJson,
                "--site-region-map-json", $siteRegionMapJson
            )
            $rustOutput = Invoke-RolloutPolicyTool -BinaryPath $script:RiskBlockedSelectBinaryPath -ToolName "risk-blocked-select" -Args $rustArgs -CaptureOutput
            if ($LASTEXITCODE -eq 0) {
                $rustText = (($rustOutput | Out-String).Trim())
                if (-not [string]::IsNullOrWhiteSpace($rustText)) {
                    $rustLine = ($rustText -split "`r?`n" | Select-Object -Last 1)
                    $rustObj = $rustLine | ConvertFrom-Json
                    if ($null -ne $rustObj) {
                        $set = @{}
                        if ($null -ne $rustObj.blocked_levels) {
                            foreach ($lvl in @($rustObj.blocked_levels)) {
                                $k = ([string]$lvl).Trim().ToLowerInvariant()
                                if ($k -eq "green" -or $k -eq "yellow" -or $k -eq "orange" -or $k -eq "red") {
                                    $set[$k] = $true
                                }
                            }
                        }
                        if ($set.Count -eq 0) {
                            $set = Copy-RiskLevelSet -Source $defaultSet
                        }
                        return [pscustomobject]@{
                            scope = [string]$rustObj.scope
                            set = $set
                        }
                    }
                }
            }
        } catch {
        }
    }
    # emergency fallback only: do not keep layered site/region blocked-set
    # selection logic in PowerShell when unified Rust selection is unavailable.
    return [pscustomobject]@{
        scope = "global-emergency"
        set = $defaultSet
    }
}

function Predict-SiteConflictRisk {
    if (-not $script:SiteConflictRiskPredictorEnabled) {
        return [pscustomobject]@{
            enabled = $false
            worst_site_id = ""
            worst_level = "green"
            worst_score = 0.0
            total_sites = 0
        }
    }
    $alpha = $script:SiteConflictRiskEmaAlpha
    if ($alpha -lt 0.01) { $alpha = 0.01 }
    if ($alpha -gt 1.0) { $alpha = 1.0 }
    $script:SiteConflictRiskState.ema_alpha = $alpha
    $maxPenaltySafe = [Math]::Max(1, [int]$script:SiteConflictMaxPenaltyPoints)
    $worstSite = ""
    $worstScore = -1.0
    $worstLevel = "green"
    foreach ($site in @($script:SiteConflictAccountabilityState.sites.Keys)) {
        $rec = $script:SiteConflictAccountabilityState.sites[$site]
        $penalty = [Math]::Max(0, [int]$rec.penalty_points)
        $repScore = [double]$rec.reputation_score
        if ($repScore -lt 0.0) { $repScore = 0.0 }
        if ($repScore -gt 100.0) { $repScore = 100.0 }
        $rawPenaltyRisk = ([double]$penalty / [double]$maxPenaltySafe) * 100.0
        $rawReputationRisk = 100.0 - $repScore
        $raw = (0.7 * $rawPenaltyRisk) + (0.3 * $rawReputationRisk)
        if ($raw -lt 0.0) { $raw = 0.0 }
        if ($raw -gt 100.0) { $raw = 100.0 }
        if (-not $script:SiteConflictRiskState.sites.ContainsKey($site)) {
            $script:SiteConflictRiskState.sites[$site] = [pscustomobject]@{
                raw_score = $raw
                ema_score = $raw
                trend = 0.0
                level = "green"
                updated_unix_ms = [int64](Now-Ms)
            }
        }
        $r = $script:SiteConflictRiskState.sites[$site]
        $oldEma = [double]$r.ema_score
        $ema = ($alpha * $raw) + ((1.0 - $alpha) * $oldEma)
        if ($ema -lt 0.0) { $ema = 0.0 }
        if ($ema -gt 100.0) { $ema = 100.0 }
        $trend = $ema - $oldEma
        $level = Score-ToRiskLevel -Score $ema
        $r.raw_score = [Math]::Round($raw, 4)
        $r.ema_score = [Math]::Round($ema, 4)
        $r.trend = [Math]::Round($trend, 4)
        $r.level = $level
        $r.updated_unix_ms = [int64](Now-Ms)
        if ([double]$r.ema_score -gt $worstScore) {
            $worstScore = [double]$r.ema_score
            $worstSite = $site
            $worstLevel = $level
        }
    }
    if ($worstScore -lt 0.0) { $worstScore = 0.0 }
    $script:SiteConflictRiskState.last_predict_unix_ms = [int64](Now-Ms)
    $script:SiteConflictRiskState.summary.worst_site_id = $worstSite
    $script:SiteConflictRiskState.summary.worst_level = $worstLevel
    $script:SiteConflictRiskState.summary.worst_score = [Math]::Round($worstScore, 4)
    $script:SiteConflictRiskState.summary.total_sites = @($script:SiteConflictAccountabilityState.sites.Keys).Count
    Save-SiteConflictRiskState -Path $script:SiteConflictRiskStatePath -State $script:SiteConflictRiskState
    return [pscustomobject]@{
        enabled = $true
        worst_site_id = $worstSite
        worst_level = $worstLevel
        worst_score = [Math]::Round($worstScore, 4)
        total_sites = [int]$script:SiteConflictRiskState.summary.total_sites
    }
}

function New-StateRecoverySnapshot {
    return [pscustomobject]@{
        version = 1
        updated_at = ""
        queue_file = ""
        plan_action = ""
        controller_id = ""
        control_operation_id = ""
        status = "init"
        pending = @()
        running = @()
        done = [pscustomobject]@{
            ok = 0
            err = 0
            skip = 0
        }
    }
}

function Load-StateRecoverySnapshot {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path)) {
        return (New-StateRecoverySnapshot)
    }
    $raw = Get-Content -LiteralPath $Path -Raw
    if ([string]::IsNullOrWhiteSpace($raw)) {
        return (New-StateRecoverySnapshot)
    }
    try {
        $obj = $raw | ConvertFrom-Json -ErrorAction Stop
    } catch {
        return (New-StateRecoverySnapshot)
    }
    $snapshot = New-StateRecoverySnapshot
    if ($null -ne $obj.version) { $snapshot.version = [int]$obj.version }
    if ($null -ne $obj.updated_at) { $snapshot.updated_at = [string]$obj.updated_at }
    if ($null -ne $obj.queue_file) { $snapshot.queue_file = [string]$obj.queue_file }
    if ($null -ne $obj.plan_action) { $snapshot.plan_action = [string]$obj.plan_action }
    if ($null -ne $obj.controller_id) { $snapshot.controller_id = [string]$obj.controller_id }
    if ($null -ne $obj.control_operation_id) { $snapshot.control_operation_id = [string]$obj.control_operation_id }
    if ($null -ne $obj.status) { $snapshot.status = [string]$obj.status }
    if ($null -ne $obj.pending) { $snapshot.pending = @($obj.pending) }
    if ($null -ne $obj.running) { $snapshot.running = @($obj.running) }
    if ($null -ne $obj.done) {
        $snapshot.done.ok = [int]$obj.done.ok
        $snapshot.done.err = [int]$obj.done.err
        $snapshot.done.skip = [int]$obj.done.skip
    }
    return $snapshot
}

function Snapshot-Entry {
    param([object]$Entry)
    return [pscustomobject]@{
        name = [string]$Entry.name
        action = [string]$Entry.action
        attempt = [int]$Entry.attempt
        next_run = [int64]$Entry.next_run
        priority = [int]$Entry.priority
        region = [string]$Entry.region
        target = [string]$Entry.target
        rollback = [string]$Entry.rollback
        preempt_requeue = [int]$Entry.preempt_requeue
        retry_max = [int]$Entry.retry_max
        retry_backoff_sec = [int]$Entry.retry_backoff_sec
        retry_backoff_factor = [int]$Entry.retry_backoff_factor
        opid = [string]$Entry.opid
    }
}

function Write-ContentWithReplicas {
    param(
        [string]$PrimaryPath,
        [string[]]$ReplicaPaths,
        [string]$Content
    )
    Ensure-ParentDir -PathValue $PrimaryPath
    [System.IO.File]::WriteAllText($PrimaryPath, $Content, [System.Text.Encoding]::UTF8)
    foreach ($rp in @($ReplicaPaths)) {
        if ([string]::IsNullOrWhiteSpace([string]$rp)) {
            continue
        }
        Ensure-ParentDir -PathValue $rp
        [System.IO.File]::WriteAllText($rp, $Content, [System.Text.Encoding]::UTF8)
    }
}

function Write-LinesWithReplicas {
    param(
        [string]$PrimaryPath,
        [string[]]$ReplicaPaths,
        [string[]]$Lines
    )
    Ensure-ParentDir -PathValue $PrimaryPath
    [System.IO.File]::WriteAllLines($PrimaryPath, @($Lines), [System.Text.Encoding]::UTF8)
    foreach ($rp in @($ReplicaPaths)) {
        if ([string]::IsNullOrWhiteSpace([string]$rp)) {
            continue
        }
        Ensure-ParentDir -PathValue $rp
        [System.IO.File]::WriteAllLines($rp, @($Lines), [System.Text.Encoding]::UTF8)
    }
}

function Save-StateRecoverySnapshot {
    param(
        [string]$Path,
        [string]$QueuePath,
        [string]$PlanAction,
        [string]$ControllerId,
        [string]$ControlOpId,
        [string]$Status,
        [object[]]$Pending,
        [object[]]$Running,
        [int]$DoneOk,
        [int]$DoneErr,
        [int]$DoneSkip
    )
    Ensure-ParentDir -PathValue $Path
    $obj = New-StateRecoverySnapshot
    $obj.updated_at = [DateTime]::UtcNow.ToString("o")
    $obj.queue_file = $QueuePath
    $obj.plan_action = $PlanAction
    $obj.controller_id = $ControllerId
    $obj.control_operation_id = $ControlOpId
    $obj.status = $Status
    $obj.pending = @($Pending | ForEach-Object { Snapshot-Entry -Entry $_ })
    $obj.running = @($Running | ForEach-Object { Snapshot-Entry -Entry $_ })
    $obj.done.ok = [int]$DoneOk
    $obj.done.err = [int]$DoneErr
    $obj.done.skip = [int]$DoneSkip
    $json = $obj | ConvertTo-Json -Depth 12
    Write-ContentWithReplicas -PrimaryPath $Path -ReplicaPaths $script:StateSnapshotReplicaPaths -Content $json
}

function Build-EntryMapByName {
    param([object[]]$Entries)
    $map = @{}
    for ($i = 0; $i -lt $Entries.Count; $i++) {
        $n = [string]$Entries[$i].name
        if (-not [string]::IsNullOrWhiteSpace($n) -and -not $map.ContainsKey($n)) {
            $map[$n] = $i
        }
    }
    return $map
}

function Restore-PendingFromSnapshot {
    param(
        [object[]]$Pending,
        [object]$Snapshot,
        [string]$QueuePath,
        [string]$PlanAction,
        [bool]$ResumeEnabled
    )
    if (-not $ResumeEnabled) {
        return [pscustomobject]@{
            pending = @($Pending)
            recovered_pending = 0
            recovered_running = 0
            skipped = "resume disabled"
        }
    }
    if ($null -eq $Snapshot) {
        return [pscustomobject]@{
            pending = @($Pending)
            recovered_pending = 0
            recovered_running = 0
            skipped = "empty snapshot"
        }
    }
    if ([string]$Snapshot.queue_file -ne $QueuePath -or [string]$Snapshot.plan_action -ne $PlanAction) {
        return [pscustomobject]@{
            pending = @($Pending)
            recovered_pending = 0
            recovered_running = 0
            skipped = "snapshot mismatch"
        }
    }
    $result = @($Pending)
    $map = Build-EntryMapByName -Entries $result
    $rp = 0
    $rr = 0
    foreach ($item in @($Snapshot.pending)) {
        $name = [string]$item.name
        if ([string]::IsNullOrWhiteSpace($name) -or -not $map.ContainsKey($name)) { continue }
        $idx = [int]$map[$name]
        $entry = $result[$idx]
        $entry.attempt = [Math]::Max([int]$entry.attempt, [Math]::Max(1, [int]$item.attempt))
        $nextRun = [int64]$item.next_run
        if ($nextRun -gt 0) { $entry.next_run = $nextRun }
        $rp += 1
    }
    foreach ($item in @($Snapshot.running)) {
        $name = [string]$item.name
        if ([string]::IsNullOrWhiteSpace($name) -or -not $map.ContainsKey($name)) { continue }
        $idx = [int]$map[$name]
        $entry = $result[$idx]
        $entry.attempt = [Math]::Max([int]$entry.attempt, [Math]::Max(1, [int]$item.attempt) + 1)
        $entry.next_run = (Now-Ms) + ([int64][Math]::Max(1, [int]$entry.retry_backoff_sec) * 1000)
        $rr += 1
    }
    return [pscustomobject]@{
        pending = $result
        recovered_pending = $rp
        recovered_running = $rr
        skipped = ""
    }
}

function Load-ReplayEvents {
    param(
        [string]$Path,
        [int]$TailMax
    )
    if (-not (Test-Path -LiteralPath $Path)) {
        return @()
    }
    $lines = @()
    if ($TailMax -gt 0) {
        $lines = @(Get-Content -LiteralPath $Path -Tail $TailMax)
    } else {
        $lines = @(Get-Content -LiteralPath $Path)
    }
    $events = @()
    foreach ($line in $lines) {
        if ([string]::IsNullOrWhiteSpace($line)) { continue }
        try {
            $events += ($line | ConvertFrom-Json -ErrorAction Stop)
        } catch {
            continue
        }
    }
    return $events
}

function Apply-ConflictReplay {
    param(
        [object[]]$Pending,
        [object[]]$Events,
        [string]$QueuePath,
        [string]$PlanAction,
        [bool]$Enabled
    )
    if (-not $Enabled) {
        return [pscustomobject]@{
            pending = @($Pending)
            replayed = 0
        }
    }
    if ($null -eq $Events -or $Events.Count -eq 0) {
        return [pscustomobject]@{
            pending = @($Pending)
            replayed = 0
        }
    }
    $result = @($Pending)
    $map = Build-EntryMapByName -Entries $result
    $count = 0
    foreach ($evt in $Events) {
        $queueMatch = ([string]$evt.queue_file -eq $QueuePath)
        if (-not $queueMatch) { continue }
        if (-not [string]::IsNullOrWhiteSpace([string]$evt.action) -and [string]$evt.action -ne $PlanAction) { continue }
        $res = [string]$evt.result
        if ($res -ne "dedupe_blocked" -and $res -ne "consensus_blocked" -and $res -ne "consensus_wait") { continue }
        $name = [string]$evt.plan
        if ([string]::IsNullOrWhiteSpace($name) -or -not $map.ContainsKey($name)) { continue }
        $idx = [int]$map[$name]
        $entry = $result[$idx]
        $entry.attempt = [Math]::Max([int]$entry.attempt, ([int]$evt.attempt + 1))
        $entry.next_run = [int64](Now-Ms)
        $count += 1
    }
    return [pscustomobject]@{
        pending = $result
        replayed = $count
    }
}

function Write-ReplayEvent {
    param(
        [string]$Path,
        [pscustomobject]$Obj
    )
    if (-not $script:StateRecoveryEnabled) {
        return
    }
    $line = $Obj | ConvertTo-Json -Compress -Depth 20
    Ensure-ParentDir -PathValue $Path
    Add-Content -LiteralPath $Path -Value $line -Encoding UTF8
    foreach ($rp in @($script:StateReplayReplicaPaths)) {
        if ([string]::IsNullOrWhiteSpace([string]$rp)) {
            continue
        }
        Ensure-ParentDir -PathValue $rp
        Add-Content -LiteralPath $rp -Value $line -Encoding UTF8
    }
}

function Trim-ReplayFile {
    param(
        [string]$Path,
        [int]$MaxEntries
    )
    if ($MaxEntries -le 0) {
        return
    }
    if (-not (Test-Path -LiteralPath $Path)) {
        Write-LinesWithReplicas -PrimaryPath $Path -ReplicaPaths $script:StateReplayReplicaPaths -Lines @()
        return
    }
    $tail = @(Get-Content -LiteralPath $Path -Tail $MaxEntries)
    Write-LinesWithReplicas -PrimaryPath $Path -ReplicaPaths $script:StateReplayReplicaPaths -Lines @($tail)
}

function Get-FileDigest {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path)) {
        return [pscustomobject]@{
            exists = $false
            hash = ""
            line_count = 0
        }
    }
    $hashObj = Get-FileHash -LiteralPath $Path -Algorithm SHA256
    $lineCount = 0
    try {
        $lineCount = @(Get-Content -LiteralPath $Path).Count
    } catch {
        $lineCount = 0
    }
    return [pscustomobject]@{
        exists = $true
        hash = [string]$hashObj.Hash
        line_count = [int]$lineCount
    }
}

function Validate-ReplicaSet {
    param(
        [string]$Kind,
        [string]$PrimaryPath,
        [string[]]$ReplicaPaths,
        [int]$AllowedLagEntries
    )
    $errors = @()
    $warnings = @()
    if ($null -eq $ReplicaPaths -or $ReplicaPaths.Count -eq 0) {
        return [pscustomobject]@{
            ok = $true
            errors = @()
            warnings = @()
        }
    }
    $primary = Get-FileDigest -Path $PrimaryPath
    if (-not $primary.exists) {
        return [pscustomobject]@{
            ok = $false
            errors = @("primary missing: " + $PrimaryPath)
            warnings = @()
        }
    }
    foreach ($rp in @($ReplicaPaths)) {
        if ([string]::IsNullOrWhiteSpace([string]$rp)) {
            continue
        }
        $rep = Get-FileDigest -Path $rp
        if (-not $rep.exists) {
            $errors += ("replica missing: " + $rp)
            continue
        }
        if ([string]$rep.hash -eq [string]$primary.hash) {
            continue
        }
        if ($Kind -eq "replay" -and $AllowedLagEntries -gt 0) {
            $lag = [Math]::Abs([int]$primary.line_count - [int]$rep.line_count)
            if ($lag -le $AllowedLagEntries) {
                $warnings += ("replica lag accepted: file=" + $rp + " lag_entries=" + $lag)
                continue
            }
        }
        $errors += ("replica mismatch: kind=" + $Kind + " file=" + $rp)
    }
    return [pscustomobject]@{
        ok = ($errors.Count -eq 0)
        errors = $errors
        warnings = $warnings
    }
}

function Validate-StateReplicas {
    param([int]$AllowedLagEntries)
    $snapshotCheck = Validate-ReplicaSet -Kind "snapshot" -PrimaryPath $script:StateSnapshotPath -ReplicaPaths $script:StateSnapshotReplicaPaths -AllowedLagEntries 0
    $replayCheck = Validate-ReplicaSet -Kind "replay" -PrimaryPath $script:StateReplayPath -ReplicaPaths $script:StateReplayReplicaPaths -AllowedLagEntries $AllowedLagEntries
    $errors = @($snapshotCheck.errors + $replayCheck.errors)
    $warnings = @($snapshotCheck.warnings + $replayCheck.warnings)
    $snapshotGrade = "green"
    if (-not $snapshotCheck.ok) {
        $snapshotGrade = "red"
    } elseif ($snapshotCheck.warnings.Count -gt 0) {
        $snapshotGrade = "yellow"
    }
    $replayGrade = "green"
    if (-not $replayCheck.ok) {
        $replayGrade = "red"
    } elseif ($replayCheck.warnings.Count -gt 0) {
        $replayGrade = "yellow"
    }
    $overall = "green"
    if ($errors.Count -gt 0) {
        $overall = "red"
    } elseif ($warnings.Count -gt 0) {
        $overall = "yellow"
    }
    return [pscustomobject]@{
        ok = ($errors.Count -eq 0)
        errors = $errors
        warnings = $warnings
        overall_grade = $overall
        snapshot_grade = $snapshotGrade
        replay_grade = $replayGrade
        snapshot = $snapshotCheck
        replay = $replayCheck
    }
}

function Save-ReplicaHealthState {
    param(
        [string]$Path,
        [string]$QueuePath,
        [string]$PlanAction,
        [string]$ControllerId,
        [string]$ControlOpId,
        [string]$Source,
        [object]$Validation,
        [bool]$FailoverTriggered,
        [string[]]$FailoverDetails
    )
    if (-not $script:StateRecoveryEnabled) {
        return
    }
    $obj = [pscustomobject]@{
        version = 1
        updated_at = [DateTime]::UtcNow.ToString("o")
        queue_file = $QueuePath
        plan_action = $PlanAction
        controller_id = $ControllerId
        control_operation_id = $ControlOpId
        source = $Source
        ok = [bool]$Validation.ok
        overall_grade = [string]$Validation.overall_grade
        snapshot_grade = [string]$Validation.snapshot_grade
        replay_grade = [string]$Validation.replay_grade
        error_count = @($Validation.errors).Count
        warning_count = @($Validation.warnings).Count
        errors = @($Validation.errors)
        warnings = @($Validation.warnings)
        failover = [pscustomobject]@{
            enabled = [bool]$script:StateReplicaAutoFailoverEnabled
            triggered = $FailoverTriggered
            details = @($FailoverDetails)
            count = [int]$script:StateReplicaFailoverCount
            last_unix_ms = [int64]$script:StateReplicaLastFailoverMs
            policy = [pscustomobject]@{
                enabled = [bool]$script:StateReplicaFailoverPolicyEnabled
                default_allow = [bool]$script:StateReplicaFailoverPolicyDefaultAllow
                matrix_size = [int]$script:StateReplicaFailoverPolicyMatrix.Count
                matrix_region_override_count = [int]$script:StateReplicaFailoverPolicyMatrixByRegion.Count
                matrix_site_override_count = [int]$script:StateReplicaFailoverPolicyMatrixBySite.Count
                last_rule = [string]$script:StateReplicaFailoverPolicyLastRule
                last_scope = [string]$script:StateReplicaFailoverPolicyLastScope
                last_allowed = [bool]$script:StateReplicaFailoverPolicyLastAllowed
                last_reason = [string]$script:StateReplicaFailoverPolicyLastReason
                last_cooldown_seconds = [int]$script:StateReplicaFailoverPolicyLastCooldownSec
                last_slo_gate = [string]$script:StateReplicaFailoverPolicyLastSloGate
                last_drill_gate = [string]$script:StateReplicaFailoverPolicyLastDrillGate
                last_risk_gate = [string]$script:StateReplicaFailoverPolicyLastRiskGate
                slo_link = [pscustomobject]@{
                    enabled = [bool]$script:StateReplicaFailoverSloLinkEnabled
                    min_effective_score = [double]$script:StateReplicaFailoverSloMinEffectiveScore
                    block_on_violation = [bool]$script:StateReplicaFailoverSloBlockOnViolation
                    region_override_count = [int]$script:StateReplicaFailoverSloLinkProfileByRegion.Count
                    site_override_count = [int]$script:StateReplicaFailoverSloLinkProfileBySite.Count
                }
                drill_link = [pscustomobject]@{
                    enabled = [bool]$script:StateReplicaFailoverDrillLinkEnabled
                    min_pass_rate = [double]$script:StateReplicaFailoverDrillMinPassRate
                    min_average_score = [double]$script:StateReplicaFailoverDrillMinAverageScore
                    require_last_pass = [bool]$script:StateReplicaFailoverDrillRequireLastPass
                    region_override_count = [int]$script:StateReplicaFailoverDrillLinkProfileByRegion.Count
                    site_override_count = [int]$script:StateReplicaFailoverDrillLinkProfileBySite.Count
                }
                risk_link = [pscustomobject]@{
                    enabled = [bool]$script:StateReplicaFailoverRiskLinkEnabled
                    blocked_levels = @($script:StateReplicaFailoverRiskBlockedSet.Keys | Sort-Object)
                    region_override_count = [int]$script:StateReplicaFailoverRiskLinkProfileByRegion.Count
                    site_override_count = [int]$script:StateReplicaFailoverRiskLinkProfileBySite.Count
                }
            }
        }
        mode = [pscustomobject]@{
            failover_mode = [bool]$script:StateReplicaFailoverMode
            stable_cycles = [int]$script:StateReplicaStableCycles
            switchback_enabled = [bool]$script:StateReplicaSwitchbackEnabled
            switchback_stable_cycles = [int]$script:StateReplicaSwitchbackStableCycles
        }
        drill = [pscustomobject]@{
            enabled = [bool]$script:StateReplicaDrillEnabled
            drill_id = [string]$script:StateReplicaDrillId
            score = [pscustomobject]@{
                enabled = [bool]$script:StateReplicaDrillScoreEnabled
                file = [string]$script:StateReplicaDrillScorePath
                window_samples = [int]$script:StateReplicaDrillScoreWindowSamples
                pass_score = [double]$script:StateReplicaDrillPassScore
                total = [int]$script:StateReplicaDrillScoreState.summary.total
                pass_count = [int]$script:StateReplicaDrillScoreState.summary.pass_count
                pass_rate = [double]$script:StateReplicaDrillScoreState.summary.pass_rate
                average_score = [double]$script:StateReplicaDrillScoreState.summary.average_score
                last_score = [double]$script:StateReplicaDrillScoreState.summary.last_score
                last_grade = [string]$script:StateReplicaDrillScoreState.summary.last_grade
            }
        }
        slo = [pscustomobject]@{
            enabled = [bool]$script:StateReplicaSloEnabled
            block_on_violation = [bool]$script:StateReplicaSloBlockOnViolation
            window_samples = [int]$script:StateReplicaSloWindowSamples
            min_green_rate = [double]$script:StateReplicaSloMinGreenRate
            max_red_in_window = [int]$script:StateReplicaSloMaxRedInWindow
            adaptive = [pscustomobject]@{
                enabled = [bool]$script:StateReplicaAdaptiveEnabled
                file = [string]$script:StateReplicaAdaptivePath
                step = [double]$script:StateReplicaAdaptiveStep
                good_score = [double]$script:StateReplicaAdaptiveGoodScore
                bad_score = [double]$script:StateReplicaAdaptiveBadScore
                max_shift = [double]$script:StateReplicaAdaptiveMaxShift
                bias = [double]$script:StateReplicaAdaptiveState.bias
                last_action = [string]$script:StateReplicaAdaptiveState.last_action
                last_reason = [string]$script:StateReplicaAdaptiveState.last_reason
                sample_count = [int]$script:StateReplicaAdaptiveState.sample_count
            }
            circuit_breaker = [pscustomobject]@{
                enabled = [bool]$script:StateReplicaCircuitEnabled
                current_grade = [string]$script:StateReplicaCircuitCurrentGrade
                current_rule = [string]$script:StateReplicaCircuitCurrentRule
                current_concurrent_cap = [int]$script:StateReplicaCircuitCurrentConcurrentCap
                current_pause_seconds = [int]$script:StateReplicaCircuitCurrentPauseSeconds
                current_block_dispatch = [bool]$script:StateReplicaCircuitCurrentBlockDispatch
                yellow_max_concurrent = [int]$script:StateReplicaCircuitYellowMaxConcurrent
                yellow_pause_seconds = [int]$script:StateReplicaCircuitYellowPauseSeconds
                red_block = [bool]$script:StateReplicaCircuitRedBlock
                matrix_size = [int]$script:StateReplicaCircuitMatrix.Count
            }
            last = [pscustomobject]@{
                score = [double]$script:StateReplicaSloState.last.score
                green_rate = [double]$script:StateReplicaSloState.last.green_rate
                total = [int]$script:StateReplicaSloState.last.total
                green = [int]$script:StateReplicaSloState.last.green
                yellow = [int]$script:StateReplicaSloState.last.yellow
                red = [int]$script:StateReplicaSloState.last.red
                violation = [bool]$script:StateReplicaSloState.last.violation
                reason = [string]$script:StateReplicaSloState.last.reason
            }
        }
    }
    Ensure-ParentDir -PathValue $Path
    $json = $obj | ConvertTo-Json -Depth 14
    [System.IO.File]::WriteAllText($Path, $json, [System.Text.Encoding]::UTF8)
}

function New-ReplicaSloState {
    return [pscustomobject]@{
        version = 1
        updated_at = ""
        window_samples = 0
        min_green_rate = 0.0
        max_red_in_window = 0
        samples = @()
        last = [pscustomobject]@{
            total = 0
            green = 0
            yellow = 0
            red = 0
            green_rate = 1.0
            score = 100.0
            violation = $false
            reason = ""
        }
    }
}

function Load-ReplicaSloState {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path)) {
        return (New-ReplicaSloState)
    }
    $raw = Get-Content -LiteralPath $Path -Raw
    if ([string]::IsNullOrWhiteSpace($raw)) {
        return (New-ReplicaSloState)
    }
    try {
        $obj = $raw | ConvertFrom-Json -ErrorAction Stop
    } catch {
        return (New-ReplicaSloState)
    }
    $state = New-ReplicaSloState
    if ($null -ne $obj.version) { $state.version = [int]$obj.version }
    if ($null -ne $obj.updated_at) { $state.updated_at = [string]$obj.updated_at }
    if ($null -ne $obj.window_samples) { $state.window_samples = [int]$obj.window_samples }
    if ($null -ne $obj.min_green_rate) { $state.min_green_rate = [double]$obj.min_green_rate }
    if ($null -ne $obj.max_red_in_window) { $state.max_red_in_window = [int]$obj.max_red_in_window }
    if ($null -ne $obj.samples) { $state.samples = @($obj.samples) }
    if ($null -ne $obj.last) {
        $state.last.total = [int]$obj.last.total
        $state.last.green = [int]$obj.last.green
        $state.last.yellow = [int]$obj.last.yellow
        $state.last.red = [int]$obj.last.red
        $state.last.green_rate = [double]$obj.last.green_rate
        $state.last.score = [double]$obj.last.score
        $state.last.violation = [bool]$obj.last.violation
        $state.last.reason = [string]$obj.last.reason
    }
    return $state
}

function Save-ReplicaSloState {
    param(
        [string]$Path,
        [pscustomobject]$State
    )
    Ensure-ParentDir -PathValue $Path
    $State.updated_at = [DateTime]::UtcNow.ToString("o")
    $json = $State | ConvertTo-Json -Depth 14
    [System.IO.File]::WriteAllText($Path, $json, [System.Text.Encoding]::UTF8)
}

function New-ReplicaAdaptiveState {
    return [pscustomobject]@{
        version = 1
        updated_at = ""
        sample_count = 0
        bias = 0.0
        last_action = "init"
        last_reason = ""
    }
}

function Load-ReplicaAdaptiveState {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path)) {
        return (New-ReplicaAdaptiveState)
    }
    $raw = Get-Content -LiteralPath $Path -Raw
    if ([string]::IsNullOrWhiteSpace($raw)) {
        return (New-ReplicaAdaptiveState)
    }
    try {
        $obj = $raw | ConvertFrom-Json -ErrorAction Stop
    } catch {
        return (New-ReplicaAdaptiveState)
    }
    $state = New-ReplicaAdaptiveState
    if ($null -ne $obj.version) { $state.version = [int]$obj.version }
    if ($null -ne $obj.updated_at) { $state.updated_at = [string]$obj.updated_at }
    if ($null -ne $obj.sample_count) { $state.sample_count = [Math]::Max(0, [int]$obj.sample_count) }
    if ($null -ne $obj.bias) { $state.bias = [double]$obj.bias }
    if ($null -ne $obj.last_action) { $state.last_action = [string]$obj.last_action }
    if ($null -ne $obj.last_reason) { $state.last_reason = [string]$obj.last_reason }
    return $state
}

function Save-ReplicaAdaptiveState {
    param(
        [string]$Path,
        [pscustomobject]$State
    )
    Ensure-ParentDir -PathValue $Path
    $State.updated_at = [DateTime]::UtcNow.ToString("o")
    $json = $State | ConvertTo-Json -Depth 12
    [System.IO.File]::WriteAllText($Path, $json, [System.Text.Encoding]::UTF8)
}

function Update-ReplicaAdaptiveState {
    param(
        [pscustomobject]$State,
        [object]$Obs
    )
    $oldBias = [double]$State.bias
    $newBias = $oldBias
    $action = "hold"
    $reason = "score in neutral range"
    if ([bool]$Obs.violation -or [double]$Obs.score -le $script:StateReplicaAdaptiveBadScore) {
        $newBias = [Math]::Min($script:StateReplicaAdaptiveMaxShift, $oldBias + $script:StateReplicaAdaptiveStep)
        $action = "tighten"
        if ([bool]$Obs.violation) {
            $reason = "slo violation"
        } else {
            $reason = ("score <= bad_score (" + $Obs.score + " <= " + $script:StateReplicaAdaptiveBadScore + ")")
        }
    } elseif ((-not [bool]$Obs.violation) -and [double]$Obs.score -ge $script:StateReplicaAdaptiveGoodScore) {
        $newBias = [Math]::Max(-$script:StateReplicaAdaptiveMaxShift, $oldBias - $script:StateReplicaAdaptiveStep)
        $action = "relax"
        $reason = ("score >= good_score (" + $Obs.score + " >= " + $script:StateReplicaAdaptiveGoodScore + ")")
    }
    $State.sample_count = [int]$State.sample_count + 1
    $State.bias = [Math]::Round($newBias, 4)
    $State.last_action = $action
    $State.last_reason = $reason
    $effectiveScore = [double]$Obs.score - [double]$State.bias
    if ($effectiveScore -lt 0.0) { $effectiveScore = 0.0 }
    if ($effectiveScore -gt 100.0) { $effectiveScore = 100.0 }
    return [pscustomobject]@{
        action = $action
        reason = $reason
        bias = [double]$State.bias
        effective_score = [Math]::Round($effectiveScore, 4)
        raw_score = [double]$Obs.score
        sample_count = [int]$State.sample_count
    }
}

function New-ReplicaDrillScoreState {
    return [pscustomobject]@{
        version = 1
        updated_at = ""
        window_samples = 0
        pass_score = 70.0
        samples = @()
        summary = [pscustomobject]@{
            total = 0
            pass_count = 0
            pass_rate = 0.0
            average_score = 0.0
            last_score = 0.0
            last_grade = "red"
        }
    }
}

function Load-ReplicaDrillScoreState {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path)) {
        return (New-ReplicaDrillScoreState)
    }
    $raw = Get-Content -LiteralPath $Path -Raw
    if ([string]::IsNullOrWhiteSpace($raw)) {
        return (New-ReplicaDrillScoreState)
    }
    try {
        $obj = $raw | ConvertFrom-Json -ErrorAction Stop
    } catch {
        return (New-ReplicaDrillScoreState)
    }
    $state = New-ReplicaDrillScoreState
    if ($null -ne $obj.version) { $state.version = [int]$obj.version }
    if ($null -ne $obj.updated_at) { $state.updated_at = [string]$obj.updated_at }
    if ($null -ne $obj.window_samples) { $state.window_samples = [Math]::Max(1, [int]$obj.window_samples) }
    if ($null -ne $obj.pass_score) { $state.pass_score = [double]$obj.pass_score }
    if ($null -ne $obj.samples) { $state.samples = @($obj.samples) }
    if ($null -ne $obj.summary) {
        if ($null -ne $obj.summary.total) { $state.summary.total = [Math]::Max(0, [int]$obj.summary.total) }
        if ($null -ne $obj.summary.pass_count) { $state.summary.pass_count = [Math]::Max(0, [int]$obj.summary.pass_count) }
        if ($null -ne $obj.summary.pass_rate) { $state.summary.pass_rate = [double]$obj.summary.pass_rate }
        if ($null -ne $obj.summary.average_score) { $state.summary.average_score = [double]$obj.summary.average_score }
        if ($null -ne $obj.summary.last_score) { $state.summary.last_score = [double]$obj.summary.last_score }
        if ($null -ne $obj.summary.last_grade) { $state.summary.last_grade = [string]$obj.summary.last_grade }
    }
    return $state
}

function Save-ReplicaDrillScoreState {
    param(
        [string]$Path,
        [pscustomobject]$State
    )
    Ensure-ParentDir -PathValue $Path
    $State.updated_at = [DateTime]::UtcNow.ToString("o")
    $json = $State | ConvertTo-Json -Depth 14
    [System.IO.File]::WriteAllText($Path, $json, [System.Text.Encoding]::UTF8)
}

function Update-ReplicaDrillScoreState {
    param(
        [pscustomobject]$State,
        [string]$SnapshotSource,
        [string]$ReplaySource
    )
    $score = 0.0
    $hasSnapshot = -not [string]::IsNullOrWhiteSpace($SnapshotSource)
    $hasReplay = -not [string]::IsNullOrWhiteSpace($ReplaySource)
    if ($hasSnapshot) { $score += 50.0 }
    if ($hasReplay) { $score += 50.0 }
    if ($hasSnapshot -and [string]::Equals($SnapshotSource, $script:StateSnapshotPath, [System.StringComparison]::OrdinalIgnoreCase)) {
        $score -= 20.0
    }
    if ($hasReplay -and [string]::Equals($ReplaySource, $script:StateReplayPath, [System.StringComparison]::OrdinalIgnoreCase)) {
        $score -= 20.0
    }
    if ($score -lt 0.0) { $score = 0.0 }
    if ($score -gt 100.0) { $score = 100.0 }
    $grade = "red"
    if ($score -ge 90.0) {
        $grade = "green"
    } elseif ($score -ge $script:StateReplicaDrillPassScore) {
        $grade = "yellow"
    }
    $pass = ($score -ge $script:StateReplicaDrillPassScore)
    $sample = [pscustomobject]@{
        timestamp_utc = [DateTime]::UtcNow.ToString("o")
        score = [Math]::Round($score, 4)
        grade = $grade
        pass = $pass
        snapshot_source = [string]$SnapshotSource
        replay_source = [string]$ReplaySource
    }
    $arr = @($State.samples)
    $arr += $sample
    $window = [Math]::Max(1, [int]$script:StateReplicaDrillScoreWindowSamples)
    if ($arr.Count -gt $window) {
        $arr = @($arr[($arr.Count - $window)..($arr.Count - 1)])
    }
    $State.samples = $arr
    $total = $arr.Count
    $passCount = 0
    $sumScore = 0.0
    foreach ($s in $arr) {
        $sumScore += [double]$s.score
        if ([bool]$s.pass) {
            $passCount += 1
        }
    }
    $avgScore = 0.0
    $passRate = 0.0
    if ($total -gt 0) {
        $avgScore = $sumScore / $total
        $passRate = $passCount / $total
    }
    $State.window_samples = $window
    $State.pass_score = [double]$script:StateReplicaDrillPassScore
    $State.summary.total = $total
    $State.summary.pass_count = $passCount
    $State.summary.pass_rate = [Math]::Round($passRate, 6)
    $State.summary.average_score = [Math]::Round($avgScore, 4)
    $State.summary.last_score = [double]$sample.score
    $State.summary.last_grade = [string]$sample.grade
    return [pscustomobject]@{
        score = [double]$sample.score
        grade = [string]$sample.grade
        pass = [bool]$sample.pass
        pass_rate = [double]$State.summary.pass_rate
        average_score = [double]$State.summary.average_score
        total = [int]$State.summary.total
    }
}

function Observe-ReplicaSlo {
    param(
        [pscustomobject]$State,
        [string]$Grade,
        [int]$WindowSamples,
        [double]$MinGreenRate,
        [int]$MaxRedInWindow
    )
    $g = [string]$Grade
    if ($g -ne "green" -and $g -ne "yellow" -and $g -ne "red") {
        $g = "yellow"
    }
    $sample = [pscustomobject]@{
        timestamp_utc = [DateTime]::UtcNow.ToString("o")
        grade = $g
    }
    $arr = @($State.samples)
    $arr += $sample
    if ($arr.Count -gt $WindowSamples) {
        $arr = @($arr[($arr.Count - $WindowSamples)..($arr.Count - 1)])
    }
    $State.samples = $arr
    $total = $arr.Count
    $green = 0
    $yellow = 0
    $red = 0
    foreach ($s in $arr) {
        $sg = [string]$s.grade
        if ($sg -eq "green") { $green += 1; continue }
        if ($sg -eq "red") { $red += 1; continue }
        $yellow += 1
    }
    $greenRate = 1.0
    if ($total -gt 0) {
        $greenRate = [double]$green / [double]$total
    }
    $score = 100.0
    if ($total -gt 0) {
        $score = (([double]$green * 100.0) + ([double]$yellow * 60.0) + ([double]$red * 0.0)) / [double]$total
    }
    $reasons = @()
    if ($greenRate -lt $MinGreenRate) {
        $reasons += ("green_rate=" + [Math]::Round($greenRate, 4) + " < min=" + $MinGreenRate)
    }
    if ($red -gt $MaxRedInWindow) {
        $reasons += ("red_count=" + $red + " > max=" + $MaxRedInWindow)
    }
    $violation = ($reasons.Count -gt 0)
    $reasonText = ""
    if ($violation) {
        $reasonText = ($reasons -join "; ")
    }
    $State.window_samples = $WindowSamples
    $State.min_green_rate = $MinGreenRate
    $State.max_red_in_window = $MaxRedInWindow
    $State.last.total = $total
    $State.last.green = $green
    $State.last.yellow = $yellow
    $State.last.red = $red
    $State.last.green_rate = [Math]::Round($greenRate, 6)
    $State.last.score = [Math]::Round($score, 4)
    $State.last.violation = $violation
    $State.last.reason = $reasonText
    return [pscustomobject]@{
        total = $total
        green = $green
        yellow = $yellow
        red = $red
        green_rate = [double]$State.last.green_rate
        score = [double]$State.last.score
        violation = $violation
        reason = $reasonText
    }
}

function Build-DefaultCircuitMatrix {
    param(
        [int]$BaseConcurrent,
        [int]$BasePause,
        [int]$YellowConcurrent,
        [int]$YellowPause,
        [bool]$RedBlock
    )
    $greenConcurrent = [Math]::Max(1, $BaseConcurrent)
    $greenPause = [Math]::Max(0, $BasePause)
    $yellowConcurrent = [Math]::Max(1, [Math]::Min($greenConcurrent, $YellowConcurrent))
    $yellowPause = [Math]::Max($greenPause, $YellowPause)
    $redConcurrent = [Math]::Max(1, $yellowConcurrent)
    $redPause = [Math]::Max($yellowPause, $greenPause)
    return @(
        [pscustomobject]@{
            name = "green"
            min_score = 95.0
            max_score = 101.0
            max_concurrent_plans = $greenConcurrent
            dispatch_pause_seconds = $greenPause
            block_dispatch = $false
        },
        [pscustomobject]@{
            name = "yellow"
            min_score = 80.0
            max_score = 95.0
            max_concurrent_plans = $yellowConcurrent
            dispatch_pause_seconds = $yellowPause
            block_dispatch = $false
        },
        [pscustomobject]@{
            name = "red"
            min_score = 0.0
            max_score = 80.0
            max_concurrent_plans = $redConcurrent
            dispatch_pause_seconds = $redPause
            block_dispatch = [bool]$RedBlock
        }
    )
}

function Resolve-CircuitMatrix {
    param(
        [object]$RawRules,
        [int]$BaseConcurrent,
        [int]$BasePause,
        [int]$YellowConcurrent,
        [int]$YellowPause,
        [bool]$RedBlock
    )
    $rules = @()
    if ($null -ne $RawRules) {
        $idx = 0
        foreach ($r in $RawRules) {
            $idx += 1
            $name = "rule-" + $idx
            if ($null -ne $r.name -and -not [string]::IsNullOrWhiteSpace([string]$r.name)) {
                $name = [string]$r.name
            }
            $minScore = 0.0
            if ($null -ne $r.min_score) { $minScore = [double]$r.min_score }
            if ($minScore -lt 0.0) { $minScore = 0.0 }
            if ($minScore -gt 100.0) { $minScore = 100.0 }
            $maxScore = 101.0
            if ($null -ne $r.max_score) { $maxScore = [double]$r.max_score }
            if ($maxScore -lt $minScore) { $maxScore = $minScore }
            if ($maxScore -gt 101.0) { $maxScore = 101.0 }
            $cc = [Math]::Max(1, $BaseConcurrent)
            if ($null -ne $r.max_concurrent_plans) { $cc = [Math]::Max(1, [int]$r.max_concurrent_plans) }
            $pause = [Math]::Max(0, $BasePause)
            if ($null -ne $r.dispatch_pause_seconds) { $pause = [Math]::Max(0, [int]$r.dispatch_pause_seconds) }
            $block = $false
            if ($null -ne $r.block_dispatch) { $block = [bool]$r.block_dispatch }
            $rules += [pscustomobject]@{
                name = $name
                min_score = $minScore
                max_score = $maxScore
                max_concurrent_plans = $cc
                dispatch_pause_seconds = $pause
                block_dispatch = $block
            }
        }
    }
    if ($rules.Count -eq 0) {
        return (Build-DefaultCircuitMatrix -BaseConcurrent $BaseConcurrent -BasePause $BasePause -YellowConcurrent $YellowConcurrent -YellowPause $YellowPause -RedBlock $RedBlock)
    }
    return @($rules | Sort-Object @{ Expression = "min_score"; Descending = $true }, @{ Expression = "max_score"; Descending = $true }, @{ Expression = "name"; Descending = $false })
}

function Match-CircuitRuleByScore {
    param(
        [object[]]$Rules,
        [double]$Score
    )
    if ($null -eq $Rules -or $Rules.Count -eq 0) {
        return $null
    }
    $s = $Score
    if ($s -lt 0.0) { $s = 0.0 }
    if ($s -gt 100.0) { $s = 100.0 }
    foreach ($r in $Rules) {
        if ($s -ge [double]$r.min_score -and $s -lt [double]$r.max_score) {
            return $r
        }
    }
    return $Rules[$Rules.Count - 1]
}

function Apply-ReplicaSloPolicy {
    param(
        [string]$Source,
        [string]$QueuePath,
        [string]$PlanAction,
        [string]$ControlOpId,
        [string]$ControllerId,
        [object]$Validation,
        [string]$AuditPath
    )
    if (-not $script:StateReplicaSloEnabled) {
        return [pscustomobject]@{
            violation = $false
            reason = ""
            grade = "green"
            score = 100.0
            green_rate = 1.0
            total = 0
            green = 0
            yellow = 0
            red = 0
        }
    }
    $grade = [string]$Validation.overall_grade
    if ([string]::IsNullOrWhiteSpace($grade)) { $grade = "yellow" }
    if ($grade -ne "green" -and $grade -ne "yellow" -and $grade -ne "red") { $grade = "yellow" }
    $obs = $null
    $hasRolloutPolicyCli = (-not [string]::IsNullOrWhiteSpace([string]$script:RolloutPolicyCliBinaryPath)) -and (Test-Path -LiteralPath $script:RolloutPolicyCliBinaryPath)
    if ($hasRolloutPolicyCli) {
        try {
            $rustArgs = @(
                "slo-evaluate",
                "--state-file", [string]$script:StateReplicaSloPath,
                "--grade", $grade,
                "--window-samples", ([string][int]$script:StateReplicaSloWindowSamples),
                "--min-green-rate", ([string][double]$script:StateReplicaSloMinGreenRate),
                "--max-red-in-window", ([string][int]$script:StateReplicaSloMaxRedInWindow),
                "--block-on-violation", ([string][bool]$script:StateReplicaSloBlockOnViolation)
            )
            $rustOutput = Invoke-RolloutPolicyTool -BinaryPath $script:RolloutPolicyCliBinaryPath -ToolName "risk" -Args $rustArgs -CaptureOutput
            if ($LASTEXITCODE -eq 0) {
                $rustText = (($rustOutput | Out-String).Trim())
                if (-not [string]::IsNullOrWhiteSpace($rustText)) {
                    $rustLine = ($rustText -split "`r?`n" | Select-Object -Last 1)
                    $rustObj = $rustLine | ConvertFrom-Json
                    $obs = [pscustomobject]@{
                        total = [int]$rustObj.total
                        green = [int]$rustObj.green
                        yellow = [int]$rustObj.yellow
                        red = [int]$rustObj.red
                        green_rate = [double]$rustObj.green_rate
                        score = [double]$rustObj.score
                        violation = [bool]$rustObj.violation
                        reason = [string]$rustObj.reason
                    }
                }
            }
        } catch {
        }
    }
    if ($null -eq $obs) {
        $score = 60.0
        $green = 0
        $yellow = 1
        $red = 0
        $greenRate = 0.0
        switch ($grade) {
            "green" {
                $score = 100.0
                $green = 1
                $yellow = 0
                $red = 0
                $greenRate = 1.0
            }
            "red" {
                $score = 0.0
                $green = 0
                $yellow = 0
                $red = 1
                $greenRate = 0.0
            }
            default {
                $score = 60.0
                $green = 0
                $yellow = 1
                $red = 0
                $greenRate = 0.0
            }
        }
        $violation = ($grade -ne "green")
        $reason = ""
        if ($violation) {
            $reason = ("emergency_fallback grade={0} rust_risk_unavailable" -f $grade)
        }
        $obs = [pscustomobject]@{
            total = 1
            green = [int]$green
            yellow = [int]$yellow
            red = [int]$red
            green_rate = [double]$greenRate
            score = [double]$score
            violation = [bool]$violation
            reason = [string]$reason
        }
    }

    $effectiveScore = [double]$obs.score
    $adaptiveAction = "disabled"
    $adaptiveReason = ""
    if ($script:StateReplicaAdaptiveEnabled) {
        $adaptive = Update-ReplicaAdaptiveState -State $script:StateReplicaAdaptiveState -Obs $obs
        Save-ReplicaAdaptiveState -Path $script:StateReplicaAdaptivePath -State $script:StateReplicaAdaptiveState
        $effectiveScore = [double]$adaptive.effective_score
        $adaptiveAction = [string]$adaptive.action
        $adaptiveReason = [string]$adaptive.reason
        Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                timestamp_utc = [DateTime]::UtcNow.ToString("o")
                control_operation_id = $ControlOpId
                controller_id = $ControllerId
                queue_file = $QueuePath
                action = $PlanAction
                result = "replica_adaptive_update"
                source = $Source
                raw_score = $adaptive.raw_score
                effective_score = $adaptive.effective_score
                adaptive_bias = $adaptive.bias
                adaptive_action = $adaptive.action
                sample_count = $adaptive.sample_count
                error = $adaptive.reason
            })
    }

    if ($script:StateReplicaCircuitEnabled) {
        $rule = $null
        if ($hasRolloutPolicyCli) {
            try {
                $matrixJson = (@($script:StateReplicaCircuitMatrix) | ConvertTo-Json -Depth 20 -Compress)
                if ([string]::IsNullOrWhiteSpace($matrixJson)) {
                    $matrixJson = "[]"
                }
                $rustArgs = @(
                    "circuit-breaker-evaluate",
                    "--score", ([string][double]$effectiveScore),
                    "--base-concurrent", ([string][int]$script:StateReplicaCircuitBaseConcurrent),
                    "--base-pause", ([string][int]$script:StateReplicaCircuitBasePauseSeconds),
                    "--yellow-concurrent", ([string][int]$script:StateReplicaCircuitYellowMaxConcurrent),
                    "--yellow-pause", ([string][int]$script:StateReplicaCircuitYellowPauseSeconds),
                    "--red-block", ([string][bool]$script:StateReplicaCircuitRedBlock),
                    "--matrix-json", $matrixJson
                )
                $rustOutput = Invoke-RolloutPolicyTool -BinaryPath $script:RolloutPolicyCliBinaryPath -ToolName "risk" -Args $rustArgs -CaptureOutput
                if ($LASTEXITCODE -eq 0) {
                    $rustText = (($rustOutput | Out-String).Trim())
                    if (-not [string]::IsNullOrWhiteSpace($rustText)) {
                        $rustLine = ($rustText -split "`r?`n" | Select-Object -Last 1)
                        $rustObj = $rustLine | ConvertFrom-Json
                        $rule = [pscustomobject]@{
                            name = [string]$rustObj.rule
                            min_score = [double]$rustObj.min_score
                            max_score = [double]$rustObj.max_score
                            max_concurrent_plans = [int]$rustObj.max_concurrent_plans
                            dispatch_pause_seconds = [int]$rustObj.dispatch_pause_seconds
                            block_dispatch = [bool]$rustObj.block_dispatch
                        }
                    }
                }
            } catch {
            }
        }
        if ($null -eq $rule) {
            $greenConcurrent = [Math]::Max(1, [int]$script:StateReplicaCircuitBaseConcurrent)
            $greenPause = [Math]::Max(0, [int]$script:StateReplicaCircuitBasePauseSeconds)
            $yellowConcurrent = [Math]::Max(1, [Math]::Min($greenConcurrent, [int]$script:StateReplicaCircuitYellowMaxConcurrent))
            $yellowPause = [Math]::Max($greenPause, [int]$script:StateReplicaCircuitYellowPauseSeconds)
            $redConcurrent = [Math]::Max(1, $yellowConcurrent)
            $redPause = [Math]::Max($yellowPause, $greenPause)
            switch ($grade) {
                "green" {
                    $rule = [pscustomobject]@{
                        name = "emergency-green"
                        min_score = 95.0
                        max_score = 101.0
                        max_concurrent_plans = $greenConcurrent
                        dispatch_pause_seconds = $greenPause
                        block_dispatch = $false
                    }
                }
                "yellow" {
                    $rule = [pscustomobject]@{
                        name = "emergency-yellow"
                        min_score = 80.0
                        max_score = 95.0
                        max_concurrent_plans = $yellowConcurrent
                        dispatch_pause_seconds = $yellowPause
                        block_dispatch = $false
                    }
                }
                default {
                    $rule = [pscustomobject]@{
                        name = "emergency-red"
                        min_score = 0.0
                        max_score = 80.0
                        max_concurrent_plans = $redConcurrent
                        dispatch_pause_seconds = $redPause
                        block_dispatch = [bool]$script:StateReplicaCircuitRedBlock
                    }
                }
            }
        }
        if ($null -eq $rule) {
            $rule = [pscustomobject]@{
                name = "fallback"
                min_score = 0.0
                max_score = 101.0
                max_concurrent_plans = $script:StateReplicaCircuitBaseConcurrent
                dispatch_pause_seconds = $script:StateReplicaCircuitBasePauseSeconds
                block_dispatch = $false
            }
        }
        $script:StateReplicaCircuitCurrentGrade = $grade
        $script:StateReplicaCircuitCurrentRule = [string]$rule.name
        $script:StateReplicaCircuitCurrentConcurrentCap = [Math]::Max(1, [int]$rule.max_concurrent_plans)
        $script:StateReplicaCircuitCurrentPauseSeconds = [Math]::Max(0, [int]$rule.dispatch_pause_seconds)
        $script:StateReplicaCircuitCurrentBlockDispatch = [bool]$rule.block_dispatch
        Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                timestamp_utc = [DateTime]::UtcNow.ToString("o")
                control_operation_id = $ControlOpId
                controller_id = $ControllerId
                queue_file = $QueuePath
                action = $PlanAction
                result = "replica_circuit_state"
                source = $Source
                grade = $script:StateReplicaCircuitCurrentGrade
                rule = $script:StateReplicaCircuitCurrentRule
                score = $obs.score
                effective_score = $effectiveScore
                min_score = $rule.min_score
                max_score = $rule.max_score
                concurrent_cap = $script:StateReplicaCircuitCurrentConcurrentCap
                pause_seconds = $script:StateReplicaCircuitCurrentPauseSeconds
                block_dispatch = [bool]$script:StateReplicaCircuitCurrentBlockDispatch
                adaptive_enabled = [bool]$script:StateReplicaAdaptiveEnabled
                adaptive_bias = [double]$script:StateReplicaAdaptiveState.bias
                adaptive_action = $adaptiveAction
                error = ""
            })
    }

    Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
            timestamp_utc = [DateTime]::UtcNow.ToString("o")
            control_operation_id = $ControlOpId
            controller_id = $ControllerId
            queue_file = $QueuePath
            action = $PlanAction
            result = "replica_slo_update"
            source = $Source
            score = $obs.score
            green_rate = $obs.green_rate
            total = $obs.total
            green = $obs.green
            yellow = $obs.yellow
            red = $obs.red
            error = ""
        })
    if ($obs.violation) {
        Write-Host ("rollout_control_replica_slo_violation: source={0} reason={1}" -f $Source, $obs.reason)
        Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                timestamp_utc = [DateTime]::UtcNow.ToString("o")
                control_operation_id = $ControlOpId
                controller_id = $ControllerId
                queue_file = $QueuePath
                action = $PlanAction
                result = "replica_slo_violation"
                source = $Source
                score = $obs.score
                green_rate = $obs.green_rate
                total = $obs.total
                green = $obs.green
                yellow = $obs.yellow
                red = $obs.red
                error = $obs.reason
            })
        if ($script:StateReplicaSloBlockOnViolation) {
            throw ("replica slo policy violation: " + $obs.reason)
        }
    }
    return [pscustomobject]@{
        violation = [bool]$obs.violation
        reason = [string]$obs.reason
        grade = $grade
        score = [double]$obs.score
        green_rate = [double]$obs.green_rate
        total = [int]$obs.total
        green = [int]$obs.green
        yellow = [int]$obs.yellow
        red = [int]$obs.red
    }
}

function Restore-ReplicaModeFromHealthFile {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path)) {
        return
    }
    try {
        $obj = (Get-Content -LiteralPath $Path -Raw) | ConvertFrom-Json -ErrorAction Stop
        if ($null -ne $obj.mode) {
            if ($null -ne $obj.mode.failover_mode) { $script:StateReplicaFailoverMode = [bool]$obj.mode.failover_mode }
            if ($null -ne $obj.mode.stable_cycles) { $script:StateReplicaStableCycles = [Math]::Max(0, [int]$obj.mode.stable_cycles) }
        }
        if ($null -ne $obj.failover) {
            if ($null -ne $obj.failover.count) { $script:StateReplicaFailoverCount = [Math]::Max(0, [int]$obj.failover.count) }
            if ($null -ne $obj.failover.last_unix_ms) { $script:StateReplicaLastFailoverMs = [int64]$obj.failover.last_unix_ms }
        }
    } catch {
        return
    }
}

function File-ExistsAndReadable {
    param([string]$Path)
    if ([string]::IsNullOrWhiteSpace([string]$Path)) {
        return $false
    }
    if (-not (Test-Path -LiteralPath $Path)) {
        return $false
    }
    try {
        $null = Get-Content -LiteralPath $Path -Raw
        return $true
    } catch {
        return $false
    }
}

function Select-SnapshotFailoverSource {
    param(
        [string]$PrimaryPath,
        [string[]]$ReplicaPaths
    )
    $files = @($PrimaryPath) + @($ReplicaPaths)
    $digestMap = @{}
    foreach ($path in $files) {
        if (-not (File-ExistsAndReadable -Path $path)) {
            continue
        }
        $dig = Get-FileDigest -Path $path
        $h = [string]$dig.hash
        if ([string]::IsNullOrWhiteSpace($h)) {
            continue
        }
        if (-not $digestMap.ContainsKey($h)) {
            $digestMap[$h] = @()
        }
        $digestMap[$h] += $path
    }
    if ($digestMap.Keys.Count -eq 0) {
        return ""
    }
    $winnerHash = ""
    $winnerCount = -1
    foreach ($h in $digestMap.Keys) {
        $cnt = @($digestMap[$h]).Count
        if ($cnt -gt $winnerCount) {
            $winnerHash = $h
            $winnerCount = $cnt
            continue
        }
        if ($cnt -eq $winnerCount -and $h -lt $winnerHash) {
            $winnerHash = $h
        }
    }
    $cands = @($digestMap[$winnerHash])
    if ($cands -contains $PrimaryPath) {
        return $PrimaryPath
    }
    return [string]$cands[0]
}

function Select-ReplayFailoverSource {
    param(
        [string]$PrimaryPath,
        [string[]]$ReplicaPaths
    )
    $files = @($PrimaryPath) + @($ReplicaPaths)
    $winner = ""
    $winnerLines = -1
    foreach ($path in $files) {
        if (-not (File-ExistsAndReadable -Path $path)) {
            continue
        }
        $dig = Get-FileDigest -Path $path
        $lines = [int]$dig.line_count
        if ($lines -gt $winnerLines) {
            $winnerLines = $lines
            $winner = $path
            continue
        }
        if ($lines -eq $winnerLines -and $path -eq $PrimaryPath) {
            $winner = $path
        }
    }
    return $winner
}

function Sync-StateFileSetFromSource {
    param(
        [string]$SourcePath,
        [string]$PrimaryPath,
        [string[]]$ReplicaPaths
    )
    if (-not (File-ExistsAndReadable -Path $SourcePath)) {
        return $false
    }
    $content = Get-Content -LiteralPath $SourcePath -Raw
    Write-ContentWithReplicas -PrimaryPath $PrimaryPath -ReplicaPaths $ReplicaPaths -Content $content
    return $true
}

function Resolve-FailoverPolicyMatrix {
    param([object[]]$RawRules)
    $hasFailoverPolicyMatrixBuildBinary = (-not [string]::IsNullOrWhiteSpace([string]$script:FailoverPolicyMatrixBuildBinaryPath)) -and (Test-Path -LiteralPath $script:FailoverPolicyMatrixBuildBinaryPath)
    if ($hasFailoverPolicyMatrixBuildBinary) {
        try {
            $rawRulesJson = (@($RawRules) | ConvertTo-Json -Depth 24 -Compress)
            if ([string]::IsNullOrWhiteSpace([string]$rawRulesJson)) {
                $rawRulesJson = "[]"
            }
            $rustOutput = Invoke-RolloutPolicyTool -BinaryPath $script:FailoverPolicyMatrixBuildBinaryPath -ToolName "failover-policy-matrix-build" -Args @("--raw-rules-json", $rawRulesJson) -CaptureOutput
            if ($LASTEXITCODE -eq 0) {
                $rustText = (($rustOutput | Out-String).Trim())
                if (-not [string]::IsNullOrWhiteSpace($rustText)) {
                    $rustLine = ($rustText -split "`r?`n" | Select-Object -Last 1)
                    $rustObj = $rustLine | ConvertFrom-Json
                    if ($null -ne $rustObj -and $null -ne $rustObj.matrix) {
                        return @($rustObj.matrix | Sort-Object @{ Expression = "order"; Descending = $false })
                    }
                }
            }
        } catch {
        }
    }
    $rules = @()
    $order = 0
    foreach ($r in @($RawRules)) {
        if ($null -eq $r) {
            continue
        }
        $order += 1
        $name = [string]$r.name
        if ([string]::IsNullOrWhiteSpace($name)) {
            $name = ("rule-" + $order)
        }
        $source = "*"
        if ($null -ne $r.source -and -not [string]::IsNullOrWhiteSpace([string]$r.source)) {
            $source = [string]$r.source
        }
        $source = $source.ToLowerInvariant()
        $allow = $true
        if ($null -ne $r.allow_auto_failover) {
            $allow = [bool]$r.allow_auto_failover
        }
        $minSitePriority = [int]::MinValue
        if ($null -ne $r.min_site_priority) {
            $minSitePriority = [int]$r.min_site_priority
        }
        $maxFailoverCount = -1
        if ($null -ne $r.max_failover_count) {
            $maxFailoverCount = [Math]::Max(0, [int]$r.max_failover_count)
        }
        $cooldownSeconds = -1
        if ($null -ne $r.cooldown_seconds) {
            $cooldownSeconds = [Math]::Max(1, [int]$r.cooldown_seconds)
        }
        $requireModeSet = $false
        $requireMode = $false
        if ($null -ne $r.require_failover_mode) {
            $requireModeSet = $true
            $requireMode = [bool]$r.require_failover_mode
        }
        $grades = @()
        if ($null -ne $r.grades) {
            foreach ($g in @($r.grades)) {
                if ($null -eq $g) { continue }
                $gText = [string]$g
                if ([string]::IsNullOrWhiteSpace($gText)) { continue }
                $gNorm = $gText.ToLowerInvariant()
                if ($gNorm -eq "*" -or $gNorm -eq "green" -or $gNorm -eq "yellow" -or $gNorm -eq "red") {
                    $grades += $gNorm
                }
            }
        } elseif ($null -ne $r.grade -and -not [string]::IsNullOrWhiteSpace([string]$r.grade)) {
            $gNorm = ([string]$r.grade).ToLowerInvariant()
            if ($gNorm -eq "*" -or $gNorm -eq "green" -or $gNorm -eq "yellow" -or $gNorm -eq "red") {
                $grades += $gNorm
            }
        }
        if ($grades.Count -eq 0) {
            $grades = @("*")
        }
        $rules += [pscustomobject]@{
            order = $order
            name = $name
            source = $source
            grades = $grades
            allow_auto_failover = $allow
            min_site_priority = $minSitePriority
            max_failover_count = $maxFailoverCount
            cooldown_seconds = $cooldownSeconds
            has_require_failover_mode = $requireModeSet
            require_failover_mode = $requireMode
        }
    }
    return @($rules | Sort-Object @{ Expression = "order"; Descending = $false })
}

function Resolve-FailoverPolicyMatrixMap {
    param(
        [object]$RawMap,
        [object[]]$FallbackMatrix,
        [bool]$NormalizeRegion = $false
    )
    $map = @{}
    if ($null -eq $RawMap) {
        return $map
    }
    foreach ($p in $RawMap.PSObject.Properties) {
        $k = ([string]$p.Name).Trim()
        if ([string]::IsNullOrWhiteSpace($k)) { continue }
        if ($NormalizeRegion) {
            $k = $k.ToUpperInvariant()
        }
        $rules = Resolve-FailoverPolicyMatrix -RawRules @($p.Value)
        if ($null -eq $rules -or $rules.Count -eq 0) {
            $rules = @($FallbackMatrix)
        }
        $map[$k] = $rules
    }
    return $map
}

function Select-FailoverPolicyMatrix {
    param([string]$SiteId)
    $site = ""
    if ($null -ne $SiteId) {
        $site = ([string]$SiteId).Trim()
    }
    if (-not [string]::IsNullOrWhiteSpace($site) -and $null -ne $script:StateReplicaFailoverPolicyMatrixBySite -and $script:StateReplicaFailoverPolicyMatrixBySite.ContainsKey($site)) {
        return [pscustomobject]@{
            scope = ("site:" + $site)
            matrix = $script:StateReplicaFailoverPolicyMatrixBySite[$site]
        }
    }
    $region = ""
    if (-not [string]::IsNullOrWhiteSpace($site) -and $null -ne $script:SiteRegionMap -and $script:SiteRegionMap.ContainsKey($site)) {
        $region = [string]$script:SiteRegionMap[$site]
    }
    if (-not [string]::IsNullOrWhiteSpace($region) -and $null -ne $script:StateReplicaFailoverPolicyMatrixByRegion -and $script:StateReplicaFailoverPolicyMatrixByRegion.ContainsKey($region)) {
        return [pscustomobject]@{
            scope = ("region:" + $region)
            matrix = $script:StateReplicaFailoverPolicyMatrixByRegion[$region]
        }
    }
    return [pscustomobject]@{
        scope = "global"
        matrix = $script:StateReplicaFailoverPolicyMatrix
    }
}

function Resolve-FailoverSloLinkProfileMap {
    param(
        [object]$RawMap,
        [pscustomobject]$FallbackProfile,
        [bool]$NormalizeRegion = $false
    )
    $map = @{}
    if ($null -eq $RawMap) {
        return $map
    }
    foreach ($p in $RawMap.PSObject.Properties) {
        $k = ([string]$p.Name).Trim()
        if ([string]::IsNullOrWhiteSpace($k)) { continue }
        if ($NormalizeRegion) {
            $k = $k.ToUpperInvariant()
        }
        $v = $p.Value
        $profile = [pscustomobject]@{
            enabled = [bool]$FallbackProfile.enabled
            min_effective_score = [double]$FallbackProfile.min_effective_score
            block_on_violation = [bool]$FallbackProfile.block_on_violation
        }
        if ($null -ne $v.enabled) { $profile.enabled = [bool]$v.enabled }
        if ($null -ne $v.min_effective_score) { $profile.min_effective_score = [double]$v.min_effective_score }
        if ($null -ne $v.block_on_violation) { $profile.block_on_violation = [bool]$v.block_on_violation }
        if ($profile.min_effective_score -lt 0.0) { $profile.min_effective_score = 0.0 }
        if ($profile.min_effective_score -gt 100.0) { $profile.min_effective_score = 100.0 }
        $map[$k] = $profile
    }
    return $map
}

function Resolve-FailoverDrillLinkProfileMap {
    param(
        [object]$RawMap,
        [pscustomobject]$FallbackProfile,
        [bool]$NormalizeRegion = $false
    )
    $map = @{}
    if ($null -eq $RawMap) {
        return $map
    }
    foreach ($p in $RawMap.PSObject.Properties) {
        $k = ([string]$p.Name).Trim()
        if ([string]::IsNullOrWhiteSpace($k)) { continue }
        if ($NormalizeRegion) {
            $k = $k.ToUpperInvariant()
        }
        $v = $p.Value
        $profile = [pscustomobject]@{
            enabled = [bool]$FallbackProfile.enabled
            min_pass_rate = [double]$FallbackProfile.min_pass_rate
            min_average_score = [double]$FallbackProfile.min_average_score
            require_last_pass = [bool]$FallbackProfile.require_last_pass
        }
        if ($null -ne $v.enabled) { $profile.enabled = [bool]$v.enabled }
        if ($null -ne $v.min_pass_rate) { $profile.min_pass_rate = [double]$v.min_pass_rate }
        if ($null -ne $v.min_average_score) { $profile.min_average_score = [double]$v.min_average_score }
        if ($null -ne $v.require_last_pass) { $profile.require_last_pass = [bool]$v.require_last_pass }
        if ($profile.min_pass_rate -lt 0.0) { $profile.min_pass_rate = 0.0 }
        if ($profile.min_pass_rate -gt 1.0) { $profile.min_pass_rate = 1.0 }
        if ($profile.min_average_score -lt 0.0) { $profile.min_average_score = 0.0 }
        if ($profile.min_average_score -gt 100.0) { $profile.min_average_score = 100.0 }
        $map[$k] = $profile
    }
    return $map
}

function Resolve-FailoverRiskLinkProfileMap {
    param(
        [object]$RawMap,
        [pscustomobject]$FallbackProfile,
        [bool]$NormalizeRegion = $false
    )
    $map = @{}
    if ($null -eq $RawMap) {
        return $map
    }
    foreach ($p in $RawMap.PSObject.Properties) {
        $k = ([string]$p.Name).Trim()
        if ([string]::IsNullOrWhiteSpace($k)) { continue }
        if ($NormalizeRegion) {
            $k = $k.ToUpperInvariant()
        }
        $v = $p.Value
        $profile = [pscustomobject]@{
            enabled = [bool]$FallbackProfile.enabled
        }
        if ($null -ne $v.enabled) { $profile.enabled = [bool]$v.enabled }
        $map[$k] = $profile
    }
    return $map
}

function Select-FailoverSloLinkProfile {
    param([string]$SiteId)
    $site = ""
    if ($null -ne $SiteId) { $site = ([string]$SiteId).Trim() }
    if (-not [string]::IsNullOrWhiteSpace($site) -and $null -ne $script:StateReplicaFailoverSloLinkProfileBySite -and $script:StateReplicaFailoverSloLinkProfileBySite.ContainsKey($site)) {
        return [pscustomobject]@{ scope = ("site:" + $site); profile = $script:StateReplicaFailoverSloLinkProfileBySite[$site] }
    }
    $region = ""
    if (-not [string]::IsNullOrWhiteSpace($site) -and $null -ne $script:SiteRegionMap -and $script:SiteRegionMap.ContainsKey($site)) {
        $region = [string]$script:SiteRegionMap[$site]
    }
    if (-not [string]::IsNullOrWhiteSpace($region) -and $null -ne $script:StateReplicaFailoverSloLinkProfileByRegion -and $script:StateReplicaFailoverSloLinkProfileByRegion.ContainsKey($region)) {
        return [pscustomobject]@{ scope = ("region:" + $region); profile = $script:StateReplicaFailoverSloLinkProfileByRegion[$region] }
    }
    return [pscustomobject]@{ scope = "global"; profile = $script:StateReplicaFailoverSloLinkProfileGlobal }
}

function Select-FailoverDrillLinkProfile {
    param([string]$SiteId)
    $site = ""
    if ($null -ne $SiteId) { $site = ([string]$SiteId).Trim() }
    if (-not [string]::IsNullOrWhiteSpace($site) -and $null -ne $script:StateReplicaFailoverDrillLinkProfileBySite -and $script:StateReplicaFailoverDrillLinkProfileBySite.ContainsKey($site)) {
        return [pscustomobject]@{ scope = ("site:" + $site); profile = $script:StateReplicaFailoverDrillLinkProfileBySite[$site] }
    }
    $region = ""
    if (-not [string]::IsNullOrWhiteSpace($site) -and $null -ne $script:SiteRegionMap -and $script:SiteRegionMap.ContainsKey($site)) {
        $region = [string]$script:SiteRegionMap[$site]
    }
    if (-not [string]::IsNullOrWhiteSpace($region) -and $null -ne $script:StateReplicaFailoverDrillLinkProfileByRegion -and $script:StateReplicaFailoverDrillLinkProfileByRegion.ContainsKey($region)) {
        return [pscustomobject]@{ scope = ("region:" + $region); profile = $script:StateReplicaFailoverDrillLinkProfileByRegion[$region] }
    }
    return [pscustomobject]@{ scope = "global"; profile = $script:StateReplicaFailoverDrillLinkProfileGlobal }
}

function Select-FailoverRiskLinkProfile {
    param([string]$SiteId)
    $site = ""
    if ($null -ne $SiteId) { $site = ([string]$SiteId).Trim() }
    if (-not [string]::IsNullOrWhiteSpace($site) -and $null -ne $script:StateReplicaFailoverRiskLinkProfileBySite -and $script:StateReplicaFailoverRiskLinkProfileBySite.ContainsKey($site)) {
        return [pscustomobject]@{ scope = ("site:" + $site); profile = $script:StateReplicaFailoverRiskLinkProfileBySite[$site] }
    }
    $region = ""
    if (-not [string]::IsNullOrWhiteSpace($site) -and $null -ne $script:SiteRegionMap -and $script:SiteRegionMap.ContainsKey($site)) {
        $region = [string]$script:SiteRegionMap[$site]
    }
    if (-not [string]::IsNullOrWhiteSpace($region) -and $null -ne $script:StateReplicaFailoverRiskLinkProfileByRegion -and $script:StateReplicaFailoverRiskLinkProfileByRegion.ContainsKey($region)) {
        return [pscustomobject]@{ scope = ("region:" + $region); profile = $script:StateReplicaFailoverRiskLinkProfileByRegion[$region] }
    }
    return [pscustomobject]@{ scope = "global"; profile = $script:StateReplicaFailoverRiskLinkProfileGlobal }
}

function Resolve-FailoverConvergeProfileMap {
    param(
        [object]$RawMap,
        [pscustomobject]$FallbackProfile,
        [bool]$NormalizeRegion = $false
    )
    $map = @{}
    if ($null -eq $RawMap) {
        return $map
    }
    foreach ($p in $RawMap.PSObject.Properties) {
        $k = ([string]$p.Name).Trim()
        if ([string]::IsNullOrWhiteSpace($k)) { continue }
        if ($NormalizeRegion) {
            $k = $k.ToUpperInvariant()
        }
        $v = $p.Value
        $profile = [pscustomobject]@{
            enabled = [bool]$FallbackProfile.enabled
            max_concurrent_plans = [int]$FallbackProfile.max_concurrent_plans
            min_dispatch_pause_seconds = [int]$FallbackProfile.min_dispatch_pause_seconds
            block_on_snapshot_red = [bool]$FallbackProfile.block_on_snapshot_red
            block_on_replay_red = [bool]$FallbackProfile.block_on_replay_red
        }
        if ($null -ne $v.enabled) { $profile.enabled = [bool]$v.enabled }
        if ($null -ne $v.max_concurrent_plans) { $profile.max_concurrent_plans = [int]$v.max_concurrent_plans }
        if ($null -ne $v.min_dispatch_pause_seconds) { $profile.min_dispatch_pause_seconds = [int]$v.min_dispatch_pause_seconds }
        if ($null -ne $v.block_on_snapshot_red) { $profile.block_on_snapshot_red = [bool]$v.block_on_snapshot_red }
        if ($null -ne $v.block_on_replay_red) { $profile.block_on_replay_red = [bool]$v.block_on_replay_red }
        if ($profile.max_concurrent_plans -lt 1) { $profile.max_concurrent_plans = 1 }
        if ($profile.min_dispatch_pause_seconds -lt 0) { $profile.min_dispatch_pause_seconds = 0 }
        $map[$k] = $profile
    }
    return $map
}

function Select-FailoverConvergeProfile {
    param([string]$SiteId)
    $site = ""
    if ($null -ne $SiteId) { $site = ([string]$SiteId).Trim() }
    if (-not [string]::IsNullOrWhiteSpace($site) -and $null -ne $script:StateReplicaFailoverConvergeProfileBySite -and $script:StateReplicaFailoverConvergeProfileBySite.ContainsKey($site)) {
        return [pscustomobject]@{ scope = ("site:" + $site); profile = $script:StateReplicaFailoverConvergeProfileBySite[$site] }
    }
    $region = ""
    if (-not [string]::IsNullOrWhiteSpace($site) -and $null -ne $script:SiteRegionMap -and $script:SiteRegionMap.ContainsKey($site)) {
        $region = [string]$script:SiteRegionMap[$site]
    }
    if (-not [string]::IsNullOrWhiteSpace($region) -and $null -ne $script:StateReplicaFailoverConvergeProfileByRegion -and $script:StateReplicaFailoverConvergeProfileByRegion.ContainsKey($region)) {
        return [pscustomobject]@{ scope = ("region:" + $region); profile = $script:StateReplicaFailoverConvergeProfileByRegion[$region] }
    }
    return [pscustomobject]@{ scope = "global"; profile = $script:StateReplicaFailoverConvergeProfileGlobal }
}

function Match-FailoverPolicyRule {
    param(
        [object[]]$Rules,
        [string]$Source,
        [string]$Grade,
        [int]$SitePriority,
        [int]$FailoverCount,
        [bool]$FailoverMode
    )
    if ($null -eq $Rules -or $Rules.Count -eq 0) {
        return $null
    }
    $src = [string]$Source
    if ([string]::IsNullOrWhiteSpace($src)) { $src = "*" }
    $src = $src.ToLowerInvariant()
    $grade = [string]$Grade
    if ([string]::IsNullOrWhiteSpace($grade)) { $grade = "red" }
    $grade = $grade.ToLowerInvariant()
    if ($grade -ne "green" -and $grade -ne "yellow" -and $grade -ne "red") { $grade = "red" }
    foreach ($r in @($Rules)) {
        $ruleSource = [string]$r.source
        if ($ruleSource -ne "*" -and $ruleSource -ne $src) {
            continue
        }
        if ([int]$r.min_site_priority -gt $SitePriority) {
            continue
        }
        if ([int]$r.max_failover_count -ge 0 -and $FailoverCount -ge [int]$r.max_failover_count) {
            continue
        }
        if ([bool]$r.has_require_failover_mode -and ([bool]$r.require_failover_mode -ne $FailoverMode)) {
            continue
        }
        $gradeMatched = $false
        foreach ($g in @($r.grades)) {
            $v = [string]$g
            if ($v -eq "*" -or $v -eq $grade) {
                $gradeMatched = $true
                break
            }
        }
        if (-not $gradeMatched) {
            continue
        }
        return $r
    }
    return $null
}

function Evaluate-ReplicaFailoverPolicy {
    param(
        [string]$Source,
        [object]$Validation
    )
    $grade = "red"
    if ($null -ne $Validation -and $null -ne $Validation.overall_grade -and -not [string]::IsNullOrWhiteSpace([string]$Validation.overall_grade)) {
        $grade = [string]$Validation.overall_grade
    }
    $grade = $grade.ToLowerInvariant()
    if ($grade -ne "green" -and $grade -ne "yellow" -and $grade -ne "red") {
        $grade = "red"
    }
    $sitePriority = Site-Priority -Site $script:SiteId
    $baseCooldown = [Math]::Max(1, [int]$script:StateReplicaFailoverCooldownSec)
    $sloScore = 100.0
    $sloViolation = $false
    if ($script:StateReplicaSloEnabled -and $null -ne $script:StateReplicaSloState.last) {
        $sloScore = [double]$script:StateReplicaSloState.last.score
        $sloViolation = [bool]$script:StateReplicaSloState.last.violation
    }
    $adaptiveBias = 0.0
    if ($script:StateReplicaAdaptiveEnabled -and $null -ne $script:StateReplicaAdaptiveState) {
        $adaptiveBias = [double]$script:StateReplicaAdaptiveState.bias
    }
    $effectiveScore = $sloScore - $adaptiveBias
    if ($effectiveScore -lt 0.0) { $effectiveScore = 0.0 }
    if ($effectiveScore -gt 100.0) { $effectiveScore = 100.0 }
    $drillPassRate = 0.0
    $drillAvgScore = 0.0
    $drillLastPass = $false
    if ($script:StateReplicaDrillScoreEnabled -and $null -ne $script:StateReplicaDrillScoreState.summary) {
        $drillPassRate = [double]$script:StateReplicaDrillScoreState.summary.pass_rate
        $drillAvgScore = [double]$script:StateReplicaDrillScoreState.summary.average_score
        $drillLastPass = ([double]$script:StateReplicaDrillScoreState.summary.last_score -ge [double]$script:StateReplicaDrillPassScore)
    }
    if (-not $script:StateReplicaFailoverPolicyEnabled) {
        $script:StateReplicaFailoverPolicyLastRule = "disabled"
        $script:StateReplicaFailoverPolicyLastScope = "disabled"
        $script:StateReplicaFailoverPolicyLastAllowed = $true
        $script:StateReplicaFailoverPolicyLastReason = "policy disabled"
        $script:StateReplicaFailoverPolicyLastCooldownSec = $baseCooldown
        $script:StateReplicaFailoverPolicyLastSloGate = "disabled"
        $script:StateReplicaFailoverPolicyLastDrillGate = "disabled"
        return [pscustomobject]@{
            allowed = $true
            rule = "disabled"
            scope = "disabled"
            reason = "policy disabled"
            cooldown_seconds = $baseCooldown
            grade = $grade
            site_priority = $sitePriority
            effective_score = [Math]::Round($effectiveScore, 4)
            slo_violation = $sloViolation
            drill_pass_rate = [Math]::Round($drillPassRate, 6)
            drill_average_score = [Math]::Round($drillAvgScore, 4)
        }
    }
    $selectedPolicy = Select-FailoverPolicyMatrix -SiteId $script:SiteId
    $rule = Match-FailoverPolicyRule -Rules $selectedPolicy.matrix -Source $Source -Grade $grade -SitePriority $sitePriority -FailoverCount $script:StateReplicaFailoverCount -FailoverMode $script:StateReplicaFailoverMode
    $ruleScope = [string]$selectedPolicy.scope
    $sloLinkSel = Select-FailoverSloLinkProfile -SiteId $script:SiteId
    $drillLinkSel = Select-FailoverDrillLinkProfile -SiteId $script:SiteId
    $riskLinkSel = Select-FailoverRiskLinkProfile -SiteId $script:SiteId
    $sloLinkEnabled = [bool]$sloLinkSel.profile.enabled
    $sloMinEffectiveScore = [double]$sloLinkSel.profile.min_effective_score
    $sloBlockOnViolation = [bool]$sloLinkSel.profile.block_on_violation
    $drillLinkEnabled = [bool]$drillLinkSel.profile.enabled
    $drillMinPassRate = [double]$drillLinkSel.profile.min_pass_rate
    $drillMinAverageScore = [double]$drillLinkSel.profile.min_average_score
    $drillRequireLastPass = [bool]$drillLinkSel.profile.require_last_pass
    $riskLinkEnabled = [bool]$riskLinkSel.profile.enabled
    $allowed = [bool]$script:StateReplicaFailoverPolicyDefaultAllow
    $ruleName = if ($allowed) { "default_allow" } else { "default_block" }
    $reason = if ($allowed) { "no rule matched, default allow" } else { "no rule matched, default block" }
    $cooldown = $baseCooldown
    if ($null -ne $rule) {
        $allowed = [bool]$rule.allow_auto_failover
        $ruleName = [string]$rule.name
        $reason = if ($allowed) { "matched allow rule" } else { "matched block rule" }
        if ([int]$rule.cooldown_seconds -gt 0) {
            $cooldown = [int]$rule.cooldown_seconds
        }
    }
    $sloGate = "pass"
    if ($sloLinkEnabled) {
        if ($sloBlockOnViolation -and $sloViolation) {
            $allowed = $false
            $sloGate = ("blocked_by_slo_violation scope=" + [string]$sloLinkSel.scope)
            $reason = "blocked by slo violation link"
        } elseif ($effectiveScore -lt $sloMinEffectiveScore) {
            $allowed = $false
            $sloGate = ("blocked_by_slo_score scope=" + [string]$sloLinkSel.scope + " " + [Math]::Round($effectiveScore, 4) + " < " + $sloMinEffectiveScore)
            $reason = "blocked by slo effective score link"
        } else {
            $sloGate = ("pass scope=" + [string]$sloLinkSel.scope)
        }
    } else {
        $sloGate = "disabled"
    }
    $drillGate = "pass"
    if ($drillLinkEnabled) {
        if (-not $script:StateReplicaDrillScoreEnabled) {
            $allowed = $false
            $drillGate = ("blocked_no_drill_score_state scope=" + [string]$drillLinkSel.scope)
            $reason = "blocked by drill link without drill score state"
        } elseif ($drillPassRate -lt $drillMinPassRate) {
            $allowed = $false
            $drillGate = ("blocked_by_drill_pass_rate scope=" + [string]$drillLinkSel.scope + " " + [Math]::Round($drillPassRate, 6) + " < " + $drillMinPassRate)
            $reason = "blocked by drill pass rate link"
        } elseif ($drillAvgScore -lt $drillMinAverageScore) {
            $allowed = $false
            $drillGate = ("blocked_by_drill_avg_score scope=" + [string]$drillLinkSel.scope + " " + [Math]::Round($drillAvgScore, 4) + " < " + $drillMinAverageScore)
            $reason = "blocked by drill average score link"
        } elseif ($drillRequireLastPass -and -not $drillLastPass) {
            $allowed = $false
            $drillGate = ("blocked_by_drill_last_sample scope=" + [string]$drillLinkSel.scope)
            $reason = "blocked by drill last sample link"
        } else {
            $drillGate = ("pass scope=" + [string]$drillLinkSel.scope)
        }
    } else {
        $drillGate = "disabled"
    }
    $riskGate = "pass"
    if ($riskLinkEnabled) {
        $siteRiskLevel = Get-SiteConflictRiskLevel -Site $script:SiteId
        $riskSetSel = Select-RiskBlockedSet -Role "failover_risk_link" -SiteId $script:SiteId
        if ($riskSetSel.set.ContainsKey($siteRiskLevel)) {
            $allowed = $false
            $riskGate = ("blocked_by_site_risk link_scope=" + [string]$riskLinkSel.scope + " blocked_scope=" + [string]$riskSetSel.scope + " level=" + $siteRiskLevel)
            $reason = "blocked by site risk link"
        } else {
            $riskGate = ("pass link_scope=" + [string]$riskLinkSel.scope + " blocked_scope=" + [string]$riskSetSel.scope + " level=" + $siteRiskLevel)
        }
    } else {
        $riskGate = "disabled"
    }
    $script:StateReplicaFailoverPolicyLastRule = $ruleName
    $script:StateReplicaFailoverPolicyLastScope = $ruleScope
    $script:StateReplicaFailoverPolicyLastAllowed = $allowed
    $script:StateReplicaFailoverPolicyLastReason = $reason
    $script:StateReplicaFailoverPolicyLastCooldownSec = [Math]::Max(1, [int]$cooldown)
    $script:StateReplicaFailoverPolicyLastSloGate = $sloGate
    $script:StateReplicaFailoverPolicyLastDrillGate = $drillGate
    $script:StateReplicaFailoverPolicyLastRiskGate = $riskGate
    return [pscustomobject]@{
        allowed = $allowed
        rule = $ruleName
        scope = $ruleScope
        reason = $reason
        cooldown_seconds = [Math]::Max(1, [int]$cooldown)
        grade = $grade
        site_priority = $sitePriority
        slo_gate = $sloGate
        drill_gate = $drillGate
        risk_gate = $riskGate
        effective_score = [Math]::Round($effectiveScore, 4)
        slo_violation = $sloViolation
        drill_pass_rate = [Math]::Round($drillPassRate, 6)
        drill_average_score = [Math]::Round($drillAvgScore, 4)
        drill_last_pass = $drillLastPass
    }
}

function Try-AutoFailoverStateReplicas {
    param(
        [int]$AllowedLagEntries,
        [string]$Source,
        [object]$Validation
    )
    $details = @()
    if (-not $script:StateRecoveryEnabled -or -not $script:StateReplicaAutoFailoverEnabled) {
        return [pscustomobject]@{
            changed = $false
            details = $details
            skipped = "disabled"
            policy_rule = "disabled"
            policy_reason = "auto failover disabled"
        }
    }
    $policy = Evaluate-ReplicaFailoverPolicy -Source $Source -Validation $Validation
    if (-not [bool]$policy.allowed) {
        return [pscustomobject]@{
            changed = $false
            details = @(
                "policy_rule=" + $policy.rule,
                "policy_scope=" + $policy.scope,
                "policy_reason=" + $policy.reason,
                "policy_slo_gate=" + $policy.slo_gate,
                "policy_drill_gate=" + $policy.drill_gate,
                "policy_risk_gate=" + $policy.risk_gate,
                "policy_effective_score=" + $policy.effective_score,
                "policy_drill_pass_rate=" + $policy.drill_pass_rate,
                "policy_drill_average_score=" + $policy.drill_average_score
            )
            skipped = ("policy_blocked rule=" + $policy.rule + " scope=" + $policy.scope + " reason=" + $policy.reason + " slo_gate=" + $policy.slo_gate + " drill_gate=" + $policy.drill_gate + " risk_gate=" + $policy.risk_gate)
            policy_rule = [string]$policy.rule
            policy_reason = [string]$policy.reason
        }
    }
    $details += ("policy_rule=" + $policy.rule)
    $details += ("policy_scope=" + $policy.scope)
    $now = Now-Ms
    $cooldownSec = [Math]::Max(1, [int]$policy.cooldown_seconds)
    if ($script:StateReplicaLastFailoverMs -gt 0) {
        $elapsedMs = $now - [int64]$script:StateReplicaLastFailoverMs
        if ($elapsedMs -lt ([int64]$cooldownSec * 1000)) {
            return [pscustomobject]@{
                changed = $false
                details = $details
                skipped = ("cooldown_sec=" + $cooldownSec)
                policy_rule = [string]$policy.rule
                policy_reason = [string]$policy.reason
            }
        }
    }
    $changed = $false
    $snapCheck = Validate-ReplicaSet -Kind "snapshot" -PrimaryPath $script:StateSnapshotPath -ReplicaPaths $script:StateSnapshotReplicaPaths -AllowedLagEntries 0
    if (-not $snapCheck.ok) {
        $snapSource = Select-SnapshotFailoverSource -PrimaryPath $script:StateSnapshotPath -ReplicaPaths $script:StateSnapshotReplicaPaths
        if (-not [string]::IsNullOrWhiteSpace($snapSource)) {
            $synced = Sync-StateFileSetFromSource -SourcePath $snapSource -PrimaryPath $script:StateSnapshotPath -ReplicaPaths $script:StateSnapshotReplicaPaths
            if ($synced) {
                $changed = $true
                $details += ("snapshot_failover_source=" + $snapSource)
            }
        }
    }
    $replayCheck = Validate-ReplicaSet -Kind "replay" -PrimaryPath $script:StateReplayPath -ReplicaPaths $script:StateReplayReplicaPaths -AllowedLagEntries $AllowedLagEntries
    if (-not $replayCheck.ok) {
        $replaySource = Select-ReplayFailoverSource -PrimaryPath $script:StateReplayPath -ReplicaPaths $script:StateReplayReplicaPaths
        if (-not [string]::IsNullOrWhiteSpace($replaySource)) {
            $syncedReplay = Sync-StateFileSetFromSource -SourcePath $replaySource -PrimaryPath $script:StateReplayPath -ReplicaPaths $script:StateReplayReplicaPaths
            if ($syncedReplay) {
                $changed = $true
                $details += ("replay_failover_source=" + $replaySource)
            }
        }
    }
    if ($changed) {
        $script:StateReplicaLastFailoverMs = [int64](Now-Ms)
        $script:StateReplicaFailoverCount = [int]$script:StateReplicaFailoverCount + 1
        $script:StateReplicaFailoverMode = $true
        $script:StateReplicaStableCycles = 0
    }
    return [pscustomobject]@{
        changed = $changed
        details = $details
        skipped = ""
        policy_rule = [string]$policy.rule
        policy_reason = [string]$policy.reason
    }
}

function Try-SwitchbackStateReplicas {
    param([object]$Validation)
    if (-not $script:StateRecoveryEnabled -or -not $script:StateReplicaSwitchbackEnabled) {
        return [pscustomobject]@{
            changed = $false
            details = @()
            skipped = "switchback disabled"
        }
    }
    if (-not $script:StateReplicaFailoverMode) {
        return [pscustomobject]@{
            changed = $false
            details = @()
            skipped = "not in failover mode"
        }
    }
    if (-not [bool]$Validation.ok) {
        $script:StateReplicaStableCycles = 0
        return [pscustomobject]@{
            changed = $false
            details = @()
            skipped = "validation not ok"
        }
    }
    $script:StateReplicaStableCycles = [int]$script:StateReplicaStableCycles + 1
    if ($script:StateReplicaStableCycles -lt $script:StateReplicaSwitchbackStableCycles) {
        return [pscustomobject]@{
            changed = $false
            details = @()
            skipped = ("waiting stable cycles " + $script:StateReplicaStableCycles + "/" + $script:StateReplicaSwitchbackStableCycles)
        }
    }
    $snapOk = Sync-StateFileSetFromSource -SourcePath $script:StateSnapshotPath -PrimaryPath $script:StateSnapshotPath -ReplicaPaths $script:StateSnapshotReplicaPaths
    $repOk = Sync-StateFileSetFromSource -SourcePath $script:StateReplayPath -PrimaryPath $script:StateReplayPath -ReplicaPaths $script:StateReplayReplicaPaths
    if (-not $snapOk -or -not $repOk) {
        return [pscustomobject]@{
            changed = $false
            details = @()
            skipped = "switchback sync failed"
        }
    }
    $script:StateReplicaFailoverMode = $false
    $script:StateReplicaStableCycles = 0
    return [pscustomobject]@{
        changed = $true
        details = @("switchback_source=primary")
        skipped = ""
    }
}

function Emit-ReplicaDrill {
    param(
        [string]$AuditPath,
        [string]$QueuePath,
        [string]$PlanAction,
        [string]$ControlOpId,
        [string]$ControllerId
    )
    if (-not $script:StateRecoveryEnabled -or -not $script:StateReplicaDrillEnabled) {
        return
    }
    if ([string]::IsNullOrWhiteSpace($script:StateReplicaDrillId)) {
        $script:StateReplicaDrillId = ("drill-" + $ControlOpId)
    }
    $snapSrc = Select-SnapshotFailoverSource -PrimaryPath $script:StateSnapshotPath -ReplicaPaths $script:StateSnapshotReplicaPaths
    $repSrc = Select-ReplayFailoverSource -PrimaryPath $script:StateReplayPath -ReplicaPaths $script:StateReplayReplicaPaths
    $ok = (-not [string]::IsNullOrWhiteSpace($snapSrc)) -and (-not [string]::IsNullOrWhiteSpace($repSrc))
    $result = "replica_drill_ok"
    $err = ""
    $drillScore = [pscustomobject]@{
        score = 0.0
        grade = "red"
        pass = $false
        pass_rate = 0.0
        average_score = 0.0
        total = 0
    }
    if ($script:StateReplicaDrillScoreEnabled) {
        $drillScore = Update-ReplicaDrillScoreState -State $script:StateReplicaDrillScoreState -SnapshotSource $snapSrc -ReplaySource $repSrc
        Save-ReplicaDrillScoreState -Path $script:StateReplicaDrillScorePath -State $script:StateReplicaDrillScoreState
    }
    if (-not $ok) {
        $result = "replica_drill_error"
        $err = "missing failover source candidate"
    } elseif ($script:StateReplicaDrillScoreEnabled -and -not [bool]$drillScore.pass) {
        $result = "replica_drill_warn"
        $err = ("drill score below pass threshold: score=" + $drillScore.score + " pass_score=" + $script:StateReplicaDrillPassScore)
    }
    Write-Host ("rollout_control_replica_drill: drill_id={0} result={1} snapshot_source={2} replay_source={3} score={4} grade={5} pass_rate={6}" -f $script:StateReplicaDrillId, $result, $snapSrc, $repSrc, $drillScore.score, $drillScore.grade, $drillScore.pass_rate)
    Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
            timestamp_utc = [DateTime]::UtcNow.ToString("o")
            control_operation_id = $ControlOpId
            controller_id = $ControllerId
            queue_file = $QueuePath
            action = $PlanAction
            result = $result
            drill_id = $script:StateReplicaDrillId
            snapshot_source = $snapSrc
            replay_source = $repSrc
            drill_score = $drillScore.score
            drill_grade = $drillScore.grade
            drill_pass = [bool]$drillScore.pass
            drill_pass_rate = $drillScore.pass_rate
            drill_average_score = $drillScore.average_score
            drill_total = $drillScore.total
            error = $err
        })
}

function Cleanup-SiteConsensusState {
    param(
        [pscustomobject]$State,
        [int]$VoteTtlSec
    )
    $now = Now-Ms
    foreach ($k in @($State.entries.Keys)) {
        $item = $State.entries[$k]
        foreach ($site in @($item.votes.Keys)) {
            $age = $now - [int64]$item.votes[$site].updated_unix_ms
            if ($age -gt ([int64]$VoteTtlSec * 1000)) {
                $null = $item.votes.Remove($site)
            }
        }
        if ($item.votes.Count -eq 0 -and [string]::IsNullOrWhiteSpace([string]$item.committed_operation_id)) {
            $null = $State.entries.Remove($k)
        }
    }
}

function Site-Priority {
    param([string]$Site)
    $base = Site-BasePriority -Site $Site
    if (-not $script:SiteConflictAccountabilityEnabled) {
        return $base
    }
    if ($null -eq $script:SiteConflictAccountabilityState -or $null -eq $script:SiteConflictAccountabilityState.sites) {
        return $base
    }
    if (-not $script:SiteConflictAccountabilityState.sites.ContainsKey($Site)) {
        return $base
    }
    $penalty = [Math]::Max(0, [int]$script:SiteConflictAccountabilityState.sites[$Site].penalty_points)
    $effective = $base - $penalty
    if ($effective -lt 0) { $effective = 0 }
    return $effective
}

function Try-SiteConsensus {
    param(
        [object]$Entry,
        [string]$ControlOpId
    )
    $emptyEvents = @()
    if (-not $script:SiteConsensusEnabled) {
        return [pscustomobject]@{
            ok = $true
            status = "disabled"
            reason = ""
            winner_site_id = ""
            winner_operation_id = ""
            accountability_events = $emptyEvents
        }
    }
    $key = Dedupe-Key -Entry $Entry
    Cleanup-SiteConsensusState -State $script:SiteConsensusState -VoteTtlSec $script:SiteConsensusVoteTtlSec
    if (-not $script:SiteConsensusState.entries.ContainsKey($key)) {
        $script:SiteConsensusState.entries[$key] = [pscustomobject]@{
            committed_operation_id = ""
            committed_site_id = ""
            committed_unix_ms = 0
            votes = @{}
        }
    }
    $item = $script:SiteConsensusState.entries[$key]
    if (-not [string]::IsNullOrWhiteSpace([string]$item.committed_operation_id)) {
        if ([string]$item.committed_operation_id -eq $ControlOpId -and [string]$item.committed_site_id -eq $script:SiteId) {
            return [pscustomobject]@{
                ok = $true
                status = "already_committed_self"
                reason = ""
                winner_site_id = [string]$item.committed_site_id
                winner_operation_id = [string]$item.committed_operation_id
                accountability_events = $emptyEvents
            }
        }
        $reason = ("committed to operation_id=" + [string]$item.committed_operation_id + " site_id=" + [string]$item.committed_site_id)
        $accEvents = @()
        if ($script:SiteConflictAccountabilityEnabled) {
            $accEvents += Apply-SiteConflictPenaltyEvent -Site $script:SiteId -Event "consensus_committed_other" -Role "self" -Reason ("key=" + $key + " " + $reason)
            Save-SiteConflictAccountabilityState -Path $script:SiteConflictAccountabilityPath -State $script:SiteConflictAccountabilityState
        }
        return [pscustomobject]@{
            ok = $false
            status = "committed_other"
            reason = $reason
            winner_site_id = [string]$item.committed_site_id
            winner_operation_id = [string]$item.committed_operation_id
            accountability_events = $accEvents
        }
    }
    $item.votes[$script:SiteId] = [pscustomobject]@{
        operation_id = $ControlOpId
        priority = Site-Priority -Site $script:SiteId
        updated_unix_ms = [int64](Now-Ms)
    }
    $activeSites = @($item.votes.Keys)
    if ($activeSites.Count -lt $script:SiteConsensusRequiredSites) {
        Save-SiteConsensusState -Path $script:SiteConsensusPath -State $script:SiteConsensusState
        return [pscustomobject]@{
            ok = $false
            status = "waiting_quorum"
            reason = ("site_votes=" + $activeSites.Count + " required_sites=" + $script:SiteConsensusRequiredSites)
            winner_site_id = ""
            winner_operation_id = ""
            accountability_events = $emptyEvents
        }
    }
    $winner = $null
    $eligibleSites = @()
    $riskBlockedSites = @()
    foreach ($site in $activeSites) {
        $winnerRiskSetSel = Select-RiskBlockedSet -Role "winner_guard" -SiteId $site
        if ($script:SiteConflictRiskWinnerGuardEnabled -and (Is-SiteConflictRiskBlocked -Site $site -BlockedLevels $winnerRiskSetSel.set)) {
            $riskBlockedSites += $site
            continue
        }
        $eligibleSites += $site
    }
    $winnerByRiskFallback = $false
    if ($eligibleSites.Count -eq 0) {
        if ($script:SiteConflictRiskWinnerGuardEnabled -and $script:SiteConflictRiskWinnerFallbackAllow) {
            $eligibleSites = @($activeSites)
            $winnerByRiskFallback = $true
        } else {
            Save-SiteConsensusState -Path $script:SiteConsensusPath -State $script:SiteConsensusState
            if ($script:SiteConflictAccountabilityEnabled) {
                $null = Apply-SiteConflictPenaltyEvent -Site $script:SiteId -Event "consensus_risk_blocked" -Role "self" -Reason ("key=" + $key + " all candidates blocked by risk guard")
                Save-SiteConflictAccountabilityState -Path $script:SiteConflictAccountabilityPath -State $script:SiteConflictAccountabilityState
            }
            return [pscustomobject]@{
                ok = $false
                status = "risk_blocked"
                reason = ("all candidate sites blocked by risk guard: " + ($riskBlockedSites -join ","))
                winner_site_id = ""
                winner_operation_id = ""
                accountability_events = @()
            }
        }
    }
    foreach ($site in $eligibleSites) {
        $v = $item.votes[$site]
        if ($null -eq $winner) {
            $winner = [pscustomobject]@{
                site_id = $site
                operation_id = [string]$v.operation_id
                priority = [int]$v.priority
                updated_unix_ms = [int64]$v.updated_unix_ms
            }
            continue
        }
        $candPriority = [int]$v.priority
        $winPriority = [int]$winner.priority
        if ($candPriority -gt $winPriority) {
            $winner = [pscustomobject]@{
                site_id = $site
                operation_id = [string]$v.operation_id
                priority = $candPriority
                updated_unix_ms = [int64]$v.updated_unix_ms
            }
            continue
        }
        if ($candPriority -eq $winPriority) {
            $candOp = [string]$v.operation_id
            $winOp = [string]$winner.operation_id
            if ($candOp -lt $winOp) {
                $winner = [pscustomobject]@{
                    site_id = $site
                    operation_id = $candOp
                    priority = $candPriority
                    updated_unix_ms = [int64]$v.updated_unix_ms
                }
            }
        }
    }
    $votesSnapshot = @{}
    foreach ($site in $activeSites) {
        $votesSnapshot[$site] = $item.votes[$site]
    }
    $accEvents = Apply-SiteConsensusAccountability -Key $key -Winner $winner -Votes $votesSnapshot
    $item.committed_operation_id = [string]$winner.operation_id
    $item.committed_site_id = [string]$winner.site_id
    $item.committed_unix_ms = [int64](Now-Ms)
    $item.votes = @{}
    Save-SiteConsensusState -Path $script:SiteConsensusPath -State $script:SiteConsensusState
    if ([string]$winner.operation_id -eq $ControlOpId -and [string]$winner.site_id -eq $script:SiteId) {
        return [pscustomobject]@{
            ok = $true
            status = "committed_self"
            reason = ""
            winner_site_id = [string]$winner.site_id
            winner_operation_id = [string]$winner.operation_id
            accountability_events = $accEvents
        }
    }
    return [pscustomobject]@{
        ok = $false
        status = "committed_other"
        reason = (if ($winnerByRiskFallback) { "winner selected by risk fallback, operation_id=" + [string]$winner.operation_id + " site_id=" + [string]$winner.site_id } else { "winner operation_id=" + [string]$winner.operation_id + " site_id=" + [string]$winner.site_id })
        winner_site_id = [string]$winner.site_id
        winner_operation_id = [string]$winner.operation_id
        accountability_events = $accEvents
    }
}

function Build-Entry {
    param(
        [string]$RepoRoot,
        [object]$Plan,
        [string]$BaseAction,
        [string]$GlobalTarget,
        [string]$GlobalRollback,
        [string]$DefaultController,
        [string]$DefaultOpId,
        [string]$DefaultAudit,
        [int]$DefaultPreemptRequeue
    )
    $name = [string]$Plan.name
    if ([string]::IsNullOrWhiteSpace($name)) {
        throw "plan.name is required"
    }
    $planFileRaw = [string]$Plan.plan_file
    if ([string]::IsNullOrWhiteSpace($planFileRaw)) {
        throw ("plan_file is required, plan=" + $name)
    }
    $planFile = Resolve-FullPath -Root $RepoRoot -Value $planFileRaw

    $action = $BaseAction
    if ($null -ne $Plan.action -and -not [string]::IsNullOrWhiteSpace([string]$Plan.action)) {
        $action = [string]$Plan.action
    }

    $target = $GlobalTarget
    if ([string]::IsNullOrWhiteSpace($target) -and $null -ne $Plan.target_version) {
        $target = [string]$Plan.target_version
    }
    $rollback = $GlobalRollback
    if ([string]::IsNullOrWhiteSpace($rollback) -and $null -ne $Plan.rollback_version) {
        $rollback = [string]$Plan.rollback_version
    }
    if ($action -eq "upgrade" -and [string]::IsNullOrWhiteSpace($target)) {
        throw ("upgrade plan missing target_version, plan=" + $name)
    }

    $controller = $DefaultController
    if ($null -ne $Plan.controller_id -and -not [string]::IsNullOrWhiteSpace([string]$Plan.controller_id)) {
        $controller = [string]$Plan.controller_id
    }
    $opid = $DefaultOpId + "-" + $name
    if ($null -ne $Plan.operation_id -and -not [string]::IsNullOrWhiteSpace([string]$Plan.operation_id)) {
        $opid = [string]$Plan.operation_id
    }
    $audit = $DefaultAudit
    if ($null -ne $Plan.audit_file -and -not [string]::IsNullOrWhiteSpace([string]$Plan.audit_file)) {
        $audit = [string]$Plan.audit_file
    }
    $auditFile = Resolve-FullPath -Root $RepoRoot -Value $audit

    $region = "default"
    if ($null -ne $Plan.region -and -not [string]::IsNullOrWhiteSpace([string]$Plan.region)) {
        $region = [string]$Plan.region
    }
    $priority = 100
    if ($null -ne $Plan.priority) {
        $priority = [int]$Plan.priority
    }
    $preemptible = $true
    if ($null -ne $Plan.preemptible) {
        $preemptible = [bool]$Plan.preemptible
    }
    $preemptRequeue = $DefaultPreemptRequeue
    if ($null -ne $Plan.preempt_requeue_seconds) {
        $preemptRequeue = [Math]::Max(1, [int]$Plan.preempt_requeue_seconds)
    }
    $retryMax = 0
    if ($null -ne $Plan.retry_max_attempts) {
        $retryMax = [Math]::Max(0, [int]$Plan.retry_max_attempts)
    }
    $retryBackoffSec = 5
    if ($null -ne $Plan.retry_backoff_seconds) {
        $retryBackoffSec = [Math]::Max(1, [int]$Plan.retry_backoff_seconds)
    }
    $retryBackoffFactor = 2
    if ($null -ne $Plan.retry_backoff_factor) {
        $retryBackoffFactor = [Math]::Max(1, [int]$Plan.retry_backoff_factor)
    }
    $overlayRouteMode = ""
    if ($null -ne $Plan.overlay_route_mode -and -not [string]::IsNullOrWhiteSpace([string]$Plan.overlay_route_mode)) {
        $overlayRouteMode = ([string]$Plan.overlay_route_mode).Trim().ToLowerInvariant()
    }
    $overlayRouteRuntimeFile = ""
    if ($null -ne $Plan.overlay_route_runtime_file -and -not [string]::IsNullOrWhiteSpace([string]$Plan.overlay_route_runtime_file)) {
        $overlayRouteRuntimeFile = Resolve-FullPath -Root $RepoRoot -Value ([string]$Plan.overlay_route_runtime_file)
    }
    $overlayRouteRuntimeProfile = ""
    if ($null -ne $Plan.overlay_route_runtime_profile -and -not [string]::IsNullOrWhiteSpace([string]$Plan.overlay_route_runtime_profile)) {
        $overlayRouteRuntimeProfile = [string]$Plan.overlay_route_runtime_profile
    }
    $overlayRouteAutoProfileEnabled = $false
    if ($null -ne $Plan.overlay_route_auto_profile_enabled) {
        $overlayRouteAutoProfileEnabled = Convert-ToBooleanLoose -Value $Plan.overlay_route_auto_profile_enabled -Default $false
    }
    $overlayRouteAutoProfileStateFile = ""
    if ($null -ne $Plan.overlay_route_auto_profile_state_file -and -not [string]::IsNullOrWhiteSpace([string]$Plan.overlay_route_auto_profile_state_file)) {
        $overlayRouteAutoProfileStateFile = Resolve-FullPath -Root $RepoRoot -Value ([string]$Plan.overlay_route_auto_profile_state_file)
    }
    $overlayRouteAutoProfileProfiles = ""
    if ($null -ne $Plan.overlay_route_auto_profile_profiles -and -not [string]::IsNullOrWhiteSpace([string]$Plan.overlay_route_auto_profile_profiles)) {
        $overlayRouteAutoProfileProfiles = ([string]$Plan.overlay_route_auto_profile_profiles).Trim()
    }
    $overlayRouteAutoProfileMinHoldSeconds = ""
    if ($null -ne $Plan.overlay_route_auto_profile_min_hold_seconds) {
        $overlayRouteAutoProfileMinHoldSeconds = [string]([Math]::Max(1, [int]$Plan.overlay_route_auto_profile_min_hold_seconds))
    }
    $overlayRouteAutoProfileSwitchMargin = ""
    if ($null -ne $Plan.overlay_route_auto_profile_switch_margin) {
        $overlayRouteAutoProfileSwitchMarginValue = [double]$Plan.overlay_route_auto_profile_switch_margin
        $overlayRouteAutoProfileSwitchMargin = [string]([Math]::Min(1, [Math]::Max(0, $overlayRouteAutoProfileSwitchMarginValue)))
    }
    $overlayRouteAutoProfileSwitchbackCooldownSeconds = ""
    if ($null -ne $Plan.overlay_route_auto_profile_switchback_cooldown_seconds) {
        $overlayRouteAutoProfileSwitchbackCooldownSeconds = [string]([Math]::Max(1, [int]$Plan.overlay_route_auto_profile_switchback_cooldown_seconds))
    }
    $overlayRouteAutoProfileRecheckSeconds = ""
    if ($null -ne $Plan.overlay_route_auto_profile_recheck_seconds) {
        $overlayRouteAutoProfileRecheckSeconds = [string]([Math]::Max(1, [int]$Plan.overlay_route_auto_profile_recheck_seconds))
    }
    $overlayRouteAutoProfileBinaryPath = ""
    if ($null -ne $Plan.overlay_route_auto_profile_binary_path -and -not [string]::IsNullOrWhiteSpace([string]$Plan.overlay_route_auto_profile_binary_path)) {
        $overlayRouteAutoProfileBinaryPath = Resolve-FullPath -Root $RepoRoot -Value ([string]$Plan.overlay_route_auto_profile_binary_path)
    }
    $overlayRouteRuntimeProfileObj = Load-OverlayRouteRuntimeProfile -RuntimeFile $overlayRouteRuntimeFile -Profile $overlayRouteRuntimeProfile
    $overlayRouteRelayDirectoryFile = ""
    if ($null -ne $Plan.overlay_route_relay_directory_file -and -not [string]::IsNullOrWhiteSpace([string]$Plan.overlay_route_relay_directory_file)) {
        $overlayRouteRelayDirectoryFile = Resolve-FullPath -Root $RepoRoot -Value ([string]$Plan.overlay_route_relay_directory_file)
    }
    $overlayRouteRelayHealthMin = ""
    if ($null -ne $Plan.overlay_route_relay_health_min) {
        $overlayRouteRelayHealthMinValue = [double]$Plan.overlay_route_relay_health_min
        $overlayRouteRelayHealthMin = [string]([Math]::Min(1, [Math]::Max(0, $overlayRouteRelayHealthMinValue)))
    }
    $overlayRouteRelayPenaltyStateFile = ""
    if ($null -ne $Plan.overlay_route_relay_penalty_state_file -and -not [string]::IsNullOrWhiteSpace([string]$Plan.overlay_route_relay_penalty_state_file)) {
        $overlayRouteRelayPenaltyStateFile = Resolve-FullPath -Root $RepoRoot -Value ([string]$Plan.overlay_route_relay_penalty_state_file)
    }
    $overlayRouteRelayPenaltyDelta = ""
    if ($null -ne $Plan.overlay_route_relay_penalty_delta) {
        if ($Plan.overlay_route_relay_penalty_delta -is [string]) {
            if (-not [string]::IsNullOrWhiteSpace([string]$Plan.overlay_route_relay_penalty_delta)) {
                $overlayRouteRelayPenaltyDelta = [string]$Plan.overlay_route_relay_penalty_delta
            }
        } else {
            $overlayRouteRelayPenaltyDelta = ($Plan.overlay_route_relay_penalty_delta | ConvertTo-Json -Depth 32 -Compress)
        }
    }
    $overlayRouteRelayPenaltyRecoverPerRun = ""
    if ($null -ne $Plan.overlay_route_relay_penalty_recover_per_run) {
        $overlayRouteRelayPenaltyRecoverPerRunValue = [double]$Plan.overlay_route_relay_penalty_recover_per_run
        $overlayRouteRelayPenaltyRecoverPerRun = [string]([Math]::Min(1, [Math]::Max(0, $overlayRouteRelayPenaltyRecoverPerRunValue)))
    }
    $overlayRouteAutoPenaltyEnabled = $false
    if ($null -ne $Plan.overlay_route_auto_penalty_enabled) {
        $overlayRouteAutoPenaltyEnabled = Convert-ToBooleanLoose -Value $Plan.overlay_route_auto_penalty_enabled -Default $false
    } elseif ($null -ne $overlayRouteRuntimeProfileObj) {
        $runtimeAutoPenaltyEnabled = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "auto_penalty_enabled"
        if ($null -eq $runtimeAutoPenaltyEnabled) {
            $runtimeAutoPenaltyEnabled = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "relay_auto_penalty_enabled"
        }
        if ($null -ne $runtimeAutoPenaltyEnabled) {
            $overlayRouteAutoPenaltyEnabled = Convert-ToBooleanLoose -Value $runtimeAutoPenaltyEnabled -Default $overlayRouteAutoPenaltyEnabled
        }
    }
    $overlayRouteAutoPenaltyStep = 0.2
    if ($null -ne $Plan.overlay_route_auto_penalty_step) {
        $overlayRouteAutoPenaltyStepValue = [double]$Plan.overlay_route_auto_penalty_step
        $overlayRouteAutoPenaltyStep = [Math]::Min(1, [Math]::Max(0, $overlayRouteAutoPenaltyStepValue))
    } elseif ($null -ne $overlayRouteRuntimeProfileObj) {
        $runtimeAutoPenaltyStep = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "auto_penalty_step"
        if ($null -eq $runtimeAutoPenaltyStep) {
            $runtimeAutoPenaltyStep = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "relay_auto_penalty_step"
        }
        if ($null -ne $runtimeAutoPenaltyStep) {
            $runtimeAutoPenaltyStepValue = 0.0
            if ([double]::TryParse([string]$runtimeAutoPenaltyStep, [ref]$runtimeAutoPenaltyStepValue)) {
                $overlayRouteAutoPenaltyStep = [Math]::Min(1, [Math]::Max(0, $runtimeAutoPenaltyStepValue))
            }
        }
    }
    $overlayRouteRelayHealthRefreshEnabled = $false
    if ($null -ne $Plan.overlay_route_relay_health_refresh_enabled) {
        $overlayRouteRelayHealthRefreshEnabled = Convert-ToBooleanLoose -Value $Plan.overlay_route_relay_health_refresh_enabled -Default $false
    } elseif ($null -ne $overlayRouteRuntimeProfileObj) {
        $runtimeRefreshEnabled = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "relay_health_refresh_enabled"
        if ($null -eq $runtimeRefreshEnabled) {
            $runtimeRefreshEnabled = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "health_refresh_enabled"
        }
        if ($null -ne $runtimeRefreshEnabled) {
            $overlayRouteRelayHealthRefreshEnabled = Convert-ToBooleanLoose -Value $runtimeRefreshEnabled -Default $overlayRouteRelayHealthRefreshEnabled
        }
    }
    $overlayRouteRelayHealthRefreshMode = "auto"
    if ($null -ne $Plan.overlay_route_relay_health_refresh_mode -and -not [string]::IsNullOrWhiteSpace([string]$Plan.overlay_route_relay_health_refresh_mode)) {
        $overlayRouteRelayHealthRefreshMode = ([string]$Plan.overlay_route_relay_health_refresh_mode).Trim().ToLowerInvariant()
    } elseif ($null -ne $overlayRouteRuntimeProfileObj) {
        $runtimeRefreshMode = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "relay_health_refresh_mode"
        if ($null -ne $runtimeRefreshMode -and -not [string]::IsNullOrWhiteSpace([string]$runtimeRefreshMode)) {
            $overlayRouteRelayHealthRefreshMode = ([string]$runtimeRefreshMode).Trim().ToLowerInvariant()
        }
    }
    $overlayRouteRelayHealthRefreshTimeoutMs = 800
    if ($null -ne $Plan.overlay_route_relay_health_refresh_timeout_ms) {
        $timeoutParsed = 0
        if ([int]::TryParse([string]$Plan.overlay_route_relay_health_refresh_timeout_ms, [ref]$timeoutParsed)) {
            $overlayRouteRelayHealthRefreshTimeoutMs = [Math]::Min(15000, [Math]::Max(100, $timeoutParsed))
        }
    } elseif ($null -ne $overlayRouteRuntimeProfileObj) {
        $runtimeRefreshTimeout = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "relay_health_refresh_timeout_ms"
        if ($null -eq $runtimeRefreshTimeout) {
            $runtimeRefreshTimeout = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "health_refresh_timeout_ms"
        }
        if ($null -ne $runtimeRefreshTimeout) {
            $timeoutParsed = 0
            if ([int]::TryParse([string]$runtimeRefreshTimeout, [ref]$timeoutParsed)) {
                $overlayRouteRelayHealthRefreshTimeoutMs = [Math]::Min(15000, [Math]::Max(100, $timeoutParsed))
            }
        }
    }
    $overlayRouteRelayHealthRefreshAlpha = 0.2
    if ($null -ne $Plan.overlay_route_relay_health_refresh_alpha) {
        $alphaParsed = 0.0
        if ([double]::TryParse([string]$Plan.overlay_route_relay_health_refresh_alpha, [ref]$alphaParsed)) {
            $overlayRouteRelayHealthRefreshAlpha = [Math]::Min(1, [Math]::Max(0.01, $alphaParsed))
        }
    } elseif ($null -ne $overlayRouteRuntimeProfileObj) {
        $runtimeRefreshAlpha = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "relay_health_refresh_alpha"
        if ($null -eq $runtimeRefreshAlpha) {
            $runtimeRefreshAlpha = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "health_refresh_alpha"
        }
        if ($null -ne $runtimeRefreshAlpha) {
            $alphaParsed = 0.0
            if ([double]::TryParse([string]$runtimeRefreshAlpha, [ref]$alphaParsed)) {
                $overlayRouteRelayHealthRefreshAlpha = [Math]::Min(1, [Math]::Max(0.01, $alphaParsed))
            }
        }
    }
    $overlayRouteRelayHealthRefreshCooldownSeconds = 30
    if ($null -ne $Plan.overlay_route_relay_health_refresh_cooldown_seconds) {
        $cooldownParsed = 0
        if ([int]::TryParse([string]$Plan.overlay_route_relay_health_refresh_cooldown_seconds, [ref]$cooldownParsed)) {
            $overlayRouteRelayHealthRefreshCooldownSeconds = [Math]::Min(3600, [Math]::Max(1, $cooldownParsed))
        }
    } elseif ($null -ne $overlayRouteRuntimeProfileObj) {
        $runtimeRefreshCooldown = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "relay_health_refresh_cooldown_seconds"
        if ($null -eq $runtimeRefreshCooldown) {
            $runtimeRefreshCooldown = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "health_refresh_cooldown_seconds"
        }
        if ($null -ne $runtimeRefreshCooldown) {
            $cooldownParsed = 0
            if ([int]::TryParse([string]$runtimeRefreshCooldown, [ref]$cooldownParsed)) {
                $overlayRouteRelayHealthRefreshCooldownSeconds = [Math]::Min(3600, [Math]::Max(1, $cooldownParsed))
            }
        }
    }
    $overlayRouteRelayDiscoveryEnabled = $false
    if ($null -ne $Plan.overlay_route_relay_discovery_enabled) {
        $overlayRouteRelayDiscoveryEnabled = Convert-ToBooleanLoose -Value $Plan.overlay_route_relay_discovery_enabled -Default $false
    } elseif ($null -ne $overlayRouteRuntimeProfileObj) {
        $runtimeDiscoveryEnabled = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "relay_discovery_enabled"
        if ($null -eq $runtimeDiscoveryEnabled) {
            $runtimeDiscoveryEnabled = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "discovery_enabled"
        }
        if ($null -ne $runtimeDiscoveryEnabled) {
            $overlayRouteRelayDiscoveryEnabled = Convert-ToBooleanLoose -Value $runtimeDiscoveryEnabled -Default $overlayRouteRelayDiscoveryEnabled
        }
    }
    $overlayRouteRelayDiscoveryFile = ""
    if ($null -ne $Plan.overlay_route_relay_discovery_file -and -not [string]::IsNullOrWhiteSpace([string]$Plan.overlay_route_relay_discovery_file)) {
        $overlayRouteRelayDiscoveryFile = Resolve-FullPath -Root $RepoRoot -Value ([string]$Plan.overlay_route_relay_discovery_file)
    } elseif ($null -ne $overlayRouteRuntimeProfileObj) {
        $runtimeDiscoveryFile = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "relay_discovery_file"
        if ($null -eq $runtimeDiscoveryFile) {
            $runtimeDiscoveryFile = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "discovery_file"
        }
        if ($null -ne $runtimeDiscoveryFile -and -not [string]::IsNullOrWhiteSpace([string]$runtimeDiscoveryFile)) {
            $overlayRouteRelayDiscoveryFile = Resolve-FullPath -Root $RepoRoot -Value ([string]$runtimeDiscoveryFile)
        }
    }
    $overlayRouteRelayDiscoveryCooldownSeconds = 120
    if ($null -ne $Plan.overlay_route_relay_discovery_cooldown_seconds) {
        $discoveryCooldownParsed = 0
        if ([int]::TryParse([string]$Plan.overlay_route_relay_discovery_cooldown_seconds, [ref]$discoveryCooldownParsed)) {
            $overlayRouteRelayDiscoveryCooldownSeconds = [Math]::Min(7200, [Math]::Max(1, $discoveryCooldownParsed))
        }
    } elseif ($null -ne $overlayRouteRuntimeProfileObj) {
        $runtimeDiscoveryCooldown = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "relay_discovery_cooldown_seconds"
        if ($null -eq $runtimeDiscoveryCooldown) {
            $runtimeDiscoveryCooldown = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "discovery_cooldown_seconds"
        }
        if ($null -ne $runtimeDiscoveryCooldown) {
            $discoveryCooldownParsed = 0
            if ([int]::TryParse([string]$runtimeDiscoveryCooldown, [ref]$discoveryCooldownParsed)) {
                $overlayRouteRelayDiscoveryCooldownSeconds = [Math]::Min(7200, [Math]::Max(1, $discoveryCooldownParsed))
            }
        }
    }
    $overlayRouteRelayDiscoveryDefaultHealth = 0.85
    if ($null -ne $Plan.overlay_route_relay_discovery_default_health) {
        $discoveryHealthParsed = 0.0
        if ([double]::TryParse([string]$Plan.overlay_route_relay_discovery_default_health, [ref]$discoveryHealthParsed)) {
            $overlayRouteRelayDiscoveryDefaultHealth = [Math]::Min(1, [Math]::Max(0, $discoveryHealthParsed))
        }
    } elseif ($null -ne $overlayRouteRuntimeProfileObj) {
        $runtimeDiscoveryHealth = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "relay_discovery_default_health"
        if ($null -eq $runtimeDiscoveryHealth) {
            $runtimeDiscoveryHealth = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "discovery_default_health"
        }
        if ($null -ne $runtimeDiscoveryHealth) {
            $discoveryHealthParsed = 0.0
            if ([double]::TryParse([string]$runtimeDiscoveryHealth, [ref]$discoveryHealthParsed)) {
                $overlayRouteRelayDiscoveryDefaultHealth = [Math]::Min(1, [Math]::Max(0, $discoveryHealthParsed))
            }
        }
    }
    $overlayRouteRelayDiscoveryDefaultEnabled = $true
    if ($null -ne $Plan.overlay_route_relay_discovery_default_enabled) {
        $overlayRouteRelayDiscoveryDefaultEnabled = Convert-ToBooleanLoose -Value $Plan.overlay_route_relay_discovery_default_enabled -Default $overlayRouteRelayDiscoveryDefaultEnabled
    } elseif ($null -ne $overlayRouteRuntimeProfileObj) {
        $runtimeDiscoveryDefaultEnabled = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "relay_discovery_default_enabled"
        if ($null -eq $runtimeDiscoveryDefaultEnabled) {
            $runtimeDiscoveryDefaultEnabled = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "discovery_default_enabled"
        }
        if ($null -ne $runtimeDiscoveryDefaultEnabled) {
            $overlayRouteRelayDiscoveryDefaultEnabled = Convert-ToBooleanLoose -Value $runtimeDiscoveryDefaultEnabled -Default $overlayRouteRelayDiscoveryDefaultEnabled
        }
    }
    $overlayRouteRelayDiscoveryHttpUrls = ""
    if ($null -ne $Plan.overlay_route_relay_discovery_http_urls -and -not [string]::IsNullOrWhiteSpace([string]$Plan.overlay_route_relay_discovery_http_urls)) {
        $overlayRouteRelayDiscoveryHttpUrls = [string]$Plan.overlay_route_relay_discovery_http_urls
    } elseif ($null -ne $overlayRouteRuntimeProfileObj) {
        $runtimeDiscoveryHttpUrls = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "relay_discovery_http_urls"
        if ($null -eq $runtimeDiscoveryHttpUrls) {
            $runtimeDiscoveryHttpUrls = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "discovery_http_urls"
        }
        if ($null -ne $runtimeDiscoveryHttpUrls -and -not [string]::IsNullOrWhiteSpace([string]$runtimeDiscoveryHttpUrls)) {
            $overlayRouteRelayDiscoveryHttpUrls = [string]$runtimeDiscoveryHttpUrls
        }
    }
    $overlayRouteRelayDiscoverySourceWeights = ""
    if ($null -ne $Plan.overlay_route_relay_discovery_source_weights) {
        if ($Plan.overlay_route_relay_discovery_source_weights -is [string]) {
            if (-not [string]::IsNullOrWhiteSpace([string]$Plan.overlay_route_relay_discovery_source_weights)) {
                $overlayRouteRelayDiscoverySourceWeights = [string]$Plan.overlay_route_relay_discovery_source_weights
            }
        } else {
            $overlayRouteRelayDiscoverySourceWeights = ($Plan.overlay_route_relay_discovery_source_weights | ConvertTo-Json -Depth 32 -Compress)
        }
    } elseif ($null -ne $overlayRouteRuntimeProfileObj) {
        $runtimeDiscoverySourceWeights = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "relay_discovery_source_weights"
        if ($null -eq $runtimeDiscoverySourceWeights) {
            $runtimeDiscoverySourceWeights = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "discovery_source_weights"
        }
        if ($null -ne $runtimeDiscoverySourceWeights) {
            if ($runtimeDiscoverySourceWeights -is [string]) {
                if (-not [string]::IsNullOrWhiteSpace([string]$runtimeDiscoverySourceWeights)) {
                    $overlayRouteRelayDiscoverySourceWeights = [string]$runtimeDiscoverySourceWeights
                }
            } else {
                $overlayRouteRelayDiscoverySourceWeights = ($runtimeDiscoverySourceWeights | ConvertTo-Json -Depth 32 -Compress)
            }
        }
    }
    $overlayRouteRelayDiscoveryHttpTimeoutMs = 1500
    if ($null -ne $Plan.overlay_route_relay_discovery_http_timeout_ms) {
        $discoveryHttpTimeoutParsed = 0
        if ([int]::TryParse([string]$Plan.overlay_route_relay_discovery_http_timeout_ms, [ref]$discoveryHttpTimeoutParsed)) {
            $overlayRouteRelayDiscoveryHttpTimeoutMs = [Math]::Min(20000, [Math]::Max(100, $discoveryHttpTimeoutParsed))
        }
    } elseif ($null -ne $overlayRouteRuntimeProfileObj) {
        $runtimeDiscoveryHttpTimeout = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "relay_discovery_http_timeout_ms"
        if ($null -eq $runtimeDiscoveryHttpTimeout) {
            $runtimeDiscoveryHttpTimeout = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "discovery_http_timeout_ms"
        }
        if ($null -ne $runtimeDiscoveryHttpTimeout) {
            $discoveryHttpTimeoutParsed = 0
            if ([int]::TryParse([string]$runtimeDiscoveryHttpTimeout, [ref]$discoveryHttpTimeoutParsed)) {
                $overlayRouteRelayDiscoveryHttpTimeoutMs = [Math]::Min(20000, [Math]::Max(100, $discoveryHttpTimeoutParsed))
            }
        }
    }
    $overlayRouteRelayDiscoverySourceReputationFile = ""
    if ($null -ne $Plan.overlay_route_relay_discovery_source_reputation_file -and -not [string]::IsNullOrWhiteSpace([string]$Plan.overlay_route_relay_discovery_source_reputation_file)) {
        $overlayRouteRelayDiscoverySourceReputationFile = Resolve-FullPath -Root $RepoRoot -Value ([string]$Plan.overlay_route_relay_discovery_source_reputation_file)
    } elseif ($null -ne $overlayRouteRuntimeProfileObj) {
        $runtimeSourceReputationFile = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "relay_discovery_source_reputation_file"
        if ($null -eq $runtimeSourceReputationFile) {
            $runtimeSourceReputationFile = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "discovery_source_reputation_file"
        }
        if ($null -ne $runtimeSourceReputationFile -and -not [string]::IsNullOrWhiteSpace([string]$runtimeSourceReputationFile)) {
            $overlayRouteRelayDiscoverySourceReputationFile = Resolve-FullPath -Root $RepoRoot -Value ([string]$runtimeSourceReputationFile)
        }
    }
    $overlayRouteRelayDiscoverySourceDecay = 0.05
    if ($null -ne $Plan.overlay_route_relay_discovery_source_decay) {
        $sourceDecayParsed = 0.0
        if ([double]::TryParse([string]$Plan.overlay_route_relay_discovery_source_decay, [ref]$sourceDecayParsed)) {
            $overlayRouteRelayDiscoverySourceDecay = [Math]::Min(1, [Math]::Max(0, $sourceDecayParsed))
        }
    } elseif ($null -ne $overlayRouteRuntimeProfileObj) {
        $runtimeSourceDecay = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "relay_discovery_source_decay"
        if ($null -eq $runtimeSourceDecay) {
            $runtimeSourceDecay = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "discovery_source_decay"
        }
        if ($null -ne $runtimeSourceDecay) {
            $sourceDecayParsed = 0.0
            if ([double]::TryParse([string]$runtimeSourceDecay, [ref]$sourceDecayParsed)) {
                $overlayRouteRelayDiscoverySourceDecay = [Math]::Min(1, [Math]::Max(0, $sourceDecayParsed))
            }
        }
    }
    $overlayRouteRelayDiscoverySourcePenaltyOnFail = 0.2
    if ($null -ne $Plan.overlay_route_relay_discovery_source_penalty_on_fail) {
        $sourcePenaltyParsed = 0.0
        if ([double]::TryParse([string]$Plan.overlay_route_relay_discovery_source_penalty_on_fail, [ref]$sourcePenaltyParsed)) {
            $overlayRouteRelayDiscoverySourcePenaltyOnFail = [Math]::Min(1, [Math]::Max(0, $sourcePenaltyParsed))
        }
    } elseif ($null -ne $overlayRouteRuntimeProfileObj) {
        $runtimeSourcePenalty = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "relay_discovery_source_penalty_on_fail"
        if ($null -eq $runtimeSourcePenalty) {
            $runtimeSourcePenalty = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "discovery_source_penalty_on_fail"
        }
        if ($null -ne $runtimeSourcePenalty) {
            $sourcePenaltyParsed = 0.0
            if ([double]::TryParse([string]$runtimeSourcePenalty, [ref]$sourcePenaltyParsed)) {
                $overlayRouteRelayDiscoverySourcePenaltyOnFail = [Math]::Min(1, [Math]::Max(0, $sourcePenaltyParsed))
            }
        }
    }
    $overlayRouteRelayDiscoverySourceRecoverOnSuccess = 0.03
    if ($null -ne $Plan.overlay_route_relay_discovery_source_recover_on_success) {
        $sourceRecoverParsed = 0.0
        if ([double]::TryParse([string]$Plan.overlay_route_relay_discovery_source_recover_on_success, [ref]$sourceRecoverParsed)) {
            $overlayRouteRelayDiscoverySourceRecoverOnSuccess = [Math]::Min(1, [Math]::Max(0, $sourceRecoverParsed))
        }
    } elseif ($null -ne $overlayRouteRuntimeProfileObj) {
        $runtimeSourceRecover = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "relay_discovery_source_recover_on_success"
        if ($null -eq $runtimeSourceRecover) {
            $runtimeSourceRecover = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "discovery_source_recover_on_success"
        }
        if ($null -ne $runtimeSourceRecover) {
            $sourceRecoverParsed = 0.0
            if ([double]::TryParse([string]$runtimeSourceRecover, [ref]$sourceRecoverParsed)) {
                $overlayRouteRelayDiscoverySourceRecoverOnSuccess = [Math]::Min(1, [Math]::Max(0, $sourceRecoverParsed))
            }
        }
    }
    $overlayRouteRelayDiscoverySourceBlacklistThreshold = 0.15
    if ($null -ne $Plan.overlay_route_relay_discovery_source_blacklist_threshold) {
        $sourceBlacklistParsed = 0.0
        if ([double]::TryParse([string]$Plan.overlay_route_relay_discovery_source_blacklist_threshold, [ref]$sourceBlacklistParsed)) {
            $overlayRouteRelayDiscoverySourceBlacklistThreshold = [Math]::Min(1, [Math]::Max(0, $sourceBlacklistParsed))
        }
    } elseif ($null -ne $overlayRouteRuntimeProfileObj) {
        $runtimeSourceBlacklist = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "relay_discovery_source_blacklist_threshold"
        if ($null -eq $runtimeSourceBlacklist) {
            $runtimeSourceBlacklist = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "discovery_source_blacklist_threshold"
        }
        if ($null -ne $runtimeSourceBlacklist) {
            $sourceBlacklistParsed = 0.0
            if ([double]::TryParse([string]$runtimeSourceBlacklist, [ref]$sourceBlacklistParsed)) {
                $overlayRouteRelayDiscoverySourceBlacklistThreshold = [Math]::Min(1, [Math]::Max(0, $sourceBlacklistParsed))
            }
        }
    }
    $overlayRouteRelayDiscoverySourceDenylist = ""
    if ($null -ne $Plan.overlay_route_relay_discovery_source_denylist) {
        if ($Plan.overlay_route_relay_discovery_source_denylist -is [System.Array]) {
            $denylistParts = @()
            foreach ($denyItem in $Plan.overlay_route_relay_discovery_source_denylist) {
                if ($null -eq $denyItem) { continue }
                $denyText = ([string]$denyItem).Trim()
                if (-not [string]::IsNullOrWhiteSpace($denyText)) {
                    $denylistParts += $denyText
                }
            }
            if ($denylistParts.Count -gt 0) {
                $overlayRouteRelayDiscoverySourceDenylist = ($denylistParts -join ",")
            }
        } elseif (-not [string]::IsNullOrWhiteSpace([string]$Plan.overlay_route_relay_discovery_source_denylist)) {
            $overlayRouteRelayDiscoverySourceDenylist = [string]$Plan.overlay_route_relay_discovery_source_denylist
        }
    } elseif ($null -ne $overlayRouteRuntimeProfileObj) {
        $runtimeSourceDenylist = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "relay_discovery_source_denylist"
        if ($null -eq $runtimeSourceDenylist) {
            $runtimeSourceDenylist = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "discovery_source_denylist"
        }
        if ($null -ne $runtimeSourceDenylist) {
            if ($runtimeSourceDenylist -is [System.Array]) {
                $denylistParts = @()
                foreach ($denyItem in $runtimeSourceDenylist) {
                    if ($null -eq $denyItem) { continue }
                    $denyText = ([string]$denyItem).Trim()
                    if (-not [string]::IsNullOrWhiteSpace($denyText)) {
                        $denylistParts += $denyText
                    }
                }
                if ($denylistParts.Count -gt 0) {
                    $overlayRouteRelayDiscoverySourceDenylist = ($denylistParts -join ",")
                }
            } elseif (-not [string]::IsNullOrWhiteSpace([string]$runtimeSourceDenylist)) {
                $overlayRouteRelayDiscoverySourceDenylist = [string]$runtimeSourceDenylist
            }
        }
    }
    $overlayRouteRelayDiscoveryHttpUrlsFile = ""
    if ($null -ne $Plan.overlay_route_relay_discovery_http_urls_file -and -not [string]::IsNullOrWhiteSpace([string]$Plan.overlay_route_relay_discovery_http_urls_file)) {
        $overlayRouteRelayDiscoveryHttpUrlsFile = Resolve-FullPath -Root $RepoRoot -Value ([string]$Plan.overlay_route_relay_discovery_http_urls_file)
    } elseif ($null -ne $overlayRouteRuntimeProfileObj) {
        $runtimeDiscoveryHttpUrlsFile = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "relay_discovery_http_urls_file"
        if ($null -eq $runtimeDiscoveryHttpUrlsFile) {
            $runtimeDiscoveryHttpUrlsFile = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "discovery_http_urls_file"
        }
        if ($null -ne $runtimeDiscoveryHttpUrlsFile -and -not [string]::IsNullOrWhiteSpace([string]$runtimeDiscoveryHttpUrlsFile)) {
            $overlayRouteRelayDiscoveryHttpUrlsFile = Resolve-FullPath -Root $RepoRoot -Value ([string]$runtimeDiscoveryHttpUrlsFile)
        }
    }
    $overlayRouteRelayDiscoverySeedRegion = ""
    if ($null -ne $Plan.overlay_route_relay_discovery_seed_region -and -not [string]::IsNullOrWhiteSpace([string]$Plan.overlay_route_relay_discovery_seed_region)) {
        $overlayRouteRelayDiscoverySeedRegion = ([string]$Plan.overlay_route_relay_discovery_seed_region).Trim()
    } elseif ($null -ne $overlayRouteRuntimeProfileObj) {
        $runtimeDiscoverySeedRegion = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "relay_discovery_seed_region"
        if ($null -eq $runtimeDiscoverySeedRegion) {
            $runtimeDiscoverySeedRegion = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "discovery_seed_region"
        }
        if ($null -ne $runtimeDiscoverySeedRegion -and -not [string]::IsNullOrWhiteSpace([string]$runtimeDiscoverySeedRegion)) {
            $overlayRouteRelayDiscoverySeedRegion = ([string]$runtimeDiscoverySeedRegion).Trim()
        }
    }
    $overlayRouteRelayDiscoverySeedMode = ""
    if ($null -ne $Plan.overlay_route_relay_discovery_seed_mode -and -not [string]::IsNullOrWhiteSpace([string]$Plan.overlay_route_relay_discovery_seed_mode)) {
        $overlayRouteRelayDiscoverySeedMode = ([string]$Plan.overlay_route_relay_discovery_seed_mode).Trim()
    } elseif ($null -ne $overlayRouteRuntimeProfileObj) {
        $runtimeDiscoverySeedMode = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "relay_discovery_seed_mode"
        if ($null -eq $runtimeDiscoverySeedMode) {
            $runtimeDiscoverySeedMode = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "discovery_seed_mode"
        }
        if ($null -ne $runtimeDiscoverySeedMode -and -not [string]::IsNullOrWhiteSpace([string]$runtimeDiscoverySeedMode)) {
            $overlayRouteRelayDiscoverySeedMode = ([string]$runtimeDiscoverySeedMode).Trim()
        }
    }
    $overlayRouteRelayDiscoverySeedProfile = ""
    if ($null -ne $Plan.overlay_route_relay_discovery_seed_profile -and -not [string]::IsNullOrWhiteSpace([string]$Plan.overlay_route_relay_discovery_seed_profile)) {
        $overlayRouteRelayDiscoverySeedProfile = ([string]$Plan.overlay_route_relay_discovery_seed_profile).Trim()
    } elseif ($null -ne $overlayRouteRuntimeProfileObj) {
        $runtimeDiscoverySeedProfile = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "relay_discovery_seed_profile"
        if ($null -eq $runtimeDiscoverySeedProfile) {
            $runtimeDiscoverySeedProfile = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "discovery_seed_profile"
        }
        if ($null -ne $runtimeDiscoverySeedProfile -and -not [string]::IsNullOrWhiteSpace([string]$runtimeDiscoverySeedProfile)) {
            $overlayRouteRelayDiscoverySeedProfile = ([string]$runtimeDiscoverySeedProfile).Trim()
        }
    }
    $overlayRouteRelayDiscoverySeedFailoverStateFile = ""
    if ($null -ne $Plan.overlay_route_relay_discovery_seed_failover_state_file -and -not [string]::IsNullOrWhiteSpace([string]$Plan.overlay_route_relay_discovery_seed_failover_state_file)) {
        $overlayRouteRelayDiscoverySeedFailoverStateFile = Resolve-FullPath -Root $RepoRoot -Value ([string]$Plan.overlay_route_relay_discovery_seed_failover_state_file)
    } elseif ($null -ne $overlayRouteRuntimeProfileObj) {
        $runtimeDiscoverySeedFailoverStateFile = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "relay_discovery_seed_failover_state_file"
        if ($null -eq $runtimeDiscoverySeedFailoverStateFile) {
            $runtimeDiscoverySeedFailoverStateFile = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "discovery_seed_failover_state_file"
        }
        if ($null -ne $runtimeDiscoverySeedFailoverStateFile -and -not [string]::IsNullOrWhiteSpace([string]$runtimeDiscoverySeedFailoverStateFile)) {
            $overlayRouteRelayDiscoverySeedFailoverStateFile = Resolve-FullPath -Root $RepoRoot -Value ([string]$runtimeDiscoverySeedFailoverStateFile)
        }
    }
    $overlayRouteRelayDiscoverySeedPriority = ""
    if ($null -ne $Plan.overlay_route_relay_discovery_seed_priority) {
        if ($Plan.overlay_route_relay_discovery_seed_priority -is [string]) {
            if (-not [string]::IsNullOrWhiteSpace([string]$Plan.overlay_route_relay_discovery_seed_priority)) {
                $overlayRouteRelayDiscoverySeedPriority = [string]$Plan.overlay_route_relay_discovery_seed_priority
            }
        } else {
            $overlayRouteRelayDiscoverySeedPriority = ($Plan.overlay_route_relay_discovery_seed_priority | ConvertTo-Json -Depth 32 -Compress)
        }
    } elseif ($null -ne $overlayRouteRuntimeProfileObj) {
        $runtimeDiscoverySeedPriority = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "relay_discovery_seed_priority"
        if ($null -eq $runtimeDiscoverySeedPriority) {
            $runtimeDiscoverySeedPriority = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "discovery_seed_priority"
        }
        if ($null -ne $runtimeDiscoverySeedPriority) {
            if ($runtimeDiscoverySeedPriority -is [string]) {
                if (-not [string]::IsNullOrWhiteSpace([string]$runtimeDiscoverySeedPriority)) {
                    $overlayRouteRelayDiscoverySeedPriority = [string]$runtimeDiscoverySeedPriority
                }
            } else {
                $overlayRouteRelayDiscoverySeedPriority = ($runtimeDiscoverySeedPriority | ConvertTo-Json -Depth 32 -Compress)
            }
        }
    }
    $overlayRouteRelayDiscoverySeedSuccessRateThreshold = 0.5
    if ($null -ne $Plan.overlay_route_relay_discovery_seed_success_rate_threshold) {
        $seedSuccessThresholdParsed = 0.0
        if ([double]::TryParse([string]$Plan.overlay_route_relay_discovery_seed_success_rate_threshold, [ref]$seedSuccessThresholdParsed)) {
            $overlayRouteRelayDiscoverySeedSuccessRateThreshold = [Math]::Min(1, [Math]::Max(0, $seedSuccessThresholdParsed))
        }
    } elseif ($null -ne $overlayRouteRuntimeProfileObj) {
        $runtimeDiscoverySeedSuccessThreshold = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "relay_discovery_seed_success_rate_threshold"
        if ($null -eq $runtimeDiscoverySeedSuccessThreshold) {
            $runtimeDiscoverySeedSuccessThreshold = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "discovery_seed_success_rate_threshold"
        }
        if ($null -ne $runtimeDiscoverySeedSuccessThreshold) {
            $seedSuccessThresholdParsed = 0.0
            if ([double]::TryParse([string]$runtimeDiscoverySeedSuccessThreshold, [ref]$seedSuccessThresholdParsed)) {
                $overlayRouteRelayDiscoverySeedSuccessRateThreshold = [Math]::Min(1, [Math]::Max(0, $seedSuccessThresholdParsed))
            }
        }
    }
    $overlayRouteRelayDiscoverySeedCooldownSeconds = 120
    if ($null -ne $Plan.overlay_route_relay_discovery_seed_cooldown_seconds) {
        $seedCooldownParsed = 0
        if ([int]::TryParse([string]$Plan.overlay_route_relay_discovery_seed_cooldown_seconds, [ref]$seedCooldownParsed)) {
            $overlayRouteRelayDiscoverySeedCooldownSeconds = [Math]::Min(86400, [Math]::Max(1, $seedCooldownParsed))
        }
    } elseif ($null -ne $overlayRouteRuntimeProfileObj) {
        $runtimeDiscoverySeedCooldown = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "relay_discovery_seed_cooldown_seconds"
        if ($null -eq $runtimeDiscoverySeedCooldown) {
            $runtimeDiscoverySeedCooldown = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "discovery_seed_cooldown_seconds"
        }
        if ($null -ne $runtimeDiscoverySeedCooldown) {
            $seedCooldownParsed = 0
            if ([int]::TryParse([string]$runtimeDiscoverySeedCooldown, [ref]$seedCooldownParsed)) {
                $overlayRouteRelayDiscoverySeedCooldownSeconds = [Math]::Min(86400, [Math]::Max(1, $seedCooldownParsed))
            }
        }
    }
    $overlayRouteRelayDiscoverySeedMaxConsecutiveFailures = 3
    if ($null -ne $Plan.overlay_route_relay_discovery_seed_max_consecutive_failures) {
        $seedMaxConsecutiveParsed = 0
        if ([int]::TryParse([string]$Plan.overlay_route_relay_discovery_seed_max_consecutive_failures, [ref]$seedMaxConsecutiveParsed)) {
            $overlayRouteRelayDiscoverySeedMaxConsecutiveFailures = [Math]::Min(100, [Math]::Max(1, $seedMaxConsecutiveParsed))
        }
    } elseif ($null -ne $overlayRouteRuntimeProfileObj) {
        $runtimeDiscoverySeedMaxConsecutive = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "relay_discovery_seed_max_consecutive_failures"
        if ($null -eq $runtimeDiscoverySeedMaxConsecutive) {
            $runtimeDiscoverySeedMaxConsecutive = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "discovery_seed_max_consecutive_failures"
        }
        if ($null -ne $runtimeDiscoverySeedMaxConsecutive) {
            $seedMaxConsecutiveParsed = 0
            if ([int]::TryParse([string]$runtimeDiscoverySeedMaxConsecutive, [ref]$seedMaxConsecutiveParsed)) {
                $overlayRouteRelayDiscoverySeedMaxConsecutiveFailures = [Math]::Min(100, [Math]::Max(1, $seedMaxConsecutiveParsed))
            }
        }
    }
    $overlayRouteRelayDiscoveryRegionPriority = ""
    if ($null -ne $Plan.overlay_route_relay_discovery_region_priority) {
        if ($Plan.overlay_route_relay_discovery_region_priority -is [string]) {
            if (-not [string]::IsNullOrWhiteSpace([string]$Plan.overlay_route_relay_discovery_region_priority)) {
                $overlayRouteRelayDiscoveryRegionPriority = [string]$Plan.overlay_route_relay_discovery_region_priority
            }
        } else {
            $overlayRouteRelayDiscoveryRegionPriority = ($Plan.overlay_route_relay_discovery_region_priority | ConvertTo-Json -Depth 32 -Compress)
        }
    } elseif ($null -ne $overlayRouteRuntimeProfileObj) {
        $runtimeDiscoveryRegionPriority = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "relay_discovery_region_priority"
        if ($null -eq $runtimeDiscoveryRegionPriority) {
            $runtimeDiscoveryRegionPriority = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "discovery_region_priority"
        }
        if ($null -ne $runtimeDiscoveryRegionPriority) {
            if ($runtimeDiscoveryRegionPriority -is [string]) {
                if (-not [string]::IsNullOrWhiteSpace([string]$runtimeDiscoveryRegionPriority)) {
                    $overlayRouteRelayDiscoveryRegionPriority = [string]$runtimeDiscoveryRegionPriority
                }
            } else {
                $overlayRouteRelayDiscoveryRegionPriority = ($runtimeDiscoveryRegionPriority | ConvertTo-Json -Depth 32 -Compress)
            }
        }
    }
    $overlayRouteRelayDiscoveryRegionFailoverThreshold = 0.5
    if ($null -ne $Plan.overlay_route_relay_discovery_region_failover_threshold) {
        $regionThresholdParsed = 0.0
        if ([double]::TryParse([string]$Plan.overlay_route_relay_discovery_region_failover_threshold, [ref]$regionThresholdParsed)) {
            $overlayRouteRelayDiscoveryRegionFailoverThreshold = [Math]::Min(1, [Math]::Max(0, $regionThresholdParsed))
        }
    } elseif ($null -ne $overlayRouteRuntimeProfileObj) {
        $runtimeDiscoveryRegionThreshold = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "relay_discovery_region_failover_threshold"
        if ($null -eq $runtimeDiscoveryRegionThreshold) {
            $runtimeDiscoveryRegionThreshold = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "discovery_region_failover_threshold"
        }
        if ($null -ne $runtimeDiscoveryRegionThreshold) {
            $regionThresholdParsed = 0.0
            if ([double]::TryParse([string]$runtimeDiscoveryRegionThreshold, [ref]$regionThresholdParsed)) {
                $overlayRouteRelayDiscoveryRegionFailoverThreshold = [Math]::Min(1, [Math]::Max(0, $regionThresholdParsed))
            }
        }
    }
    $overlayRouteRelayDiscoveryRegionCooldownSeconds = 120
    if ($null -ne $Plan.overlay_route_relay_discovery_region_cooldown_seconds) {
        $regionCooldownParsed = 0
        if ([int]::TryParse([string]$Plan.overlay_route_relay_discovery_region_cooldown_seconds, [ref]$regionCooldownParsed)) {
            $overlayRouteRelayDiscoveryRegionCooldownSeconds = [Math]::Min(86400, [Math]::Max(1, $regionCooldownParsed))
        }
    } elseif ($null -ne $overlayRouteRuntimeProfileObj) {
        $runtimeDiscoveryRegionCooldown = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "relay_discovery_region_cooldown_seconds"
        if ($null -eq $runtimeDiscoveryRegionCooldown) {
            $runtimeDiscoveryRegionCooldown = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "discovery_region_cooldown_seconds"
        }
        if ($null -ne $runtimeDiscoveryRegionCooldown) {
            $regionCooldownParsed = 0
            if ([int]::TryParse([string]$runtimeDiscoveryRegionCooldown, [ref]$regionCooldownParsed)) {
                $overlayRouteRelayDiscoveryRegionCooldownSeconds = [Math]::Min(86400, [Math]::Max(1, $regionCooldownParsed))
            }
        }
    }
    $overlayRouteRelayDiscoveryRelayScoreSmoothingAlpha = 0.3
    if ($null -ne $Plan.overlay_route_relay_discovery_relay_score_smoothing_alpha) {
        $relayScoreSmoothingAlphaParsed = 0.0
        if ([double]::TryParse([string]$Plan.overlay_route_relay_discovery_relay_score_smoothing_alpha, [ref]$relayScoreSmoothingAlphaParsed)) {
            $overlayRouteRelayDiscoveryRelayScoreSmoothingAlpha = [Math]::Min(1, [Math]::Max(0.01, $relayScoreSmoothingAlphaParsed))
        }
    } elseif ($null -ne $overlayRouteRuntimeProfileObj) {
        $runtimeDiscoveryRelayScoreSmoothingAlpha = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "relay_discovery_relay_score_smoothing_alpha"
        if ($null -eq $runtimeDiscoveryRelayScoreSmoothingAlpha) {
            $runtimeDiscoveryRelayScoreSmoothingAlpha = Get-ObjectPropertyValueCI -Object $overlayRouteRuntimeProfileObj -Name "discovery_relay_score_smoothing_alpha"
        }
        if ($null -ne $runtimeDiscoveryRelayScoreSmoothingAlpha) {
            $relayScoreSmoothingAlphaParsed = 0.0
            if ([double]::TryParse([string]$runtimeDiscoveryRelayScoreSmoothingAlpha, [ref]$relayScoreSmoothingAlphaParsed)) {
                $overlayRouteRelayDiscoveryRelayScoreSmoothingAlpha = [Math]::Min(1, [Math]::Max(0.01, $relayScoreSmoothingAlphaParsed))
            }
        }
    }
    $overlayRouteRelayCandidatesByRegion = ""
    if ($null -ne $Plan.overlay_route_relay_candidates_by_region) {
        $overlayRouteRelayCandidatesByRegion = ($Plan.overlay_route_relay_candidates_by_region | ConvertTo-Json -Depth 32 -Compress)
    }
    $overlayRouteRelayCandidatesByRole = ""
    if ($null -ne $Plan.overlay_route_relay_candidates_by_role) {
        $overlayRouteRelayCandidatesByRole = ($Plan.overlay_route_relay_candidates_by_role | ConvertTo-Json -Depth 32 -Compress)
    }
    $overlayRouteRelayCandidates = ""
    if ($null -ne $Plan.overlay_route_relay_candidates) {
        if ($Plan.overlay_route_relay_candidates -is [System.Array]) {
            $candidateList = @()
            foreach ($candidate in $Plan.overlay_route_relay_candidates) {
                if ($null -ne $candidate) {
                    $candidateText = ([string]$candidate).Trim()
                    if (-not [string]::IsNullOrWhiteSpace($candidateText)) {
                        $candidateList += $candidateText
                    }
                }
            }
            if ($candidateList.Count -gt 0) {
                $overlayRouteRelayCandidates = ($candidateList -join ",")
            }
        } elseif (-not [string]::IsNullOrWhiteSpace([string]$Plan.overlay_route_relay_candidates)) {
            $overlayRouteRelayCandidates = [string]$Plan.overlay_route_relay_candidates
        }
    }

    return [pscustomobject]@{
        plan = $Plan
        name = $name
        action = $action
        plan_file = $planFile
        target = $target
        rollback = $rollback
        controller = $controller
        opid = $opid
        audit_file = $auditFile
        region = $region
        priority = $priority
        preemptible = $preemptible
        preempt_requeue = $preemptRequeue
        retry_max = $retryMax
        retry_backoff_sec = $retryBackoffSec
        retry_backoff_factor = $retryBackoffFactor
        overlay_route_mode = $overlayRouteMode
        overlay_route_runtime_file = $overlayRouteRuntimeFile
        overlay_route_runtime_profile = $overlayRouteRuntimeProfile
        overlay_route_relay_directory_file = $overlayRouteRelayDirectoryFile
        overlay_route_relay_health_min = $overlayRouteRelayHealthMin
        overlay_route_relay_penalty_state_file = $overlayRouteRelayPenaltyStateFile
        overlay_route_relay_penalty_delta = $overlayRouteRelayPenaltyDelta
        overlay_route_relay_penalty_recover_per_run = $overlayRouteRelayPenaltyRecoverPerRun
        overlay_route_auto_penalty_enabled = $overlayRouteAutoPenaltyEnabled
        overlay_route_auto_penalty_step = $overlayRouteAutoPenaltyStep
        overlay_route_relay_health_refresh_enabled = $overlayRouteRelayHealthRefreshEnabled
        overlay_route_relay_health_refresh_mode = $overlayRouteRelayHealthRefreshMode
        overlay_route_relay_health_refresh_timeout_ms = $overlayRouteRelayHealthRefreshTimeoutMs
        overlay_route_relay_health_refresh_alpha = $overlayRouteRelayHealthRefreshAlpha
        overlay_route_relay_health_refresh_cooldown_seconds = $overlayRouteRelayHealthRefreshCooldownSeconds
        overlay_route_relay_discovery_enabled = $overlayRouteRelayDiscoveryEnabled
        overlay_route_relay_discovery_file = $overlayRouteRelayDiscoveryFile
        overlay_route_relay_discovery_cooldown_seconds = $overlayRouteRelayDiscoveryCooldownSeconds
        overlay_route_relay_discovery_default_health = $overlayRouteRelayDiscoveryDefaultHealth
        overlay_route_relay_discovery_default_enabled = $overlayRouteRelayDiscoveryDefaultEnabled
        overlay_route_relay_discovery_http_urls = $overlayRouteRelayDiscoveryHttpUrls
        overlay_route_relay_discovery_source_weights = $overlayRouteRelayDiscoverySourceWeights
        overlay_route_relay_discovery_http_timeout_ms = $overlayRouteRelayDiscoveryHttpTimeoutMs
        overlay_route_relay_discovery_source_reputation_file = $overlayRouteRelayDiscoverySourceReputationFile
        overlay_route_relay_discovery_source_decay = $overlayRouteRelayDiscoverySourceDecay
        overlay_route_relay_discovery_source_penalty_on_fail = $overlayRouteRelayDiscoverySourcePenaltyOnFail
        overlay_route_relay_discovery_source_recover_on_success = $overlayRouteRelayDiscoverySourceRecoverOnSuccess
        overlay_route_relay_discovery_source_blacklist_threshold = $overlayRouteRelayDiscoverySourceBlacklistThreshold
        overlay_route_relay_discovery_source_denylist = $overlayRouteRelayDiscoverySourceDenylist
        overlay_route_relay_discovery_http_urls_file = $overlayRouteRelayDiscoveryHttpUrlsFile
        overlay_route_relay_discovery_seed_region = $overlayRouteRelayDiscoverySeedRegion
        overlay_route_relay_discovery_seed_mode = $overlayRouteRelayDiscoverySeedMode
        overlay_route_relay_discovery_seed_profile = $overlayRouteRelayDiscoverySeedProfile
        overlay_route_relay_discovery_seed_failover_state_file = $overlayRouteRelayDiscoverySeedFailoverStateFile
        overlay_route_relay_discovery_seed_priority = $overlayRouteRelayDiscoverySeedPriority
        overlay_route_relay_discovery_seed_success_rate_threshold = $overlayRouteRelayDiscoverySeedSuccessRateThreshold
        overlay_route_relay_discovery_seed_cooldown_seconds = $overlayRouteRelayDiscoverySeedCooldownSeconds
        overlay_route_relay_discovery_seed_max_consecutive_failures = $overlayRouteRelayDiscoverySeedMaxConsecutiveFailures
        overlay_route_relay_discovery_region_priority = $overlayRouteRelayDiscoveryRegionPriority
        overlay_route_relay_discovery_region_failover_threshold = $overlayRouteRelayDiscoveryRegionFailoverThreshold
        overlay_route_relay_discovery_region_cooldown_seconds = $overlayRouteRelayDiscoveryRegionCooldownSeconds
        overlay_route_relay_discovery_relay_score_smoothing_alpha = $overlayRouteRelayDiscoveryRelayScoreSmoothingAlpha
        overlay_route_relay_candidates = $overlayRouteRelayCandidates
        overlay_route_relay_candidates_by_region = $overlayRouteRelayCandidatesByRegion
        overlay_route_relay_candidates_by_role = $overlayRouteRelayCandidatesByRole
        overlay_route_auto_profile_enabled = $overlayRouteAutoProfileEnabled
        overlay_route_auto_profile_state_file = $overlayRouteAutoProfileStateFile
        overlay_route_auto_profile_profiles = $overlayRouteAutoProfileProfiles
        overlay_route_auto_profile_min_hold_seconds = $overlayRouteAutoProfileMinHoldSeconds
        overlay_route_auto_profile_switch_margin = $overlayRouteAutoProfileSwitchMargin
        overlay_route_auto_profile_switchback_cooldown_seconds = $overlayRouteAutoProfileSwitchbackCooldownSeconds
        overlay_route_auto_profile_recheck_seconds = $overlayRouteAutoProfileRecheckSeconds
        overlay_route_auto_profile_binary_path = $overlayRouteAutoProfileBinaryPath
        attempt = 1
        next_run = 0
    }
}
function Start-Entry {
    param(
        [string]$RepoRoot,
        [string]$RolloutScript,
        [object]$Entry
    )
    $args = @(
        "rollout",
        "--action", [string]$Entry.action,
        "--plan-file", [string]$Entry.plan_file,
        "--controller-id", [string]$Entry.controller,
        "--operation-id", [string]$Entry.opid,
        "--audit-file", [string]$Entry.audit_file
    )
    if (-not [string]::IsNullOrWhiteSpace([string]$Entry.target)) {
        $args += @("--target-version", [string]$Entry.target)
    }
    if (-not [string]::IsNullOrWhiteSpace([string]$Entry.rollback)) {
        $args += @("--rollback-version", [string]$Entry.rollback)
    }
    if (-not [string]::IsNullOrWhiteSpace([string]$Entry.overlay_route_mode)) {
        $args += @("--overlay-route-mode", [string]$Entry.overlay_route_mode)
    }
    if (-not [string]::IsNullOrWhiteSpace([string]$Entry.overlay_route_runtime_file)) {
        $args += @("--overlay-route-runtime-file", [string]$Entry.overlay_route_runtime_file)
    }
    if (-not [string]::IsNullOrWhiteSpace([string]$Entry.overlay_route_runtime_profile)) {
        $args += @("--overlay-route-runtime-profile", [string]$Entry.overlay_route_runtime_profile)
    }
    if (-not [string]::IsNullOrWhiteSpace([string]$Entry.overlay_route_relay_directory_file)) {
        $args += @("--overlay-route-relay-directory-file", [string]$Entry.overlay_route_relay_directory_file)
    }
    if (-not [string]::IsNullOrWhiteSpace([string]$Entry.overlay_route_relay_health_min)) {
        $args += @("--overlay-route-relay-health-min", [string]$Entry.overlay_route_relay_health_min)
    }
    if (-not [string]::IsNullOrWhiteSpace([string]$Entry.overlay_route_relay_penalty_state_file)) {
        $args += @("--overlay-route-relay-penalty-state-file", [string]$Entry.overlay_route_relay_penalty_state_file)
    }
    if (-not [string]::IsNullOrWhiteSpace([string]$Entry.overlay_route_relay_penalty_delta)) {
        $args += @("--overlay-route-relay-penalty-delta", [string]$Entry.overlay_route_relay_penalty_delta)
    }
    if (-not [string]::IsNullOrWhiteSpace([string]$Entry.overlay_route_relay_penalty_recover_per_run)) {
        $args += @("--overlay-route-relay-penalty-recover-per-run", [string]$Entry.overlay_route_relay_penalty_recover_per_run)
    }
    if (-not [string]::IsNullOrWhiteSpace([string]$Entry.overlay_route_relay_candidates)) {
        $args += @("--overlay-route-relay-candidates", [string]$Entry.overlay_route_relay_candidates)
    }
    if (-not [string]::IsNullOrWhiteSpace([string]$Entry.overlay_route_relay_candidates_by_region)) {
        $args += @("--overlay-route-relay-candidates-by-region", [string]$Entry.overlay_route_relay_candidates_by_region)
    }
    if (-not [string]::IsNullOrWhiteSpace([string]$Entry.overlay_route_relay_candidates_by_role)) {
        $args += @("--overlay-route-relay-candidates-by-role", [string]$Entry.overlay_route_relay_candidates_by_role)
    }
    if ($null -ne $Entry.overlay_route_auto_profile_enabled -and [bool]$Entry.overlay_route_auto_profile_enabled) {
        $args += "--auto-profile-enabled"
    }
    if (-not [string]::IsNullOrWhiteSpace([string]$Entry.overlay_route_auto_profile_state_file)) {
        $args += @("--auto-profile-state-file", [string]$Entry.overlay_route_auto_profile_state_file)
    }
    if (-not [string]::IsNullOrWhiteSpace([string]$Entry.overlay_route_auto_profile_profiles)) {
        $args += @("--auto-profile-profiles", [string]$Entry.overlay_route_auto_profile_profiles)
    }
    if (-not [string]::IsNullOrWhiteSpace([string]$Entry.overlay_route_auto_profile_min_hold_seconds)) {
        $args += @("--auto-profile-min-hold-seconds", [string]$Entry.overlay_route_auto_profile_min_hold_seconds)
    }
    if (-not [string]::IsNullOrWhiteSpace([string]$Entry.overlay_route_auto_profile_switch_margin)) {
        $args += @("--auto-profile-switch-margin", [string]$Entry.overlay_route_auto_profile_switch_margin)
    }
    if (-not [string]::IsNullOrWhiteSpace([string]$Entry.overlay_route_auto_profile_switchback_cooldown_seconds)) {
        $args += @("--auto-profile-switchback-cooldown-seconds", [string]$Entry.overlay_route_auto_profile_switchback_cooldown_seconds)
    }
    if (-not [string]::IsNullOrWhiteSpace([string]$Entry.overlay_route_auto_profile_recheck_seconds)) {
        $args += @("--auto-profile-recheck-seconds", [string]$Entry.overlay_route_auto_profile_recheck_seconds)
    }
    if (-not [string]::IsNullOrWhiteSpace([string]$Entry.overlay_route_auto_profile_binary_path)) {
        $args += @("--policy-cli-binary-file", [string]$Entry.overlay_route_auto_profile_binary_path)
    }

    $plan = $Entry.plan
    if ($null -ne $plan.upgrade_health_seconds) {
        $args += @("--upgrade-health-seconds", [string][int]$plan.upgrade_health_seconds)
    }
    if ($null -ne $plan.default_max_failures) {
        $args += @("--default-max-failures", [string][int]$plan.default_max_failures)
    }
    if ($null -ne $plan.pause_seconds_between_nodes) {
        $args += @("--pause-seconds-between-nodes", [string][int]$plan.pause_seconds_between_nodes)
    }
    if ($null -ne $plan.default_transport -and -not [string]::IsNullOrWhiteSpace([string]$plan.default_transport)) {
        $args += @("--default-transport", [string]$plan.default_transport)
    }
    if ($null -ne $plan.ssh_binary -and -not [string]::IsNullOrWhiteSpace([string]$plan.ssh_binary)) {
        $args += @("--ssh-binary", [string]$plan.ssh_binary)
    }
    if ($null -ne $plan.ssh_identity_file -and -not [string]::IsNullOrWhiteSpace([string]$plan.ssh_identity_file)) {
        $args += @("--ssh-identity-file", [string]$plan.ssh_identity_file)
    }
    if ($null -ne $plan.ssh_known_hosts_file -and -not [string]::IsNullOrWhiteSpace([string]$plan.ssh_known_hosts_file)) {
        $args += @("--ssh-known-hosts-file", [string]$plan.ssh_known_hosts_file)
    }
    if ($null -ne $plan.ssh_strict_host_key -and -not [string]::IsNullOrWhiteSpace([string]$plan.ssh_strict_host_key)) {
        $args += @("--ssh-strict-host-key-checking", [string]$plan.ssh_strict_host_key)
    }
    if ($null -ne $plan.winrm_cred_user_env -and -not [string]::IsNullOrWhiteSpace([string]$plan.winrm_cred_user_env)) {
        $args += @("--win-rm-credential-user-env", [string]$plan.winrm_cred_user_env)
    }
    if ($null -ne $plan.winrm_cred_pass_env -and -not [string]::IsNullOrWhiteSpace([string]$plan.winrm_cred_pass_env)) {
        $args += @("--win-rm-credential-password-env", [string]$plan.winrm_cred_pass_env)
    }
    if ($null -ne $plan.remote_timeout_seconds) {
        $args += @("--remote-timeout-seconds", [string][int]$plan.remote_timeout_seconds)
    }
    if ($null -ne $plan.auto_rollback_on_failure -and [bool]$plan.auto_rollback_on_failure) {
        $args += "--auto-rollback-on-failure"
    }
    if ($null -ne $plan.continue_on_failure -and [bool]$plan.continue_on_failure) {
        $args += "--continue-on-failure"
    }
    if ($null -ne $plan.ignore_upgrade_window -and [bool]$plan.ignore_upgrade_window) {
        $args += "--ignore-upgrade-window"
    }
    if ($DryRun) {
        $args += "--dry-run"
    }

    $logRoot = Join-Path $RepoRoot "artifacts/runtime/rollout/control-plane-logs"
    New-Item -ItemType Directory -Force -Path $logRoot | Out-Null
    $stdout = Join-Path $logRoot (([string]$Entry.opid) + ".stdout.log")
    $stderr = Join-Path $logRoot (([string]$Entry.opid) + ".stderr.log")

    $proc = Start-Process -FilePath $RolloutScript -ArgumentList $args -WorkingDirectory $RepoRoot -RedirectStandardOutput $stdout -RedirectStandardError $stderr -PassThru -NoNewWindow -ErrorAction Stop

    $job = Clone-Entry -Source $Entry -Attempt ([int]$Entry.attempt) -NextRun ([int64]$Entry.next_run)
    $job | Add-Member -NotePropertyName process -NotePropertyValue $proc -Force
    $job | Add-Member -NotePropertyName stdout_log -NotePropertyValue $stdout -Force
    $job | Add-Member -NotePropertyName stderr_log -NotePropertyValue $stderr -Force
    $job | Add-Member -NotePropertyName started -NotePropertyValue (Now-Ms) -Force
    return $job
}

$RepoRoot = Resolve-RootPath -Root ""
$RolloutScript = Resolve-NovovmCtlBinary
if (-not (Test-Path -LiteralPath $RolloutScript)) {
    throw ("novovmctl not found: " + $RolloutScript)
}

$QueuePath = Resolve-FullPath -Root $RepoRoot -Value $QueueFile
if (-not (Test-Path -LiteralPath $QueuePath)) {
    throw ("rollout queue not found: " + $QueuePath)
}
$queueRaw = Get-Content -LiteralPath $QueuePath -Raw
if ([string]::IsNullOrWhiteSpace($queueRaw)) {
    throw ("rollout queue is empty: " + $QueuePath)
}
$queue = $queueRaw | ConvertFrom-Json -ErrorAction Stop
if ($null -eq $queue.plans -or $queue.plans.Count -eq 0) {
    throw ("rollout queue has no plans: " + $QueuePath)
}
$script:RiskPolicyRequestedProfile = "base"
$script:RiskPolicyActiveProfile = "base"
$script:RiskPolicyProfileResolved = $true
$script:RiskPolicyHotReloadEnabled = $true
$script:RiskPolicyHotReloadCheckSeconds = 2
$script:RiskPolicyQueueLastWriteTicks = 0
$script:RiskPolicyNextReloadCheckMs = 0
$script:FailoverPolicyMatrixBySiteRaw = $null
$script:FailoverPolicyMatrixByRegionRaw = $null
$script:FailoverSloLinkBySiteRaw = $null
$script:FailoverSloLinkByRegionRaw = $null
$script:FailoverDrillLinkBySiteRaw = $null
$script:FailoverDrillLinkByRegionRaw = $null
$script:FailoverRiskLinkBySiteRaw = $null
$script:FailoverRiskLinkByRegionRaw = $null
Apply-RiskPolicyConfigFromQueue -RiskPolicy $queue.risk_policy
Apply-RiskLevelSetRuntimeConfig -RepoRoot $RepoRoot
Apply-RiskActionMatrixBuildRuntimeConfig -RepoRoot $RepoRoot
Apply-FailoverPolicyMatrixBuildRuntimeConfig -RepoRoot $RepoRoot
try {
    $script:RiskPolicyQueueLastWriteTicks = [int64](Get-Item -LiteralPath $QueuePath -ErrorAction Stop).LastWriteTimeUtc.Ticks
} catch {
    $script:RiskPolicyQueueLastWriteTicks = 0
}
$script:RiskPolicyNextReloadCheckMs = (Now-Ms) + ([int64]$script:RiskPolicyHotReloadCheckSeconds * 1000)
if ($null -ne $queue.state_recovery -and $null -ne $queue.state_recovery.failover_policy) {
    if ($null -ne $queue.state_recovery.failover_policy.site_matrix_overrides) {
        $script:FailoverPolicyMatrixBySiteRaw = $queue.state_recovery.failover_policy.site_matrix_overrides
    }
    if ($null -ne $queue.state_recovery.failover_policy.region_matrix_overrides) {
        $script:FailoverPolicyMatrixByRegionRaw = $queue.state_recovery.failover_policy.region_matrix_overrides
    }
    if ($null -ne $queue.state_recovery.failover_policy.slo_link) {
        if ($null -ne $queue.state_recovery.failover_policy.slo_link.site_overrides) { $script:FailoverSloLinkBySiteRaw = $queue.state_recovery.failover_policy.slo_link.site_overrides }
        if ($null -ne $queue.state_recovery.failover_policy.slo_link.region_overrides) { $script:FailoverSloLinkByRegionRaw = $queue.state_recovery.failover_policy.slo_link.region_overrides }
    }
    if ($null -ne $queue.state_recovery.failover_policy.drill_link) {
        if ($null -ne $queue.state_recovery.failover_policy.drill_link.site_overrides) { $script:FailoverDrillLinkBySiteRaw = $queue.state_recovery.failover_policy.drill_link.site_overrides }
        if ($null -ne $queue.state_recovery.failover_policy.drill_link.region_overrides) { $script:FailoverDrillLinkByRegionRaw = $queue.state_recovery.failover_policy.drill_link.region_overrides }
    }
    if ($null -ne $queue.state_recovery.failover_policy.risk_link) {
        if ($null -ne $queue.state_recovery.failover_policy.risk_link.site_overrides) { $script:FailoverRiskLinkBySiteRaw = $queue.state_recovery.failover_policy.risk_link.site_overrides }
        if ($null -ne $queue.state_recovery.failover_policy.risk_link.region_overrides) { $script:FailoverRiskLinkByRegionRaw = $queue.state_recovery.failover_policy.risk_link.region_overrides }
    }
}

$ControlOpId = Resolve-OperationId -Raw $OperationId
$AuditPath = Resolve-FullPath -Root $RepoRoot -Value $AuditFile
$LeasePath = Resolve-FullPath -Root $RepoRoot -Value $ControllerLeaseFile
$script:DedupePath = Resolve-FullPath -Root $RepoRoot -Value $DedupeFile
$script:DedupeTtlSec = $DedupeTtlSeconds
Apply-DecisionDashboardExportRuntimeConfig -RepoRoot $RepoRoot -AuditPathDefault $AuditPath
$script:DecisionDashboardExportNextRunMs = 0
Apply-DecisionDashboardConsumerRuntimeConfig -RepoRoot $RepoRoot
$script:DecisionDashboardConsumerNextRunMs = 0
Apply-DecisionRouteRuntimeConfig -RepoRoot $RepoRoot
Apply-RiskBlockedMapBuildRuntimeConfig -RepoRoot $RepoRoot
Apply-RiskBlockedSelectRuntimeConfig -RepoRoot $RepoRoot
Apply-RiskMatrixSelectRuntimeConfig -RepoRoot $RepoRoot
Apply-RiskActionEvalRuntimeConfig -RepoRoot $RepoRoot
Apply-RiskActionMatrixBuildRuntimeConfig -RepoRoot $RepoRoot
Apply-FailoverPolicyMatrixBuildRuntimeConfig -RepoRoot $RepoRoot
Apply-DecisionDeliveryRuntimeConfig -RepoRoot $RepoRoot
Apply-RolloutPolicyCliRuntimeConfig -RepoRoot $RepoRoot
Apply-RolloutPolicyCliOverrides

$effectiveConcurrent = $MaxConcurrentPlans
if ($null -ne $queue.max_concurrent_plans) { $effectiveConcurrent = [Math]::Max(1, [int]$queue.max_concurrent_plans) }
$effectivePoll = $PollSeconds
if ($null -ne $queue.poll_seconds) { $effectivePoll = [Math]::Max(1, [int]$queue.poll_seconds) }
$effectivePause = $DispatchPauseSeconds
if ($null -ne $queue.dispatch_pause_seconds) { $effectivePause = [Math]::Max(0, [int]$queue.dispatch_pause_seconds) }
$effectivePreempt = $EnablePriorityPreemption
if ($null -ne $queue.enable_priority_preemption) { $effectivePreempt = [bool]$queue.enable_priority_preemption }
$effectivePreemptRequeue = [Math]::Max(1, $PreemptRequeueSeconds)
if ($null -ne $queue.preempt_requeue_seconds) { $effectivePreemptRequeue = [Math]::Max(1, [int]$queue.preempt_requeue_seconds) }
$regionCaps = Get-RegionCaps -Queue $queue -Fallback $effectiveConcurrent

$effectiveLeaseTtl = $LeaseTtlSeconds
$effectiveLeaseHeartbeat = $LeaseHeartbeatSeconds
$effectiveAllowStandbyTakeover = [bool]$AllowStandbyTakeover
$primaryId = ""
$standbyIds = @()
if ($null -ne $queue.controller_governance) {
    if ($null -ne $queue.controller_governance.primary_id -and -not [string]::IsNullOrWhiteSpace([string]$queue.controller_governance.primary_id)) { $primaryId = [string]$queue.controller_governance.primary_id }
    if ($null -ne $queue.controller_governance.standby_ids) {
        foreach ($id in $queue.controller_governance.standby_ids) {
            if (-not [string]::IsNullOrWhiteSpace([string]$id)) { $standbyIds += [string]$id }
        }
    }
    if ($null -ne $queue.controller_governance.allow_standby_takeover) { $effectiveAllowStandbyTakeover = [bool]$queue.controller_governance.allow_standby_takeover }
    if ($null -ne $queue.controller_governance.lease_ttl_seconds) { $effectiveLeaseTtl = [Math]::Max(5, [int]$queue.controller_governance.lease_ttl_seconds) }
    if ($null -ne $queue.controller_governance.lease_heartbeat_seconds) { $effectiveLeaseHeartbeat = [Math]::Max(1, [int]$queue.controller_governance.lease_heartbeat_seconds) }
    if ($null -ne $queue.controller_governance.lease_file -and -not [string]::IsNullOrWhiteSpace([string]$queue.controller_governance.lease_file)) { $LeasePath = Resolve-FullPath -Root $RepoRoot -Value ([string]$queue.controller_governance.lease_file) }
    if ($null -ne $queue.controller_governance.dedupe_file -and -not [string]::IsNullOrWhiteSpace([string]$queue.controller_governance.dedupe_file)) { $script:DedupePath = Resolve-FullPath -Root $RepoRoot -Value ([string]$queue.controller_governance.dedupe_file) }
    if ($null -ne $queue.controller_governance.dedupe_ttl_seconds) { $script:DedupeTtlSec = [Math]::Max(5, [int]$queue.controller_governance.dedupe_ttl_seconds) }
}

$controllerRole = "standalone"
if (-not [string]::IsNullOrWhiteSpace($primaryId)) {
    if ($ControllerId -eq $primaryId) {
        $controllerRole = "primary"
    } elseif ($standbyIds -contains $ControllerId) {
        $controllerRole = "standby"
    } else {
        throw ("controller is not in governance set: controller_id=" + $ControllerId)
    }
}
if ($controllerRole -eq "standby" -and -not $effectiveAllowStandbyTakeover) {
    throw ("standby takeover is disabled, controller_id=" + $ControllerId)
}

$script:AdaptiveEnabled = [bool]$EnableAdaptivePolicy
$script:AdaptiveAlpha = $AdaptiveAlpha
$script:AdaptiveHighFailureRate = $AdaptiveHighFailureRate
$script:AdaptiveLowFailureRate = $AdaptiveLowFailureRate
$script:AdaptiveMaxCapBoost = [Math]::Max(0, $AdaptiveMaxCapBoost)
$script:AdaptiveStatePath = Resolve-FullPath -Root $RepoRoot -Value $AdaptiveStateFile
if ($null -ne $queue.adaptive_policy) {
    if ($null -ne $queue.adaptive_policy.enabled) { $script:AdaptiveEnabled = [bool]$queue.adaptive_policy.enabled }
    if ($null -ne $queue.adaptive_policy.alpha) { $script:AdaptiveAlpha = [double]$queue.adaptive_policy.alpha }
    if ($null -ne $queue.adaptive_policy.high_failure_rate) { $script:AdaptiveHighFailureRate = [double]$queue.adaptive_policy.high_failure_rate }
    if ($null -ne $queue.adaptive_policy.low_failure_rate) { $script:AdaptiveLowFailureRate = [double]$queue.adaptive_policy.low_failure_rate }
    if ($null -ne $queue.adaptive_policy.max_cap_boost) { $script:AdaptiveMaxCapBoost = [Math]::Max(0, [int]$queue.adaptive_policy.max_cap_boost) }
    if ($null -ne $queue.adaptive_policy.state_file -and -not [string]::IsNullOrWhiteSpace([string]$queue.adaptive_policy.state_file)) { $script:AdaptiveStatePath = Resolve-FullPath -Root $RepoRoot -Value ([string]$queue.adaptive_policy.state_file) }
}
if ($script:AdaptiveLowFailureRate -gt $script:AdaptiveHighFailureRate) {
    $tmp = $script:AdaptiveLowFailureRate
    $script:AdaptiveLowFailureRate = $script:AdaptiveHighFailureRate
    $script:AdaptiveHighFailureRate = $tmp
}
$script:AdaptiveState = New-AdaptiveState
$script:AdaptiveDirty = $false
if ($script:AdaptiveEnabled) {
    $script:AdaptiveState = Load-AdaptiveState -Path $script:AdaptiveStatePath
}

$script:StateRecoveryEnabled = [bool]$EnableStateRecovery
$script:StateSnapshotPath = Resolve-FullPath -Root $RepoRoot -Value $StateSnapshotFile
$script:StateReplayPath = Resolve-FullPath -Root $RepoRoot -Value $StateReplayFile
$script:StateSnapshotReplicaPaths = Resolve-FullPathList -Root $RepoRoot -Values $StateSnapshotReplicaFiles
$script:StateReplayReplicaPaths = Resolve-FullPathList -Root $RepoRoot -Values $StateReplayReplicaFiles
$script:StateReplayMaxEntries = [Math]::Max(100, $StateReplayMaxEntries)
$script:StateReplicaValidationEnabled = [bool]$EnableStateReplicaValidation
$script:StateReplicaValidationIntervalSec = [Math]::Max(1, $StateReplicaValidationIntervalSeconds)
$script:StateReplicaAllowedLagEntries = [Math]::Max(0, $StateReplicaAllowedLagEntries)
$script:StateReplicaAutoFailoverEnabled = [bool]$EnableStateReplicaAutoFailover
$script:StateReplicaHealthPath = Resolve-FullPath -Root $RepoRoot -Value $StateReplicaHealthFile
$script:StateReplicaFailoverCooldownSec = [Math]::Max(1, $StateReplicaFailoverCooldownSeconds)
$script:StateReplicaFailoverPolicyEnabled = [bool]$EnableStateReplicaFailoverPolicy
$script:StateReplicaFailoverPolicyDefaultAllow = [bool]$StateReplicaFailoverPolicyDefaultAllow
$script:StateReplicaFailoverSloLinkEnabled = [bool]$EnableStateReplicaFailoverSloLink
$script:StateReplicaFailoverSloMinEffectiveScore = $StateReplicaFailoverSloMinEffectiveScore
$script:StateReplicaFailoverSloBlockOnViolation = [bool]$StateReplicaFailoverSloBlockOnViolation
$script:StateReplicaFailoverDrillLinkEnabled = [bool]$EnableStateReplicaFailoverDrillLink
$script:StateReplicaFailoverDrillMinPassRate = $StateReplicaFailoverDrillMinPassRate
$script:StateReplicaFailoverDrillMinAverageScore = $StateReplicaFailoverDrillMinAverageScore
$script:StateReplicaFailoverDrillRequireLastPass = [bool]$StateReplicaFailoverDrillRequireLastPass
$script:StateReplicaFailoverRiskLinkEnabled = [bool]$EnableStateReplicaFailoverRiskLink
$script:StateReplicaFailoverRiskBlockedRaw = if ($null -ne $script:RoleRiskFailoverBlockedRaw) { $script:RoleRiskFailoverBlockedRaw } else { $script:UnifiedRiskBlockedRaw }
$script:StateReplicaFailoverRiskBlockedSet = @{}
$script:StateReplicaFailoverRiskBlockedSetBySite = @{}
$script:StateReplicaFailoverRiskBlockedSetByRegion = @{}
$script:StateReplicaFailoverPolicyRawMatrix = @()
$script:StateReplicaFailoverPolicyMatrix = @()
$script:StateReplicaFailoverPolicyMatrixBySite = @{}
$script:StateReplicaFailoverPolicyMatrixByRegion = @{}
$script:StateReplicaFailoverConvergeEnabled = $true
$script:StateReplicaFailoverConvergeMaxConcurrent = 1
$script:StateReplicaFailoverConvergeMinPauseSeconds = [Math]::Max(1, $StateReplicaFailoverCooldownSeconds)
$script:StateReplicaFailoverConvergeBlockOnSnapshotRed = $true
$script:StateReplicaFailoverConvergeBlockOnReplayRed = $true
$script:StateReplicaFailoverConvergeProfileGlobal = [pscustomobject]@{ enabled = $true; max_concurrent_plans = 1; min_dispatch_pause_seconds = [Math]::Max(1, $StateReplicaFailoverCooldownSeconds); block_on_snapshot_red = $true; block_on_replay_red = $true }
$script:StateReplicaFailoverConvergeProfileBySite = @{}
$script:StateReplicaFailoverConvergeProfileByRegion = @{}
$script:FailoverConvergeBySiteRaw = @{}
$script:FailoverConvergeByRegionRaw = @{}
$script:StateReplicaConvergeLastSignature = ""
$script:StateReplicaFailoverSloLinkProfileGlobal = [pscustomobject]@{ enabled = $false; min_effective_score = 0.0; block_on_violation = $false }
$script:StateReplicaFailoverSloLinkProfileBySite = @{}
$script:StateReplicaFailoverSloLinkProfileByRegion = @{}
$script:StateReplicaFailoverDrillLinkProfileGlobal = [pscustomobject]@{ enabled = $false; min_pass_rate = 0.0; min_average_score = 0.0; require_last_pass = $false }
$script:StateReplicaFailoverDrillLinkProfileBySite = @{}
$script:StateReplicaFailoverDrillLinkProfileByRegion = @{}
$script:StateReplicaFailoverRiskLinkProfileGlobal = [pscustomobject]@{ enabled = $false }
$script:StateReplicaFailoverRiskLinkProfileBySite = @{}
$script:StateReplicaFailoverRiskLinkProfileByRegion = @{}
$script:StateReplicaFailoverPolicyLastRule = "none"
$script:StateReplicaFailoverPolicyLastScope = "global"
$script:StateReplicaFailoverPolicyLastAllowed = $true
$script:StateReplicaFailoverPolicyLastReason = ""
$script:StateReplicaFailoverPolicyLastCooldownSec = [Math]::Max(1, $StateReplicaFailoverCooldownSeconds)
$script:StateReplicaFailoverPolicyLastSloGate = "init"
$script:StateReplicaFailoverPolicyLastDrillGate = "init"
$script:StateReplicaFailoverPolicyLastRiskGate = "init"
$script:StateReplicaFailoverOnStartup = [bool]$StateReplicaFailoverOnStartup
$script:StateReplicaSwitchbackEnabled = [bool]$EnableStateReplicaSwitchback
$script:StateReplicaSwitchbackStableCycles = [Math]::Max(1, $StateReplicaSwitchbackStableCycles)
$script:StateReplicaDrillEnabled = [bool]$EnableStateReplicaDrill
$script:StateReplicaDrillId = $StateReplicaDrillId
$script:StateReplicaDrillScoreEnabled = [bool]$EnableStateReplicaDrillScore
$script:StateReplicaDrillScorePath = Resolve-FullPath -Root $RepoRoot -Value $StateReplicaDrillScoreFile
$script:StateReplicaDrillScoreWindowSamples = [Math]::Max(1, $StateReplicaDrillScoreWindowSamples)
$script:StateReplicaDrillPassScore = $StateReplicaDrillPassScore
$script:StateReplicaDrillScoreState = New-ReplicaDrillScoreState
$script:StateReplicaSloEnabled = [bool]$EnableStateReplicaSlo
$script:StateReplicaSloPath = Resolve-FullPath -Root $RepoRoot -Value $StateReplicaSloFile
$script:StateReplicaSloWindowSamples = [Math]::Max(5, $StateReplicaSloWindowSamples)
$script:StateReplicaSloMinGreenRate = $StateReplicaSloMinGreenRate
$script:StateReplicaSloMaxRedInWindow = [Math]::Max(0, $StateReplicaSloMaxRedInWindow)
$script:StateReplicaSloBlockOnViolation = [bool]$StateReplicaSloBlockOnViolation
$script:StateReplicaSloState = New-ReplicaSloState
$script:StateReplicaAdaptiveEnabled = [bool]$EnableStateReplicaAdaptiveThreshold
$script:StateReplicaAdaptivePath = Resolve-FullPath -Root $RepoRoot -Value $StateReplicaAdaptiveFile
$script:StateReplicaAdaptiveStep = [Math]::Max(0.1, $StateReplicaAdaptiveStep)
$script:StateReplicaAdaptiveGoodScore = $StateReplicaAdaptiveGoodScore
$script:StateReplicaAdaptiveBadScore = $StateReplicaAdaptiveBadScore
$script:StateReplicaAdaptiveMaxShift = [Math]::Max(0.0, $StateReplicaAdaptiveMaxShift)
$script:StateReplicaAdaptiveState = New-ReplicaAdaptiveState
$script:StateReplicaCircuitEnabled = [bool]$EnableStateReplicaCircuitBreaker
$script:StateReplicaCircuitYellowMaxConcurrent = [Math]::Max(1, $StateReplicaYellowMaxConcurrentPlans)
$script:StateReplicaCircuitYellowPauseSeconds = [Math]::Max(0, $StateReplicaYellowDispatchPauseSeconds)
$script:StateReplicaCircuitRedBlock = [bool]$StateReplicaCircuitRedBlock
$script:StateReplicaCircuitBaseConcurrent = $effectiveConcurrent
$script:StateReplicaCircuitBasePauseSeconds = $effectivePause
$script:StateReplicaCircuitCurrentGrade = "green"
$script:StateReplicaCircuitCurrentConcurrentCap = $effectiveConcurrent
$script:StateReplicaCircuitCurrentPauseSeconds = $effectivePause
$script:StateReplicaCircuitCurrentRule = "base"
$script:StateReplicaCircuitCurrentBlockDispatch = $false
$script:StateReplicaCircuitRawMatrix = @()
$script:StateReplicaCircuitMatrix = @()
$script:StateReplicaLastFailoverMs = 0
$script:StateReplicaFailoverCount = 0
$script:StateReplicaFailoverMode = $false
$script:StateReplicaStableCycles = 0
$script:ResumeFromSnapshot = [bool]$ResumeFromSnapshot
$script:ReplayConflictsOnStart = [bool]$ReplayConflictsOnStart
if ($null -ne $queue.state_recovery) {
    if ($null -ne $queue.state_recovery.enabled) { $script:StateRecoveryEnabled = [bool]$queue.state_recovery.enabled }
    if ($null -ne $queue.state_recovery.snapshot_file -and -not [string]::IsNullOrWhiteSpace([string]$queue.state_recovery.snapshot_file)) { $script:StateSnapshotPath = Resolve-FullPath -Root $RepoRoot -Value ([string]$queue.state_recovery.snapshot_file) }
    if ($null -ne $queue.state_recovery.replay_file -and -not [string]::IsNullOrWhiteSpace([string]$queue.state_recovery.replay_file)) { $script:StateReplayPath = Resolve-FullPath -Root $RepoRoot -Value ([string]$queue.state_recovery.replay_file) }
    if ($null -ne $queue.state_recovery.snapshot_replica_files) { $script:StateSnapshotReplicaPaths = Resolve-FullPathList -Root $RepoRoot -Values $queue.state_recovery.snapshot_replica_files }
    if ($null -ne $queue.state_recovery.replay_replica_files) { $script:StateReplayReplicaPaths = Resolve-FullPathList -Root $RepoRoot -Values $queue.state_recovery.replay_replica_files }
    if ($null -ne $queue.state_recovery.replay_max_entries) { $script:StateReplayMaxEntries = [Math]::Max(100, [int]$queue.state_recovery.replay_max_entries) }
    if ($null -ne $queue.state_recovery.enable_replica_validation) { $script:StateReplicaValidationEnabled = [bool]$queue.state_recovery.enable_replica_validation }
    if ($null -ne $queue.state_recovery.replica_validation_interval_seconds) { $script:StateReplicaValidationIntervalSec = [Math]::Max(1, [int]$queue.state_recovery.replica_validation_interval_seconds) }
    if ($null -ne $queue.state_recovery.replica_allowed_lag_entries) { $script:StateReplicaAllowedLagEntries = [Math]::Max(0, [int]$queue.state_recovery.replica_allowed_lag_entries) }
    if ($null -ne $queue.state_recovery.enable_replica_auto_failover) { $script:StateReplicaAutoFailoverEnabled = [bool]$queue.state_recovery.enable_replica_auto_failover }
    if ($null -ne $queue.state_recovery.replica_health_file -and -not [string]::IsNullOrWhiteSpace([string]$queue.state_recovery.replica_health_file)) { $script:StateReplicaHealthPath = Resolve-FullPath -Root $RepoRoot -Value ([string]$queue.state_recovery.replica_health_file) }
    if ($null -ne $queue.state_recovery.replica_failover_cooldown_seconds) { $script:StateReplicaFailoverCooldownSec = [Math]::Max(1, [int]$queue.state_recovery.replica_failover_cooldown_seconds) }
    $script:StateReplicaFailoverConvergeMinPauseSeconds = [Math]::Max([int]$script:StateReplicaFailoverConvergeMinPauseSeconds, [int]$script:StateReplicaFailoverCooldownSec)
    if ($null -ne $queue.state_recovery.failover_policy) {
        if ($null -ne $queue.state_recovery.failover_policy.enabled) { $script:StateReplicaFailoverPolicyEnabled = [bool]$queue.state_recovery.failover_policy.enabled }
        if ($null -ne $queue.state_recovery.failover_policy.default_allow) { $script:StateReplicaFailoverPolicyDefaultAllow = [bool]$queue.state_recovery.failover_policy.default_allow }
        if ($null -ne $queue.state_recovery.failover_policy.matrix) { $script:StateReplicaFailoverPolicyRawMatrix = @($queue.state_recovery.failover_policy.matrix) }
        if ($null -ne $queue.state_recovery.failover_policy.site_matrix_overrides) { $script:FailoverPolicyMatrixBySiteRaw = $queue.state_recovery.failover_policy.site_matrix_overrides }
        if ($null -ne $queue.state_recovery.failover_policy.region_matrix_overrides) { $script:FailoverPolicyMatrixByRegionRaw = $queue.state_recovery.failover_policy.region_matrix_overrides }
        if ($null -ne $queue.state_recovery.failover_policy.slo_link) {
            if ($null -ne $queue.state_recovery.failover_policy.slo_link.enabled) { $script:StateReplicaFailoverSloLinkEnabled = [bool]$queue.state_recovery.failover_policy.slo_link.enabled }
            if ($null -ne $queue.state_recovery.failover_policy.slo_link.min_effective_score) { $script:StateReplicaFailoverSloMinEffectiveScore = [double]$queue.state_recovery.failover_policy.slo_link.min_effective_score }
            if ($null -ne $queue.state_recovery.failover_policy.slo_link.block_on_violation) { $script:StateReplicaFailoverSloBlockOnViolation = [bool]$queue.state_recovery.failover_policy.slo_link.block_on_violation }
            if ($null -ne $queue.state_recovery.failover_policy.slo_link.site_overrides) { $script:FailoverSloLinkBySiteRaw = $queue.state_recovery.failover_policy.slo_link.site_overrides }
            if ($null -ne $queue.state_recovery.failover_policy.slo_link.region_overrides) { $script:FailoverSloLinkByRegionRaw = $queue.state_recovery.failover_policy.slo_link.region_overrides }
        }
        if ($null -ne $queue.state_recovery.failover_policy.drill_link) {
            if ($null -ne $queue.state_recovery.failover_policy.drill_link.enabled) { $script:StateReplicaFailoverDrillLinkEnabled = [bool]$queue.state_recovery.failover_policy.drill_link.enabled }
            if ($null -ne $queue.state_recovery.failover_policy.drill_link.min_pass_rate) { $script:StateReplicaFailoverDrillMinPassRate = [double]$queue.state_recovery.failover_policy.drill_link.min_pass_rate }
            if ($null -ne $queue.state_recovery.failover_policy.drill_link.min_average_score) { $script:StateReplicaFailoverDrillMinAverageScore = [double]$queue.state_recovery.failover_policy.drill_link.min_average_score }
            if ($null -ne $queue.state_recovery.failover_policy.drill_link.require_last_pass) { $script:StateReplicaFailoverDrillRequireLastPass = [bool]$queue.state_recovery.failover_policy.drill_link.require_last_pass }
            if ($null -ne $queue.state_recovery.failover_policy.drill_link.site_overrides) { $script:FailoverDrillLinkBySiteRaw = $queue.state_recovery.failover_policy.drill_link.site_overrides }
            if ($null -ne $queue.state_recovery.failover_policy.drill_link.region_overrides) { $script:FailoverDrillLinkByRegionRaw = $queue.state_recovery.failover_policy.drill_link.region_overrides }
        }
        if ($null -ne $queue.state_recovery.failover_policy.risk_link) {
            if ($null -ne $queue.state_recovery.failover_policy.risk_link.enabled) { $script:StateReplicaFailoverRiskLinkEnabled = [bool]$queue.state_recovery.failover_policy.risk_link.enabled }
            if ($null -ne $queue.state_recovery.failover_policy.risk_link.blocked_levels) { $script:StateReplicaFailoverRiskBlockedRaw = $queue.state_recovery.failover_policy.risk_link.blocked_levels }
            if ($null -ne $queue.state_recovery.failover_policy.risk_link.site_overrides) { $script:FailoverRiskLinkBySiteRaw = $queue.state_recovery.failover_policy.risk_link.site_overrides }
            if ($null -ne $queue.state_recovery.failover_policy.risk_link.region_overrides) { $script:FailoverRiskLinkByRegionRaw = $queue.state_recovery.failover_policy.risk_link.region_overrides }
        }
    }
    Apply-FailoverConvergeConfigFromStateRecovery -StateRecovery $queue.state_recovery -RiskPolicy $queue.risk_policy
    if ($null -ne $queue.state_recovery.replica_failover_on_startup) { $script:StateReplicaFailoverOnStartup = [bool]$queue.state_recovery.replica_failover_on_startup }
    if ($null -ne $queue.state_recovery.enable_replica_switchback) { $script:StateReplicaSwitchbackEnabled = [bool]$queue.state_recovery.enable_replica_switchback }
    if ($null -ne $queue.state_recovery.replica_switchback_stable_cycles) { $script:StateReplicaSwitchbackStableCycles = [Math]::Max(1, [int]$queue.state_recovery.replica_switchback_stable_cycles) }
    if ($null -ne $queue.state_recovery.replica_drill) {
        if ($null -ne $queue.state_recovery.replica_drill.enabled) { $script:StateReplicaDrillEnabled = [bool]$queue.state_recovery.replica_drill.enabled }
        if ($null -ne $queue.state_recovery.replica_drill.drill_id -and -not [string]::IsNullOrWhiteSpace([string]$queue.state_recovery.replica_drill.drill_id)) { $script:StateReplicaDrillId = [string]$queue.state_recovery.replica_drill.drill_id }
        if ($null -ne $queue.state_recovery.replica_drill.score) {
            if ($null -ne $queue.state_recovery.replica_drill.score.enabled) { $script:StateReplicaDrillScoreEnabled = [bool]$queue.state_recovery.replica_drill.score.enabled }
            if ($null -ne $queue.state_recovery.replica_drill.score.file -and -not [string]::IsNullOrWhiteSpace([string]$queue.state_recovery.replica_drill.score.file)) { $script:StateReplicaDrillScorePath = Resolve-FullPath -Root $RepoRoot -Value ([string]$queue.state_recovery.replica_drill.score.file) }
            if ($null -ne $queue.state_recovery.replica_drill.score.window_samples) { $script:StateReplicaDrillScoreWindowSamples = [Math]::Max(1, [int]$queue.state_recovery.replica_drill.score.window_samples) }
            if ($null -ne $queue.state_recovery.replica_drill.score.pass_score) { $script:StateReplicaDrillPassScore = [double]$queue.state_recovery.replica_drill.score.pass_score }
        }
    }
    if ($null -ne $queue.state_recovery.slo) {
        if ($null -ne $queue.state_recovery.slo.enabled) { $script:StateReplicaSloEnabled = [bool]$queue.state_recovery.slo.enabled }
        if ($null -ne $queue.state_recovery.slo.file -and -not [string]::IsNullOrWhiteSpace([string]$queue.state_recovery.slo.file)) { $script:StateReplicaSloPath = Resolve-FullPath -Root $RepoRoot -Value ([string]$queue.state_recovery.slo.file) }
        if ($null -ne $queue.state_recovery.slo.window_samples) { $script:StateReplicaSloWindowSamples = [Math]::Max(5, [int]$queue.state_recovery.slo.window_samples) }
        if ($null -ne $queue.state_recovery.slo.min_green_rate) { $script:StateReplicaSloMinGreenRate = [double]$queue.state_recovery.slo.min_green_rate }
        if ($null -ne $queue.state_recovery.slo.max_red_in_window) { $script:StateReplicaSloMaxRedInWindow = [Math]::Max(0, [int]$queue.state_recovery.slo.max_red_in_window) }
        if ($null -ne $queue.state_recovery.slo.block_on_violation) { $script:StateReplicaSloBlockOnViolation = [bool]$queue.state_recovery.slo.block_on_violation }
        if ($null -ne $queue.state_recovery.slo.adaptive) {
            if ($null -ne $queue.state_recovery.slo.adaptive.enabled) { $script:StateReplicaAdaptiveEnabled = [bool]$queue.state_recovery.slo.adaptive.enabled }
            if ($null -ne $queue.state_recovery.slo.adaptive.file -and -not [string]::IsNullOrWhiteSpace([string]$queue.state_recovery.slo.adaptive.file)) { $script:StateReplicaAdaptivePath = Resolve-FullPath -Root $RepoRoot -Value ([string]$queue.state_recovery.slo.adaptive.file) }
            if ($null -ne $queue.state_recovery.slo.adaptive.step) { $script:StateReplicaAdaptiveStep = [Math]::Max(0.1, [double]$queue.state_recovery.slo.adaptive.step) }
            if ($null -ne $queue.state_recovery.slo.adaptive.good_score) { $script:StateReplicaAdaptiveGoodScore = [double]$queue.state_recovery.slo.adaptive.good_score }
            if ($null -ne $queue.state_recovery.slo.adaptive.bad_score) { $script:StateReplicaAdaptiveBadScore = [double]$queue.state_recovery.slo.adaptive.bad_score }
            if ($null -ne $queue.state_recovery.slo.adaptive.max_shift) { $script:StateReplicaAdaptiveMaxShift = [Math]::Max(0.0, [double]$queue.state_recovery.slo.adaptive.max_shift) }
        }
        if ($null -ne $queue.state_recovery.slo.circuit_breaker) {
            if ($null -ne $queue.state_recovery.slo.circuit_breaker.enabled) { $script:StateReplicaCircuitEnabled = [bool]$queue.state_recovery.slo.circuit_breaker.enabled }
            if ($null -ne $queue.state_recovery.slo.circuit_breaker.yellow_max_concurrent_plans) { $script:StateReplicaCircuitYellowMaxConcurrent = [Math]::Max(1, [int]$queue.state_recovery.slo.circuit_breaker.yellow_max_concurrent_plans) }
            if ($null -ne $queue.state_recovery.slo.circuit_breaker.yellow_dispatch_pause_seconds) { $script:StateReplicaCircuitYellowPauseSeconds = [Math]::Max(0, [int]$queue.state_recovery.slo.circuit_breaker.yellow_dispatch_pause_seconds) }
            if ($null -ne $queue.state_recovery.slo.circuit_breaker.red_block) { $script:StateReplicaCircuitRedBlock = [bool]$queue.state_recovery.slo.circuit_breaker.red_block }
            if ($null -ne $queue.state_recovery.slo.circuit_breaker.matrix) { $script:StateReplicaCircuitRawMatrix = @($queue.state_recovery.slo.circuit_breaker.matrix) }
        }
    }
    if ($null -ne $queue.state_recovery.resume_from_snapshot) { $script:ResumeFromSnapshot = [bool]$queue.state_recovery.resume_from_snapshot }
    if ($null -ne $queue.state_recovery.replay_conflicts_on_start) { $script:ReplayConflictsOnStart = [bool]$queue.state_recovery.replay_conflicts_on_start }
}
if ($script:StateReplicaSloMinGreenRate -lt 0.0) { $script:StateReplicaSloMinGreenRate = 0.0 }
if ($script:StateReplicaSloMinGreenRate -gt 1.0) { $script:StateReplicaSloMinGreenRate = 1.0 }
if ($script:StateReplicaAdaptiveGoodScore -lt 0.0) { $script:StateReplicaAdaptiveGoodScore = 0.0 }
if ($script:StateReplicaAdaptiveGoodScore -gt 100.0) { $script:StateReplicaAdaptiveGoodScore = 100.0 }
if ($script:StateReplicaAdaptiveBadScore -lt 0.0) { $script:StateReplicaAdaptiveBadScore = 0.0 }
if ($script:StateReplicaAdaptiveBadScore -gt 100.0) { $script:StateReplicaAdaptiveBadScore = 100.0 }
if ($script:StateReplicaAdaptiveBadScore -gt $script:StateReplicaAdaptiveGoodScore) {
    $tmpScore = $script:StateReplicaAdaptiveBadScore
    $script:StateReplicaAdaptiveBadScore = $script:StateReplicaAdaptiveGoodScore
    $script:StateReplicaAdaptiveGoodScore = $tmpScore
}
if ($script:StateReplicaDrillPassScore -lt 0.0) { $script:StateReplicaDrillPassScore = 0.0 }
if ($script:StateReplicaDrillPassScore -gt 100.0) { $script:StateReplicaDrillPassScore = 100.0 }
if ($script:StateReplicaFailoverSloMinEffectiveScore -lt 0.0) { $script:StateReplicaFailoverSloMinEffectiveScore = 0.0 }
if ($script:StateReplicaFailoverSloMinEffectiveScore -gt 100.0) { $script:StateReplicaFailoverSloMinEffectiveScore = 100.0 }
if ($script:StateReplicaFailoverDrillMinPassRate -lt 0.0) { $script:StateReplicaFailoverDrillMinPassRate = 0.0 }
if ($script:StateReplicaFailoverDrillMinPassRate -gt 1.0) { $script:StateReplicaFailoverDrillMinPassRate = 1.0 }
if ($script:StateReplicaFailoverDrillMinAverageScore -lt 0.0) { $script:StateReplicaFailoverDrillMinAverageScore = 0.0 }
if ($script:StateReplicaFailoverDrillMinAverageScore -gt 100.0) { $script:StateReplicaFailoverDrillMinAverageScore = 100.0 }
if ($script:StateReplicaSloEnabled -and -not $script:StateReplicaValidationEnabled) {
    throw "state replica slo enabled but replica validation is disabled"
}
if ($script:StateReplicaCircuitEnabled -and -not $script:StateReplicaSloEnabled) {
    throw "state replica circuit breaker enabled but replica slo is disabled"
}
if ($script:StateReplicaAdaptiveEnabled -and -not $script:StateReplicaSloEnabled) {
    throw "state replica adaptive threshold enabled but replica slo is disabled"
}
$script:StateReplicaFailoverPolicyMatrix = Resolve-FailoverPolicyMatrix -RawRules $script:StateReplicaFailoverPolicyRawMatrix
$script:StateReplicaFailoverPolicyMatrixBySite = Resolve-FailoverPolicyMatrixMap -RawMap $script:FailoverPolicyMatrixBySiteRaw -FallbackMatrix $script:StateReplicaFailoverPolicyMatrix -NormalizeRegion:$false
$script:StateReplicaFailoverPolicyMatrixByRegion = Resolve-FailoverPolicyMatrixMap -RawMap $script:FailoverPolicyMatrixByRegionRaw -FallbackMatrix $script:StateReplicaFailoverPolicyMatrix -NormalizeRegion:$true
Rebuild-FailoverConvergeDerivedState
$script:StateReplicaFailoverSloLinkProfileGlobal = [pscustomobject]@{
    enabled = [bool]$script:StateReplicaFailoverSloLinkEnabled
    min_effective_score = [double]$script:StateReplicaFailoverSloMinEffectiveScore
    block_on_violation = [bool]$script:StateReplicaFailoverSloBlockOnViolation
}
$script:StateReplicaFailoverDrillLinkProfileGlobal = [pscustomobject]@{
    enabled = [bool]$script:StateReplicaFailoverDrillLinkEnabled
    min_pass_rate = [double]$script:StateReplicaFailoverDrillMinPassRate
    min_average_score = [double]$script:StateReplicaFailoverDrillMinAverageScore
    require_last_pass = [bool]$script:StateReplicaFailoverDrillRequireLastPass
}
$script:StateReplicaFailoverRiskLinkProfileGlobal = [pscustomobject]@{
    enabled = [bool]$script:StateReplicaFailoverRiskLinkEnabled
}
$script:StateReplicaFailoverSloLinkProfileBySite = Resolve-FailoverSloLinkProfileMap -RawMap $script:FailoverSloLinkBySiteRaw -FallbackProfile $script:StateReplicaFailoverSloLinkProfileGlobal -NormalizeRegion:$false
$script:StateReplicaFailoverSloLinkProfileByRegion = Resolve-FailoverSloLinkProfileMap -RawMap $script:FailoverSloLinkByRegionRaw -FallbackProfile $script:StateReplicaFailoverSloLinkProfileGlobal -NormalizeRegion:$true
$script:StateReplicaFailoverDrillLinkProfileBySite = Resolve-FailoverDrillLinkProfileMap -RawMap $script:FailoverDrillLinkBySiteRaw -FallbackProfile $script:StateReplicaFailoverDrillLinkProfileGlobal -NormalizeRegion:$false
$script:StateReplicaFailoverDrillLinkProfileByRegion = Resolve-FailoverDrillLinkProfileMap -RawMap $script:FailoverDrillLinkByRegionRaw -FallbackProfile $script:StateReplicaFailoverDrillLinkProfileGlobal -NormalizeRegion:$true
$script:StateReplicaFailoverRiskLinkProfileBySite = Resolve-FailoverRiskLinkProfileMap -RawMap $script:FailoverRiskLinkBySiteRaw -FallbackProfile $script:StateReplicaFailoverRiskLinkProfileGlobal -NormalizeRegion:$false
$script:StateReplicaFailoverRiskLinkProfileByRegion = Resolve-FailoverRiskLinkProfileMap -RawMap $script:FailoverRiskLinkByRegionRaw -FallbackProfile $script:StateReplicaFailoverRiskLinkProfileGlobal -NormalizeRegion:$true
$script:StateReplicaCircuitMatrix = Resolve-CircuitMatrix -RawRules $script:StateReplicaCircuitRawMatrix -BaseConcurrent $script:StateReplicaCircuitBaseConcurrent -BasePause $script:StateReplicaCircuitBasePauseSeconds -YellowConcurrent $script:StateReplicaCircuitYellowMaxConcurrent -YellowPause $script:StateReplicaCircuitYellowPauseSeconds -RedBlock $script:StateReplicaCircuitRedBlock

$script:SiteConsensusEnabled = [bool]$EnableSiteConsensus
$script:SiteId = $SiteId
$script:SiteConsensusPath = Resolve-FullPath -Root $RepoRoot -Value $SiteConsensusStateFile
$script:SiteConsensusRequiredSites = [Math]::Max(1, $SiteConsensusRequiredSites)
$script:SiteConsensusVoteTtlSec = [Math]::Max(5, $SiteConsensusVoteTtlSeconds)
$script:SiteConsensusRetrySec = [Math]::Max(1, $SiteConsensusRetrySeconds)
$script:SiteConflictAccountabilityEnabled = [bool]$EnableSiteConflictAccountability
$script:SiteConflictAccountabilityPath = Resolve-FullPath -Root $RepoRoot -Value $SiteConflictAccountabilityFile
$script:SiteConflictMaxPenaltyPoints = [Math]::Max(0, $SiteConflictMaxPenaltyPoints)
$script:SiteConflictRecoveryPerWin = [Math]::Max(0, $SiteConflictRecoveryPerWin)
$script:SiteConflictReputationAgingEnabled = [bool]$EnableSiteConflictReputationAging
$script:SiteConflictReputationAgingIntervalSec = [Math]::Max(60, $SiteConflictReputationAgingIntervalSeconds)
$script:SiteConflictReputationRecoverPointsPerInterval = [Math]::Max(0, $SiteConflictReputationRecoverPointsPerInterval)
$script:SiteConflictReputationRecoverIdleSec = [Math]::Max(0, $SiteConflictReputationRecoverIdleSeconds)
$script:SiteConflictRiskPredictorEnabled = [bool]$EnableSiteConflictRiskPredictor
$script:SiteConflictRiskStatePath = Resolve-FullPath -Root $RepoRoot -Value $SiteConflictRiskStateFile
$script:SiteConflictRiskEmaAlpha = $SiteConflictRiskEmaAlpha
$script:SiteConflictRiskAutoThrottleEnabled = [bool]$EnableSiteConflictRiskAutoThrottle
$script:SiteConflictRiskYellowMaxConcurrent = [Math]::Max(1, $SiteConflictRiskYellowMaxConcurrentPlans)
$script:SiteConflictRiskYellowPauseSeconds = [Math]::Max(0, $SiteConflictRiskYellowDispatchPauseSeconds)
$script:SiteConflictRiskOrangeMaxConcurrent = [Math]::Max(1, $SiteConflictRiskOrangeMaxConcurrentPlans)
$script:SiteConflictRiskOrangePauseSeconds = [Math]::Max(0, $SiteConflictRiskOrangeDispatchPauseSeconds)
$script:SiteConflictRiskRedBlock = [bool]$SiteConflictRiskRedBlock
$script:SiteConflictRiskWinnerGuardEnabled = [bool]$EnableSiteConflictRiskWinnerGuard
$script:SiteConflictRiskWinnerBlockedRaw = if ($null -ne $script:RoleRiskWinnerBlockedRaw) { $script:RoleRiskWinnerBlockedRaw } else { $script:UnifiedRiskBlockedRaw }
$script:SiteConflictRiskWinnerBlockedSet = @{}
$script:SiteConflictRiskWinnerBlockedSetBySite = @{}
$script:SiteConflictRiskWinnerBlockedSetByRegion = @{}
$script:SiteConflictRiskWinnerFallbackAllow = [bool]$SiteConflictRiskWinnerFallbackAllow
$script:SiteConflictRiskActionMatrix = @{}
$script:SiteConflictRiskActionMatrixGlobal = @()
$script:SiteConflictRiskActionMatrixBySite = @{}
$script:SiteConflictRiskActionMatrixByRegion = @{}
$script:SiteRegionMap = @{}
$script:SiteConflictPenaltyRawMatrix = @()
$script:SiteConflictPenaltyMatrix = @()
$script:SiteConflictAccountabilityState = New-SiteConflictAccountabilityState
$script:SiteConflictRiskState = New-SiteConflictRiskState
$script:SitePriorityDefault = 0
$script:SitePriorityMap = @{}
$script:SitePriorityMapBySite = @{}
$script:SitePriorityMapByRegion = @{}
if ($null -ne $queue.site_consensus) {
    if ($null -ne $queue.site_consensus.enabled) { $script:SiteConsensusEnabled = [bool]$queue.site_consensus.enabled }
    if ($null -ne $queue.site_consensus.site_id -and -not [string]::IsNullOrWhiteSpace([string]$queue.site_consensus.site_id)) { $script:SiteId = [string]$queue.site_consensus.site_id }
    if ($null -ne $queue.site_consensus.state_file -and -not [string]::IsNullOrWhiteSpace([string]$queue.site_consensus.state_file)) { $script:SiteConsensusPath = Resolve-FullPath -Root $RepoRoot -Value ([string]$queue.site_consensus.state_file) }
    if ($null -ne $queue.site_consensus.required_sites) { $script:SiteConsensusRequiredSites = [Math]::Max(1, [int]$queue.site_consensus.required_sites) }
    if ($null -ne $queue.site_consensus.vote_ttl_seconds) { $script:SiteConsensusVoteTtlSec = [Math]::Max(5, [int]$queue.site_consensus.vote_ttl_seconds) }
    if ($null -ne $queue.site_consensus.retry_seconds) { $script:SiteConsensusRetrySec = [Math]::Max(1, [int]$queue.site_consensus.retry_seconds) }
    if ($null -ne $queue.site_consensus.site_priorities) {
        $priorityCfg = $queue.site_consensus.site_priorities
        $isStructuredPriority = ($null -ne $priorityCfg.global_default) -or ($null -ne $priorityCfg.region_overrides) -or ($null -ne $priorityCfg.site_overrides)
        if ($isStructuredPriority) {
            if ($null -ne $priorityCfg.global_default) {
                $script:SitePriorityDefault = [int]$priorityCfg.global_default
            }
            if ($null -ne $priorityCfg.site_overrides) {
                $script:SitePriorityMapBySite = Resolve-SitePriorityMap -RawMap $priorityCfg.site_overrides -NormalizeRegion:$false
            }
            if ($null -ne $priorityCfg.region_overrides) {
                $script:SitePriorityMapByRegion = Resolve-SitePriorityMap -RawMap $priorityCfg.region_overrides -NormalizeRegion:$true
            }
        } else {
            $script:SitePriorityMapBySite = Resolve-SitePriorityMap -RawMap $priorityCfg -NormalizeRegion:$false
        }
        $script:SitePriorityMap = $script:SitePriorityMapBySite
    }
    if ($null -ne $queue.site_consensus.accountability) {
        if ($null -ne $queue.site_consensus.accountability.enabled) { $script:SiteConflictAccountabilityEnabled = [bool]$queue.site_consensus.accountability.enabled }
        if ($null -ne $queue.site_consensus.accountability.state_file -and -not [string]::IsNullOrWhiteSpace([string]$queue.site_consensus.accountability.state_file)) { $script:SiteConflictAccountabilityPath = Resolve-FullPath -Root $RepoRoot -Value ([string]$queue.site_consensus.accountability.state_file) }
        if ($null -ne $queue.site_consensus.accountability.max_penalty_points) { $script:SiteConflictMaxPenaltyPoints = [Math]::Max(0, [int]$queue.site_consensus.accountability.max_penalty_points) }
        if ($null -ne $queue.site_consensus.accountability.recovery_per_win) { $script:SiteConflictRecoveryPerWin = [Math]::Max(0, [int]$queue.site_consensus.accountability.recovery_per_win) }
        if ($null -ne $queue.site_consensus.accountability.matrix) { $script:SiteConflictPenaltyRawMatrix = @($queue.site_consensus.accountability.matrix) }
        if ($null -ne $queue.site_consensus.accountability.reputation) {
            if ($null -ne $queue.site_consensus.accountability.reputation.enabled) { $script:SiteConflictReputationAgingEnabled = [bool]$queue.site_consensus.accountability.reputation.enabled }
            if ($null -ne $queue.site_consensus.accountability.reputation.aging_interval_seconds) { $script:SiteConflictReputationAgingIntervalSec = [Math]::Max(60, [int]$queue.site_consensus.accountability.reputation.aging_interval_seconds) }
            if ($null -ne $queue.site_consensus.accountability.reputation.recover_points_per_interval) { $script:SiteConflictReputationRecoverPointsPerInterval = [Math]::Max(0, [int]$queue.site_consensus.accountability.reputation.recover_points_per_interval) }
            if ($null -ne $queue.site_consensus.accountability.reputation.recover_idle_seconds) { $script:SiteConflictReputationRecoverIdleSec = [Math]::Max(0, [int]$queue.site_consensus.accountability.reputation.recover_idle_seconds) }
        }
        if ($null -ne $queue.site_consensus.accountability.risk) {
            if ($null -ne $queue.site_consensus.accountability.risk.enabled) { $script:SiteConflictRiskPredictorEnabled = [bool]$queue.site_consensus.accountability.risk.enabled }
            if ($null -ne $queue.site_consensus.accountability.risk.state_file -and -not [string]::IsNullOrWhiteSpace([string]$queue.site_consensus.accountability.risk.state_file)) { $script:SiteConflictRiskStatePath = Resolve-FullPath -Root $RepoRoot -Value ([string]$queue.site_consensus.accountability.risk.state_file) }
            if ($null -ne $queue.site_consensus.accountability.risk.ema_alpha) { $script:SiteConflictRiskEmaAlpha = [double]$queue.site_consensus.accountability.risk.ema_alpha }
            if ($null -ne $queue.site_consensus.accountability.risk.auto_throttle) {
                if ($null -ne $queue.site_consensus.accountability.risk.auto_throttle.enabled) { $script:SiteConflictRiskAutoThrottleEnabled = [bool]$queue.site_consensus.accountability.risk.auto_throttle.enabled }
                if ($null -ne $queue.site_consensus.accountability.risk.auto_throttle.yellow_max_concurrent_plans) { $script:SiteConflictRiskYellowMaxConcurrent = [Math]::Max(1, [int]$queue.site_consensus.accountability.risk.auto_throttle.yellow_max_concurrent_plans) }
                if ($null -ne $queue.site_consensus.accountability.risk.auto_throttle.yellow_dispatch_pause_seconds) { $script:SiteConflictRiskYellowPauseSeconds = [Math]::Max(0, [int]$queue.site_consensus.accountability.risk.auto_throttle.yellow_dispatch_pause_seconds) }
                if ($null -ne $queue.site_consensus.accountability.risk.auto_throttle.orange_max_concurrent_plans) { $script:SiteConflictRiskOrangeMaxConcurrent = [Math]::Max(1, [int]$queue.site_consensus.accountability.risk.auto_throttle.orange_max_concurrent_plans) }
                if ($null -ne $queue.site_consensus.accountability.risk.auto_throttle.orange_dispatch_pause_seconds) { $script:SiteConflictRiskOrangePauseSeconds = [Math]::Max(0, [int]$queue.site_consensus.accountability.risk.auto_throttle.orange_dispatch_pause_seconds) }
                if ($null -ne $queue.site_consensus.accountability.risk.auto_throttle.red_block) { $script:SiteConflictRiskRedBlock = [bool]$queue.site_consensus.accountability.risk.auto_throttle.red_block }
            }
            if ($null -ne $queue.site_consensus.accountability.risk.winner_guard) {
                if ($null -ne $queue.site_consensus.accountability.risk.winner_guard.enabled) { $script:SiteConflictRiskWinnerGuardEnabled = [bool]$queue.site_consensus.accountability.risk.winner_guard.enabled }
                if ($null -ne $queue.site_consensus.accountability.risk.winner_guard.blocked_levels) { $script:SiteConflictRiskWinnerBlockedRaw = $queue.site_consensus.accountability.risk.winner_guard.blocked_levels }
                if ($null -ne $queue.site_consensus.accountability.risk.winner_guard.fallback_allow_when_all_blocked) { $script:SiteConflictRiskWinnerFallbackAllow = [bool]$queue.site_consensus.accountability.risk.winner_guard.fallback_allow_when_all_blocked }
            }
        }
    }
}
if ($script:SiteConsensusEnabled -and [string]::IsNullOrWhiteSpace($script:SiteId)) {
    throw "site consensus enabled but site_id is empty"
}
if ($script:SiteConflictAccountabilityEnabled -and -not $script:SiteConsensusEnabled) {
    throw "site conflict accountability enabled but site consensus is disabled"
}
if ($script:SiteConflictReputationAgingEnabled -and -not $script:SiteConflictAccountabilityEnabled) {
    throw "site conflict reputation aging enabled but accountability is disabled"
}
if ($script:SiteConflictRiskPredictorEnabled -and -not $script:SiteConflictAccountabilityEnabled) {
    throw "site conflict risk predictor enabled but accountability is disabled"
}
if ($script:SiteConflictRiskWinnerGuardEnabled -and -not $script:SiteConflictRiskPredictorEnabled) {
    throw "site conflict risk winner guard enabled but risk predictor is disabled"
}
if ($script:StateReplicaFailoverRiskLinkEnabled -and -not $script:SiteConflictRiskPredictorEnabled) {
    throw "state replica failover risk link enabled but site conflict risk predictor is disabled"
}
if ($script:SiteConflictRiskEmaAlpha -lt 0.01) { $script:SiteConflictRiskEmaAlpha = 0.01 }
if ($script:SiteConflictRiskEmaAlpha -gt 1.0) { $script:SiteConflictRiskEmaAlpha = 1.0 }
Rebuild-RiskPolicyDerivedState
$script:SiteConflictPenaltyMatrix = Resolve-SiteConflictPenaltyMatrix -RawRules $script:SiteConflictPenaltyRawMatrix
$script:SiteConflictAccountabilityState.max_penalty_points = [int]$script:SiteConflictMaxPenaltyPoints
$script:SiteConflictAccountabilityState.recovery_per_win = [int]$script:SiteConflictRecoveryPerWin
$script:SiteConflictAccountabilityState.reputation_aging_enabled = [bool]$script:SiteConflictReputationAgingEnabled
$script:SiteConflictAccountabilityState.reputation_aging_interval_seconds = [int]$script:SiteConflictReputationAgingIntervalSec
$script:SiteConflictAccountabilityState.reputation_recover_points_per_interval = [int]$script:SiteConflictReputationRecoverPointsPerInterval
$script:SiteConflictAccountabilityState.reputation_recover_idle_seconds = [int]$script:SiteConflictReputationRecoverIdleSec
$script:SiteConflictRiskState.ema_alpha = [double]$script:SiteConflictRiskEmaAlpha

$leaseStream = $null
$nextLeaseHeartbeatMs = 0
$script:DedupeState = New-DedupeState
$script:SiteConsensusState = New-SiteConsensusState
$pending = @()
$running = @()
$doneOk = 0
$doneErr = 0
$doneSkip = 0
$stateStatus = "init"
$nextReplicaValidationMs = 0

try {
    $leaseStream = Try-Acquire-LeaseLock -Path $LeasePath
    if ($null -eq $leaseStream) {
        throw ("controller lease is held by another active controller: lease_file=" + $LeasePath)
    }
    Write-LeaseHeartbeat -Stream $leaseStream -Controller $ControllerId -Operation $ControlOpId -TtlSec $effectiveLeaseTtl
    $nextLeaseHeartbeatMs = (Now-Ms) + ([int64]$effectiveLeaseHeartbeat * 1000)

    $script:DedupeState = Load-DedupeState -Path $script:DedupePath
    Cleanup-DedupeState -State $script:DedupeState -TtlSec $script:DedupeTtlSec
    Save-DedupeState -Path $script:DedupePath -State $script:DedupeState
    if ($script:SiteConsensusEnabled) {
        $script:SiteConsensusState = Load-SiteConsensusState -Path $script:SiteConsensusPath
        Cleanup-SiteConsensusState -State $script:SiteConsensusState -VoteTtlSec $script:SiteConsensusVoteTtlSec
        Save-SiteConsensusState -Path $script:SiteConsensusPath -State $script:SiteConsensusState
        if ($script:SiteConflictAccountabilityEnabled) {
            $script:SiteConflictAccountabilityState = Load-SiteConflictAccountabilityState -Path $script:SiteConflictAccountabilityPath
            $script:SiteConflictAccountabilityState.max_penalty_points = [int]$script:SiteConflictMaxPenaltyPoints
            $script:SiteConflictAccountabilityState.recovery_per_win = [int]$script:SiteConflictRecoveryPerWin
            $script:SiteConflictAccountabilityState.reputation_aging_enabled = [bool]$script:SiteConflictReputationAgingEnabled
            $script:SiteConflictAccountabilityState.reputation_aging_interval_seconds = [int]$script:SiteConflictReputationAgingIntervalSec
            $script:SiteConflictAccountabilityState.reputation_recover_points_per_interval = [int]$script:SiteConflictReputationRecoverPointsPerInterval
            $script:SiteConflictAccountabilityState.reputation_recover_idle_seconds = [int]$script:SiteConflictReputationRecoverIdleSec
            Save-SiteConflictAccountabilityState -Path $script:SiteConflictAccountabilityPath -State $script:SiteConflictAccountabilityState
            if ($script:SiteConflictRiskPredictorEnabled) {
                $script:SiteConflictRiskState = Load-SiteConflictRiskState -Path $script:SiteConflictRiskStatePath
                $script:SiteConflictRiskState.ema_alpha = [double]$script:SiteConflictRiskEmaAlpha
                Save-SiteConflictRiskState -Path $script:SiteConflictRiskStatePath -State $script:SiteConflictRiskState
            }
        }
    }
    if ($script:StateRecoveryEnabled) {
        Trim-ReplayFile -Path $script:StateReplayPath -MaxEntries $script:StateReplayMaxEntries
        Restore-ReplicaModeFromHealthFile -Path $script:StateReplicaHealthPath
        if ($script:StateReplicaDrillScoreEnabled) {
            $script:StateReplicaDrillScoreState = Load-ReplicaDrillScoreState -Path $script:StateReplicaDrillScorePath
        }
        if ($script:StateReplicaSloEnabled) {
            $script:StateReplicaSloState = Load-ReplicaSloState -Path $script:StateReplicaSloPath
        }
        if ($script:StateReplicaAdaptiveEnabled) {
            $script:StateReplicaAdaptiveState = Load-ReplicaAdaptiveState -Path $script:StateReplicaAdaptivePath
        }
    }

    Write-Host ("rollout_control_in: queue={0} action={1} max_concurrent={2} preemption={3} adaptive={4} site_consensus={5} site_accountability={6} site_reputation_aging={7} site_risk_predictor={8} site_risk_throttle={9} state_recovery={10} replica_validation={11} replica_failover={12} failover_policy={13} failover_slo_link={14} failover_drill_link={15} switchback={16} drill={17} drill_score={18} slo={19} slo_block={20} slo_circuit={21} slo_adaptive={22} role={23} controller={24} op={25}" -f $QueuePath, $PlanAction, $effectiveConcurrent, $effectivePreempt, $script:AdaptiveEnabled, $script:SiteConsensusEnabled, $script:SiteConflictAccountabilityEnabled, $script:SiteConflictReputationAgingEnabled, $script:SiteConflictRiskPredictorEnabled, $script:SiteConflictRiskAutoThrottleEnabled, $script:StateRecoveryEnabled, $script:StateReplicaValidationEnabled, $script:StateReplicaAutoFailoverEnabled, $script:StateReplicaFailoverPolicyEnabled, $script:StateReplicaFailoverSloLinkEnabled, $script:StateReplicaFailoverDrillLinkEnabled, $script:StateReplicaSwitchbackEnabled, $script:StateReplicaDrillEnabled, $script:StateReplicaDrillScoreEnabled, $script:StateReplicaSloEnabled, $script:StateReplicaSloBlockOnViolation, $script:StateReplicaCircuitEnabled, $script:StateReplicaAdaptiveEnabled, $controllerRole, $ControllerId, $ControlOpId)

    Emit-ReplicaDrill -AuditPath $AuditPath -QueuePath $QueuePath -PlanAction $PlanAction -ControlOpId $ControlOpId -ControllerId $ControllerId
    $siteRepAgingStartup = Apply-SiteConflictReputationAging
    foreach ($ev in @($siteRepAgingStartup)) {
        Write-Host ("rollout_control_site_reputation_aging: source=startup site={0} old_penalty={1} new_penalty={2} reputation_score={3}" -f $ev.site_id, $ev.old_penalty, $ev.new_penalty, $ev.reputation_score)
        Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                timestamp_utc = [DateTime]::UtcNow.ToString("o")
                control_operation_id = $ControlOpId
                controller_id = $ControllerId
                queue_file = $QueuePath
                action = $PlanAction
                result = "consensus_accountability_reputation_aging"
                source = "startup"
                site_id = [string]$ev.site_id
                old_penalty = [int]$ev.old_penalty
                new_penalty = [int]$ev.new_penalty
                recover_points = [int]$ev.recover_points
                base_priority = [int]$ev.base_priority
                effective_priority = [int]$ev.effective_priority
                reputation_score = [double]$ev.reputation_score
                error = ""
            })
    }
    $siteRiskStartup = Predict-SiteConflictRisk
    $startupRiskCapOverride = 0
    $startupRiskPauseOverride = 0
    $startupRiskBlocked = $false
    if ([bool]$siteRiskStartup.enabled) {
        Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                timestamp_utc = [DateTime]::UtcNow.ToString("o")
                control_operation_id = $ControlOpId
                controller_id = $ControllerId
                queue_file = $QueuePath
                action = $PlanAction
                result = "consensus_accountability_risk_predict"
                source = "startup"
                worst_site_id = [string]$siteRiskStartup.worst_site_id
                worst_level = [string]$siteRiskStartup.worst_level
                worst_score = [double]$siteRiskStartup.worst_score
                total_sites = [int]$siteRiskStartup.total_sites
                error = ""
            })
        $startupRiskPolicy = Resolve-SiteConflictRiskThrottlePolicy -Risk $siteRiskStartup -Source "startup" -SiteId ([string]$siteRiskStartup.worst_site_id)
        Emit-SiteRiskThrottlePolicyAuditIfChanged -Source "startup" -Risk $siteRiskStartup -Policy $startupRiskPolicy -AuditPath $AuditPath -QueuePath $QueuePath -PlanAction $PlanAction -ControlOpId $ControlOpId -ControllerId $ControllerId
        if ([int]$startupRiskPolicy.cap_concurrent -gt 0) {
            $startupRiskCapOverride = [int]$startupRiskPolicy.cap_concurrent
        }
        if ([int]$startupRiskPolicy.pause_seconds -gt 0) {
            $startupRiskPauseOverride = [int]$startupRiskPolicy.pause_seconds
        }
        if ([bool]$startupRiskPolicy.block_dispatch) {
            $startupRiskBlocked = $true
            Write-Host ("rollout_control_site_risk_block: source=startup level={0} worst_site={1} score={2}" -f $startupRiskPolicy.level, $siteRiskStartup.worst_site_id, $siteRiskStartup.worst_score)
            Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                    timestamp_utc = [DateTime]::UtcNow.ToString("o")
                    control_operation_id = $ControlOpId
                    controller_id = $ControllerId
                    queue_file = $QueuePath
                    action = $PlanAction
                    result = "site_risk_throttle_blocked"
                    source = "startup"
                    worst_site_id = [string]$siteRiskStartup.worst_site_id
                    risk_level = [string]$startupRiskPolicy.level
                    risk_score = [double]$siteRiskStartup.worst_score
                    risk_policy_scope = [string]$startupRiskPolicy.scope
                    risk_policy_rule_source = [string]$startupRiskPolicy.rule_source
                    risk_policy_site_priority = [int]$startupRiskPolicy.site_priority
                    risk_policy_min_site_priority = [int]$startupRiskPolicy.min_site_priority
                    risk_policy_priority_gate = [string]$startupRiskPolicy.priority_gate
                    error = "dispatch blocked by site conflict risk policy"
                })
        }
        $startupDecisionConcurrent = [Math]::Max(1, [int]$effectiveConcurrent)
        if ([int]$startupRiskCapOverride -gt 0) {
            $startupDecisionConcurrent = [Math]::Max(1, [Math]::Min($startupDecisionConcurrent, [int]$startupRiskCapOverride))
        }
        $startupDecisionPause = [Math]::Max(0, [int]$effectivePause)
        if ([int]$startupRiskPauseOverride -gt 0) {
            $startupDecisionPause = [Math]::Max($startupDecisionPause, [int]$startupRiskPauseOverride)
        }
        Emit-RolloutDecisionSummaryIfChanged -Source "startup" -Risk $siteRiskStartup -RiskPolicy $startupRiskPolicy -Role $controllerRole -EffectiveConcurrent $startupDecisionConcurrent -EffectivePauseSeconds $startupDecisionPause -DispatchBlocked ([bool]$startupRiskBlocked) -AuditPath $AuditPath -QueuePath $QueuePath -PlanAction $PlanAction -ControlOpId $ControlOpId -ControllerId $ControllerId
        Invoke-DecisionDashboardExportIfDue -Source "startup" -AuditPath $AuditPath -QueuePath $QueuePath -PlanAction $PlanAction -ControlOpId $ControlOpId -ControllerId $ControllerId
        Invoke-DecisionDashboardConsumerIfDue -Source "startup" -AuditPath $AuditPath -QueuePath $QueuePath -PlanAction $PlanAction -ControlOpId $ControlOpId -ControllerId $ControllerId
    }

    foreach ($plan in $queue.plans) {
        $enabled = $true
        if ($null -ne $plan.enabled) { $enabled = [bool]$plan.enabled }
        if (-not $enabled) { continue }

        $name = [string]$plan.name
        if ([string]::IsNullOrWhiteSpace($name)) { $name = "(unnamed-plan)" }

        if (-not $IgnoreRegionWindow) {
            $window = ""
            if ($null -ne $plan.region_window) { $window = [string]$plan.region_window }
            $wc = Test-WindowUtc -Window $window
            if (-not $wc.allowed) {
                $msg = ("blocked by region window: " + $wc.reason)
                Write-Host ("rollout_control_skip: plan={0} reason={1}" -f $name, $msg)
                Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                        timestamp_utc = [DateTime]::UtcNow.ToString("o")
                        control_operation_id = $ControlOpId
                        controller_id = $ControllerId
                        queue_file = $QueuePath
                        plan = $name
                        action = $PlanAction
                        result = "blocked"
                        error = $msg
                    })
                continue
            }
        }

        $entry = Build-Entry -RepoRoot $RepoRoot -Plan $plan -BaseAction $PlanAction -GlobalTarget $GlobalTargetVersion -GlobalRollback $GlobalRollbackVersion -DefaultController $ControllerId -DefaultOpId $ControlOpId -DefaultAudit $AuditFile -DefaultPreemptRequeue $effectivePreemptRequeue
        $pending += $entry
    }

    if ($script:StateRecoveryEnabled) {
        $snapshot = Load-StateRecoverySnapshot -Path $script:StateSnapshotPath
        $restored = Restore-PendingFromSnapshot -Pending $pending -Snapshot $snapshot -QueuePath $QueuePath -PlanAction $PlanAction -ResumeEnabled $script:ResumeFromSnapshot
        $pending = @($restored.pending)
        if ($restored.recovered_pending -gt 0 -or $restored.recovered_running -gt 0) {
            Write-Host ("rollout_control_recover_snapshot: recovered_pending={0} recovered_running={1}" -f $restored.recovered_pending, $restored.recovered_running)
            Write-ReplayEvent -Path $script:StateReplayPath -Obj ([pscustomobject][ordered]@{
                    timestamp_utc = [DateTime]::UtcNow.ToString("o")
                    control_operation_id = $ControlOpId
                    controller_id = $ControllerId
                    queue_file = $QueuePath
                    action = $PlanAction
                    result = "snapshot_recovered"
                    recovered_pending = $restored.recovered_pending
                    recovered_running = $restored.recovered_running
                    error = ""
                })
        }
        $replayEvents = Load-ReplayEvents -Path $script:StateReplayPath -TailMax $script:StateReplayMaxEntries
        $replayed = Apply-ConflictReplay -Pending $pending -Events $replayEvents -QueuePath $QueuePath -PlanAction $PlanAction -Enabled $script:ReplayConflictsOnStart
        $pending = @($replayed.pending)
        if ($replayed.replayed -gt 0) {
            Write-Host ("rollout_control_replay_conflicts: replayed={0}" -f $replayed.replayed)
        }
        Save-StateRecoverySnapshot -Path $script:StateSnapshotPath -QueuePath $QueuePath -PlanAction $PlanAction -ControllerId $ControllerId -ControlOpId $ControlOpId -Status "running" -Pending $pending -Running $running -DoneOk $doneOk -DoneErr $doneErr -DoneSkip $doneSkip
        if ($script:StateReplicaValidationEnabled) {
            $vc = Validate-StateReplicas -AllowedLagEntries $script:StateReplicaAllowedLagEntries
            $foStartup = [pscustomobject]@{
                changed = $false
                details = @()
                skipped = ""
            }
            if (-not $vc.ok -and $script:StateReplicaAutoFailoverEnabled -and $script:StateReplicaFailoverOnStartup) {
                $foStartup = Try-AutoFailoverStateReplicas -AllowedLagEntries $script:StateReplicaAllowedLagEntries -Source "startup" -Validation $vc
                if ($foStartup.changed) {
                    Write-Host ("rollout_control_replica_failover: source=startup details={0}" -f ($foStartup.details -join ","))
                    Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                            timestamp_utc = [DateTime]::UtcNow.ToString("o")
                            control_operation_id = $ControlOpId
                            controller_id = $ControllerId
                            queue_file = $QueuePath
                            action = $PlanAction
                            result = "replica_failover"
                            error = ("source=startup details=" + ($foStartup.details -join ","))
                        })
                    $vc = Validate-StateReplicas -AllowedLagEntries $script:StateReplicaAllowedLagEntries
                } elseif (-not [string]::IsNullOrWhiteSpace([string]$foStartup.skipped) -and ([string]$foStartup.skipped).StartsWith("policy_blocked")) {
                    Write-Host ("rollout_control_replica_failover_policy_blocked: source=startup {0}" -f $foStartup.skipped)
                    Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                            timestamp_utc = [DateTime]::UtcNow.ToString("o")
                            control_operation_id = $ControlOpId
                            controller_id = $ControllerId
                            queue_file = $QueuePath
                            action = $PlanAction
                            result = "replica_failover_policy_blocked"
                            error = ("source=startup " + [string]$foStartup.skipped)
                        })
                }
            }
            if (-not [bool]$vc.ok -and $script:StateReplicaFailoverMode) {
                $script:StateReplicaStableCycles = 0
            }
            $sbStartup = [pscustomobject]@{
                changed = $false
                details = @()
                skipped = ""
            }
            if ([bool]$vc.ok) {
                $sbStartup = Try-SwitchbackStateReplicas -Validation $vc
                if ($sbStartup.changed) {
                    Write-Host ("rollout_control_replica_switchback: source=startup details={0}" -f ($sbStartup.details -join ","))
                    Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                            timestamp_utc = [DateTime]::UtcNow.ToString("o")
                            control_operation_id = $ControlOpId
                            controller_id = $ControllerId
                            queue_file = $QueuePath
                            action = $PlanAction
                            result = "replica_switchback"
                            error = ("source=startup details=" + ($sbStartup.details -join ","))
                        })
                    $vc = Validate-StateReplicas -AllowedLagEntries $script:StateReplicaAllowedLagEntries
                }
            }
            foreach ($warn in @($vc.warnings)) {
                Write-Host ("rollout_control_replica_warn: " + $warn)
                Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                        timestamp_utc = [DateTime]::UtcNow.ToString("o")
                        control_operation_id = $ControlOpId
                        controller_id = $ControllerId
                        queue_file = $QueuePath
                        action = $PlanAction
                        result = "replica_warning"
                        error = $warn
                    })
            }
            Save-ReplicaHealthState -Path $script:StateReplicaHealthPath -QueuePath $QueuePath -PlanAction $PlanAction -ControllerId $ControllerId -ControlOpId $ControlOpId -Source "startup" -Validation $vc -FailoverTriggered ([bool]$foStartup.changed) -FailoverDetails @($foStartup.details)
            Apply-ReplicaSloPolicy -Source "startup" -QueuePath $QueuePath -PlanAction $PlanAction -ControlOpId $ControlOpId -ControllerId $ControllerId -Validation $vc -AuditPath $AuditPath
            if (-not $vc.ok) {
                $msg = (($vc.errors -join "; "))
                throw ("state replica validation failed at startup: " + $msg)
            }
            $nextReplicaValidationMs = (Now-Ms) + ([int64]$script:StateReplicaValidationIntervalSec * 1000)
        }
    }

    $stop = $false
    if ([int]$startupRiskCapOverride -gt 0) {
        $effectiveConcurrent = [Math]::Max(1, [Math]::Min($effectiveConcurrent, [int]$startupRiskCapOverride))
    }
    if ([int]$startupRiskPauseOverride -gt 0) {
        $effectivePause = [Math]::Max($effectivePause, [int]$startupRiskPauseOverride)
    }
    if ($startupRiskBlocked) {
        $stop = $true
    }
    while (($pending.Count -gt 0 -and -not $stop) -or $running.Count -gt 0) {
        $progress = $false
        $activeConcurrent = $effectiveConcurrent
        $activePause = $effectivePause
        if ($script:RiskPolicyHotReloadEnabled -and (Now-Ms) -ge $script:RiskPolicyNextReloadCheckMs) {
            $script:RiskPolicyNextReloadCheckMs = (Now-Ms) + ([int64]$script:RiskPolicyHotReloadCheckSeconds * 1000)
            try {
                $queueInfo = Get-Item -LiteralPath $QueuePath -ErrorAction Stop
                $queueWriteTicks = [int64]$queueInfo.LastWriteTimeUtc.Ticks
                if ($queueWriteTicks -ne $script:RiskPolicyQueueLastWriteTicks) {
                    $hotRaw = Get-Content -LiteralPath $QueuePath -Raw
                    if (-not [string]::IsNullOrWhiteSpace($hotRaw)) {
                        $hotQueue = $hotRaw | ConvertFrom-Json -ErrorAction Stop
                        $oldRequested = [string]$script:RiskPolicyRequestedProfile
                        $oldActive = [string]$script:RiskPolicyActiveProfile
                        $oldResolved = [bool]$script:RiskPolicyProfileResolved
                        $oldConvergeSig = ("enabled={0}|max={1}|pause={2}|bs={3}|br={4}|site={5}|region={6}" -f [bool]$script:StateReplicaFailoverConvergeProfileGlobal.enabled, [int]$script:StateReplicaFailoverConvergeProfileGlobal.max_concurrent_plans, [int]$script:StateReplicaFailoverConvergeProfileGlobal.min_dispatch_pause_seconds, [bool]$script:StateReplicaFailoverConvergeProfileGlobal.block_on_snapshot_red, [bool]$script:StateReplicaFailoverConvergeProfileGlobal.block_on_replay_red, [int]$script:StateReplicaFailoverConvergeProfileBySite.Count, [int]$script:StateReplicaFailoverConvergeProfileByRegion.Count)
                        $oldDashboardSig = [string]$script:DecisionDashboardExportConfigSignature
                        $oldConsumerSig = [string]$script:DecisionDashboardConsumerConfigSignature
                        $oldRouteSig = [string]$script:DecisionRouteConfigSignature
                        $oldRiskBlockedMapBuildSig = [string]$script:RiskBlockedMapBuildConfigSignature
                        $oldRiskBlockedSelectSig = [string]$script:RiskBlockedSelectConfigSignature
                        $oldRiskMatrixSelectSig = [string]$script:RiskMatrixSelectConfigSignature
                        $oldRiskActionSig = [string]$script:RiskActionEvalConfigSignature
                        $oldRiskActionMatrixBuildSig = [string]$script:RiskActionMatrixBuildConfigSignature
                        $oldFailoverPolicyMatrixBuildSig = [string]$script:FailoverPolicyMatrixBuildConfigSignature
                        $oldRiskLevelSetSig = [string]$script:RiskLevelSetConfigSignature
                        $oldProfileSelectSig = [string]$script:RiskPolicyProfileSelectConfigSignature
                        $oldDeliverySig = [string]$script:DecisionDeliveryConfigSignature
                        $oldRolloutPolicyCliSig = [string]$script:RolloutPolicyCliConfigSignature
                        Apply-RiskPolicyConfigFromQueue -RiskPolicy $hotQueue.risk_policy
                        Apply-RiskLevelSetRuntimeConfig -RepoRoot $RepoRoot
                        Apply-RiskActionMatrixBuildRuntimeConfig -RepoRoot $RepoRoot
                        Rebuild-RiskPolicyDerivedState
                        Apply-DecisionDashboardExportRuntimeConfig -RepoRoot $RepoRoot -AuditPathDefault $AuditPath
                        Apply-DecisionDashboardConsumerRuntimeConfig -RepoRoot $RepoRoot
                        Apply-DecisionRouteRuntimeConfig -RepoRoot $RepoRoot
                        Apply-RiskBlockedMapBuildRuntimeConfig -RepoRoot $RepoRoot
                        Apply-RiskBlockedSelectRuntimeConfig -RepoRoot $RepoRoot
                        Apply-RiskMatrixSelectRuntimeConfig -RepoRoot $RepoRoot
                        Apply-RiskActionEvalRuntimeConfig -RepoRoot $RepoRoot
                        Apply-RiskActionMatrixBuildRuntimeConfig -RepoRoot $RepoRoot
                        Apply-FailoverPolicyMatrixBuildRuntimeConfig -RepoRoot $RepoRoot
                        Apply-DecisionDeliveryRuntimeConfig -RepoRoot $RepoRoot
                        Apply-RolloutPolicyCliRuntimeConfig -RepoRoot $RepoRoot
                        Apply-RolloutPolicyCliOverrides
                        Apply-FailoverConvergeConfigFromStateRecovery -StateRecovery $hotQueue.state_recovery -RiskPolicy $hotQueue.risk_policy
                        Rebuild-FailoverConvergeDerivedState
                        $newConvergeSig = ("enabled={0}|max={1}|pause={2}|bs={3}|br={4}|site={5}|region={6}" -f [bool]$script:StateReplicaFailoverConvergeProfileGlobal.enabled, [int]$script:StateReplicaFailoverConvergeProfileGlobal.max_concurrent_plans, [int]$script:StateReplicaFailoverConvergeProfileGlobal.min_dispatch_pause_seconds, [bool]$script:StateReplicaFailoverConvergeProfileGlobal.block_on_snapshot_red, [bool]$script:StateReplicaFailoverConvergeProfileGlobal.block_on_replay_red, [int]$script:StateReplicaFailoverConvergeProfileBySite.Count, [int]$script:StateReplicaFailoverConvergeProfileByRegion.Count)
                        $newDashboardSig = [string]$script:DecisionDashboardExportConfigSignature
                        $newConsumerSig = [string]$script:DecisionDashboardConsumerConfigSignature
                        $newRouteSig = [string]$script:DecisionRouteConfigSignature
                        $newRiskBlockedMapBuildSig = [string]$script:RiskBlockedMapBuildConfigSignature
                        $newRiskBlockedSelectSig = [string]$script:RiskBlockedSelectConfigSignature
                        $newRiskMatrixSelectSig = [string]$script:RiskMatrixSelectConfigSignature
                        $newRiskActionSig = [string]$script:RiskActionEvalConfigSignature
                        $newRiskActionMatrixBuildSig = [string]$script:RiskActionMatrixBuildConfigSignature
                        $newFailoverPolicyMatrixBuildSig = [string]$script:FailoverPolicyMatrixBuildConfigSignature
                        $newRiskLevelSetSig = [string]$script:RiskLevelSetConfigSignature
                        $newProfileSelectSig = [string]$script:RiskPolicyProfileSelectConfigSignature
                        $newDeliverySig = [string]$script:DecisionDeliveryConfigSignature
                        $newRolloutPolicyCliSig = [string]$script:RolloutPolicyCliConfigSignature
                        $script:RiskPolicyQueueLastWriteTicks = $queueWriteTicks
                        $script:RiskPolicyNextReloadCheckMs = (Now-Ms) + ([int64]$script:RiskPolicyHotReloadCheckSeconds * 1000)
                        Write-Host ("rollout_control_risk_policy_hot_reload: requested={0} active={1} resolved={2}" -f $script:RiskPolicyRequestedProfile, $script:RiskPolicyActiveProfile, $script:RiskPolicyProfileResolved)
                        Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                                timestamp_utc = [DateTime]::UtcNow.ToString("o")
                                control_operation_id = $ControlOpId
                                controller_id = $ControllerId
                                queue_file = $QueuePath
                                action = $PlanAction
                                result = "risk_policy_hot_reload"
                                old_requested_profile = $oldRequested
                                old_active_profile = $oldActive
                                old_profile_resolved = [bool]$oldResolved
                                requested_profile = [string]$script:RiskPolicyRequestedProfile
                                active_profile = [string]$script:RiskPolicyActiveProfile
                                profile_resolved = [bool]$script:RiskPolicyProfileResolved
                                error = ""
                            })
                        if ($oldRiskLevelSetSig -ne $newRiskLevelSetSig) {
                            Write-Host ("rollout_control_risk_level_set_hot_reload: old={0} new={1}" -f $oldRiskLevelSetSig, $newRiskLevelSetSig)
                            Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                                    timestamp_utc = [DateTime]::UtcNow.ToString("o")
                                    control_operation_id = $ControlOpId
                                    controller_id = $ControllerId
                                    queue_file = $QueuePath
                                    action = $PlanAction
                                    result = "risk_level_set_hot_reload"
                                    old_signature = $oldRiskLevelSetSig
                                    new_signature = $newRiskLevelSetSig
                                    risk_level_set_binary_path = [string]$script:RiskLevelSetBinaryPath
                                    error = ""
                                })
                        }
                        if ($oldRiskActionMatrixBuildSig -ne $newRiskActionMatrixBuildSig) {
                            Write-Host ("rollout_control_risk_action_matrix_build_hot_reload: old={0} new={1}" -f $oldRiskActionMatrixBuildSig, $newRiskActionMatrixBuildSig)
                            Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                                    timestamp_utc = [DateTime]::UtcNow.ToString("o")
                                    control_operation_id = $ControlOpId
                                    controller_id = $ControllerId
                                    queue_file = $QueuePath
                                    action = $PlanAction
                                    result = "risk_action_matrix_build_hot_reload"
                                    old_signature = $oldRiskActionMatrixBuildSig
                                    new_signature = $newRiskActionMatrixBuildSig
                                    risk_action_matrix_build_binary_path = [string]$script:RiskActionMatrixBuildBinaryPath
                                    error = ""
                                })
                        }
                        if ($oldFailoverPolicyMatrixBuildSig -ne $newFailoverPolicyMatrixBuildSig) {
                            Write-Host ("rollout_control_failover_policy_matrix_build_hot_reload: old={0} new={1}" -f $oldFailoverPolicyMatrixBuildSig, $newFailoverPolicyMatrixBuildSig)
                            Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                                    timestamp_utc = [DateTime]::UtcNow.ToString("o")
                                    control_operation_id = $ControlOpId
                                    controller_id = $ControllerId
                                    queue_file = $QueuePath
                                    action = $PlanAction
                                    result = "failover_policy_matrix_build_hot_reload"
                                    old_signature = $oldFailoverPolicyMatrixBuildSig
                                    new_signature = $newFailoverPolicyMatrixBuildSig
                                    failover_policy_matrix_build_binary_path = [string]$script:FailoverPolicyMatrixBuildBinaryPath
                                    error = ""
                                })
                        }
                        if ($oldProfileSelectSig -ne $newProfileSelectSig) {
                            Write-Host ("rollout_control_risk_profile_select_hot_reload: old={0} new={1}" -f $oldProfileSelectSig, $newProfileSelectSig)
                            Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                                    timestamp_utc = [DateTime]::UtcNow.ToString("o")
                                    control_operation_id = $ControlOpId
                                    controller_id = $ControllerId
                                    queue_file = $QueuePath
                                    action = $PlanAction
                                    result = "risk_profile_select_hot_reload"
                                    old_signature = $oldProfileSelectSig
                                    new_signature = $newProfileSelectSig
                                    profile_select_binary_path = [string]$script:RiskPolicyProfileSelectBinaryPath
                                    error = ""
                                })
                        }
                        if ($oldConvergeSig -ne $newConvergeSig) {
                            Write-Host ("rollout_control_replica_failover_converge_hot_reload: old={0} new={1}" -f $oldConvergeSig, $newConvergeSig)
                            Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                                    timestamp_utc = [DateTime]::UtcNow.ToString("o")
                                    control_operation_id = $ControlOpId
                                    controller_id = $ControllerId
                                    queue_file = $QueuePath
                                    action = $PlanAction
                                    result = "replica_failover_converge_hot_reload"
                                    old_signature = $oldConvergeSig
                                    new_signature = $newConvergeSig
                                    converge_enabled = [bool]$script:StateReplicaFailoverConvergeProfileGlobal.enabled
                                    converge_max_concurrent = [int]$script:StateReplicaFailoverConvergeProfileGlobal.max_concurrent_plans
                                    converge_min_dispatch_pause_seconds = [int]$script:StateReplicaFailoverConvergeProfileGlobal.min_dispatch_pause_seconds
                                    converge_site_override_count = [int]$script:StateReplicaFailoverConvergeProfileBySite.Count
                                    converge_region_override_count = [int]$script:StateReplicaFailoverConvergeProfileByRegion.Count
                                    error = ""
                                })
                        }
                        if ($oldDashboardSig -ne $newDashboardSig) {
                            $script:DecisionDashboardExportNextRunMs = 0
                            Write-Host ("rollout_control_decision_dashboard_export_hot_reload: old={0} new={1}" -f $oldDashboardSig, $newDashboardSig)
                            Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                                    timestamp_utc = [DateTime]::UtcNow.ToString("o")
                                    control_operation_id = $ControlOpId
                                    controller_id = $ControllerId
                                    queue_file = $QueuePath
                                    action = $PlanAction
                                    result = "decision_dashboard_export_hot_reload"
                                    old_signature = $oldDashboardSig
                                    new_signature = $newDashboardSig
                                    export_enabled = [bool]$script:DecisionDashboardExportEnabled
                                    export_check_seconds = [int]$script:DecisionDashboardExportCheckSeconds
                                    export_mode = [string]$script:DecisionDashboardExportMode
                                    export_tail = [int]$script:DecisionDashboardExportTail
                                    export_output_file = [string]$script:DecisionDashboardExportOutputPath
                                    error = ""
                                })
                        }
                        if ($oldConsumerSig -ne $newConsumerSig) {
                            $script:DecisionDashboardConsumerNextRunMs = 0
                            Write-Host ("rollout_control_decision_dashboard_consumer_hot_reload: old={0} new={1}" -f $oldConsumerSig, $newConsumerSig)
                            Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                                    timestamp_utc = [DateTime]::UtcNow.ToString("o")
                                    control_operation_id = $ControlOpId
                                    controller_id = $ControllerId
                                    queue_file = $QueuePath
                                    action = $PlanAction
                                    result = "decision_dashboard_consumer_hot_reload"
                                    old_signature = $oldConsumerSig
                                    new_signature = $newConsumerSig
                                    consumer_enabled = [bool]$script:DecisionDashboardConsumerEnabled
                                    consumer_check_seconds = [int]$script:DecisionDashboardConsumerCheckSeconds
                                    consumer_mode = [string]$script:DecisionDashboardConsumerMode
                                    consumer_tail = [int]$script:DecisionDashboardConsumerTail
                                    consumer_output_file = [string]$script:DecisionDashboardConsumerOutputPath
                                    consumer_alerts_file = [string]$script:DecisionDashboardConsumerAlertsPath
                                    error = ""
                                })
                        }
                        if ($oldRouteSig -ne $newRouteSig) {
                            Write-Host ("rollout_control_decision_route_hot_reload: old={0} new={1}" -f $oldRouteSig, $newRouteSig)
                            Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                                    timestamp_utc = [DateTime]::UtcNow.ToString("o")
                                    control_operation_id = $ControlOpId
                                    controller_id = $ControllerId
                                    queue_file = $QueuePath
                                    action = $PlanAction
                                    result = "decision_route_hot_reload"
                                    old_signature = $oldRouteSig
                                    new_signature = $newRouteSig
                                    route_binary_file = [string]$script:DecisionRouteBinaryPath
                                    error = ""
                                })
                        }
                        if ($oldRiskMatrixSelectSig -ne $newRiskMatrixSelectSig) {
                            Write-Host ("rollout_control_risk_matrix_select_hot_reload: old={0} new={1}" -f $oldRiskMatrixSelectSig, $newRiskMatrixSelectSig)
                            Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                                    timestamp_utc = [DateTime]::UtcNow.ToString("o")
                                    control_operation_id = $ControlOpId
                                    controller_id = $ControllerId
                                    queue_file = $QueuePath
                                    action = $PlanAction
                                    result = "risk_matrix_select_hot_reload"
                                    old_signature = $oldRiskMatrixSelectSig
                                    new_signature = $newRiskMatrixSelectSig
                                    risk_matrix_select_binary_file = [string]$script:RiskMatrixSelectBinaryPath
                                    error = ""
                                })
                        }
                        if ($oldRiskBlockedMapBuildSig -ne $newRiskBlockedMapBuildSig) {
                            Write-Host ("rollout_control_risk_blocked_map_build_hot_reload: old={0} new={1}" -f $oldRiskBlockedMapBuildSig, $newRiskBlockedMapBuildSig)
                            Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                                    timestamp_utc = [DateTime]::UtcNow.ToString("o")
                                    control_operation_id = $ControlOpId
                                    controller_id = $ControllerId
                                    queue_file = $QueuePath
                                    action = $PlanAction
                                    result = "risk_blocked_map_build_hot_reload"
                                    old_signature = $oldRiskBlockedMapBuildSig
                                    new_signature = $newRiskBlockedMapBuildSig
                                    risk_blocked_map_build_binary_file = [string]$script:RiskBlockedMapBuildBinaryPath
                                    error = ""
                                })
                        }
                        if ($oldRiskBlockedSelectSig -ne $newRiskBlockedSelectSig) {
                            Write-Host ("rollout_control_risk_blocked_select_hot_reload: old={0} new={1}" -f $oldRiskBlockedSelectSig, $newRiskBlockedSelectSig)
                            Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                                    timestamp_utc = [DateTime]::UtcNow.ToString("o")
                                    control_operation_id = $ControlOpId
                                    controller_id = $ControllerId
                                    queue_file = $QueuePath
                                    action = $PlanAction
                                    result = "risk_blocked_select_hot_reload"
                                    old_signature = $oldRiskBlockedSelectSig
                                    new_signature = $newRiskBlockedSelectSig
                                    risk_blocked_select_binary_file = [string]$script:RiskBlockedSelectBinaryPath
                                    error = ""
                                })
                        }
                        if ($oldRiskActionSig -ne $newRiskActionSig) {
                            Write-Host ("rollout_control_risk_action_eval_hot_reload: old={0} new={1}" -f $oldRiskActionSig, $newRiskActionSig)
                            Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                                    timestamp_utc = [DateTime]::UtcNow.ToString("o")
                                    control_operation_id = $ControlOpId
                                    controller_id = $ControllerId
                                    queue_file = $QueuePath
                                    action = $PlanAction
                                    result = "risk_action_eval_hot_reload"
                                    old_signature = $oldRiskActionSig
                                    new_signature = $newRiskActionSig
                                    risk_action_eval_binary_file = [string]$script:RiskActionEvalBinaryPath
                                    error = ""
                                })
                        }
                        if ($oldDeliverySig -ne $newDeliverySig) {
                            Write-Host ("rollout_control_decision_delivery_hot_reload: old={0} new={1}" -f $oldDeliverySig, $newDeliverySig)
                            Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                                    timestamp_utc = [DateTime]::UtcNow.ToString("o")
                                    control_operation_id = $ControlOpId
                                    controller_id = $ControllerId
                                    queue_file = $QueuePath
                                    action = $PlanAction
                                    result = "decision_delivery_hot_reload"
                                    old_signature = $oldDeliverySig
                                    new_signature = $newDeliverySig
                                    delivery_binary_file = [string]$script:DecisionDeliveryBinaryPath
                                    error = ""
                                })
                        }
                        if ($oldRolloutPolicyCliSig -ne $newRolloutPolicyCliSig) {
                            Write-Host ("rollout_control_rollout_policy_cli_hot_reload: old={0} new={1}" -f $oldRolloutPolicyCliSig, $newRolloutPolicyCliSig)
                            Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                                    timestamp_utc = [DateTime]::UtcNow.ToString("o")
                                    control_operation_id = $ControlOpId
                                    controller_id = $ControllerId
                                    queue_file = $QueuePath
                                    action = $PlanAction
                                    result = "rollout_policy_cli_hot_reload"
                                    old_signature = $oldRolloutPolicyCliSig
                                    new_signature = $newRolloutPolicyCliSig
                                    rollout_policy_cli_binary_file = [string]$script:RolloutPolicyCliBinaryPath
                                    error = ""
                                })
                        }
                    }
                }
            } catch {
                $reloadErr = $_.Exception.Message
                Write-Host ("rollout_control_risk_policy_hot_reload_warn: " + $reloadErr)
                Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                        timestamp_utc = [DateTime]::UtcNow.ToString("o")
                        control_operation_id = $ControlOpId
                        controller_id = $ControllerId
                        queue_file = $QueuePath
                        action = $PlanAction
                        result = "risk_policy_hot_reload_error"
                        requested_profile = [string]$script:RiskPolicyRequestedProfile
                        active_profile = [string]$script:RiskPolicyActiveProfile
                        profile_resolved = [bool]$script:RiskPolicyProfileResolved
                        error = $reloadErr
                    })
            }
        }
        if ($script:StateRecoveryEnabled -and $script:StateReplicaFailoverMode) {
            $convergeResolved = Select-FailoverConvergeProfile -SiteId $SiteId
            $convergeScope = [string]$convergeResolved.scope
            $convergeProfile = $convergeResolved.profile
            if ([bool]$convergeProfile.enabled) {
                $failoverConvergeCap = [Math]::Max(1, [int]$convergeProfile.max_concurrent_plans)
                $failoverConvergePauseFloor = [Math]::Max([int]$convergeProfile.min_dispatch_pause_seconds, [int]$script:StateReplicaFailoverCooldownSec)
                $failoverConvergePause = [Math]::Max([int]$activePause, $failoverConvergePauseFloor)
                $activeConcurrent = [Math]::Max(1, [Math]::Min($activeConcurrent, $failoverConvergeCap))
                $activePause = [Math]::Max($activePause, $failoverConvergePause)

                $snapshotGrade = ""
                $replayGrade = ""
                if ($null -ne $script:StateReplicaHealthState -and $null -ne $script:StateReplicaHealthState.last_validation) {
                    $snapshotGrade = [string]$script:StateReplicaHealthState.last_validation.snapshot_grade
                    $replayGrade = [string]$script:StateReplicaHealthState.last_validation.replay_grade
                }
                $sig = ("mode=failover|scope={0}|cap={1}|pause={2}|snapshot={3}|replay={4}|stable={5}|bs={6}|br={7}" -f $convergeScope, $activeConcurrent, $activePause, $snapshotGrade, $replayGrade, [int]$script:StateReplicaStableCycles, [bool]$convergeProfile.block_on_snapshot_red, [bool]$convergeProfile.block_on_replay_red)
                if ([string]$script:StateReplicaConvergeLastSignature -ne $sig) {
                    $script:StateReplicaConvergeLastSignature = $sig
                    Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                            timestamp_utc = [DateTime]::UtcNow.ToString("o")
                            control_operation_id = $ControlOpId
                            controller_id = $ControllerId
                            queue_file = $QueuePath
                            action = $PlanAction
                            result = "replica_failover_converge"
                            mode = "failover"
                            converge_scope = $convergeScope
                            stable_cycles = [int]$script:StateReplicaStableCycles
                            effective_max_concurrent = [int]$activeConcurrent
                            effective_dispatch_pause_seconds = [int]$activePause
                            snapshot_grade = $snapshotGrade
                            replay_grade = $replayGrade
                            block_on_snapshot_red = [bool]$convergeProfile.block_on_snapshot_red
                            block_on_replay_red = [bool]$convergeProfile.block_on_replay_red
                            error = ""
                        })
                }
                $snapshotRed = ($snapshotGrade -eq "red")
                $replayRed = ($replayGrade -eq "red")
                $convergeGradeBlocked = (($snapshotRed -and [bool]$convergeProfile.block_on_snapshot_red) -or ($replayRed -and [bool]$convergeProfile.block_on_replay_red))
                if ($convergeGradeBlocked -and [bool]$script:StateReplicaCircuitRedBlock -and -not $stop) {
                    Write-Host ("rollout_control_replica_failover_converge_block: scope={0} snapshot_grade={1} replay_grade={2}" -f $convergeScope, $snapshotGrade, $replayGrade)
                    Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                            timestamp_utc = [DateTime]::UtcNow.ToString("o")
                            control_operation_id = $ControlOpId
                            controller_id = $ControllerId
                            queue_file = $QueuePath
                            action = $PlanAction
                            result = "replica_failover_converge_blocked"
                            mode = "failover"
                            converge_scope = $convergeScope
                            stable_cycles = [int]$script:StateReplicaStableCycles
                            snapshot_grade = $snapshotGrade
                            replay_grade = $replayGrade
                            block_on_snapshot_red = [bool]$convergeProfile.block_on_snapshot_red
                            block_on_replay_red = [bool]$convergeProfile.block_on_replay_red
                            error = "dispatch blocked by failover converge guard"
                        })
                    $stop = $true
                }
            } else {
                $script:StateReplicaConvergeLastSignature = ""
            }
        } else {
            $script:StateReplicaConvergeLastSignature = ""
        }

        if ($script:StateReplicaCircuitEnabled) {
            $activeConcurrent = [Math]::Max(1, [Math]::Min($activeConcurrent, [int]$script:StateReplicaCircuitCurrentConcurrentCap))
            $activePause = [Math]::Max($activePause, [int]$script:StateReplicaCircuitCurrentPauseSeconds)
            if ([bool]$script:StateReplicaCircuitCurrentBlockDispatch) {
                if (-not $stop) {
                    Write-Host ("rollout_control_replica_circuit_block: rule={0} grade={1}, dispatch blocked" -f $script:StateReplicaCircuitCurrentRule, $script:StateReplicaCircuitCurrentGrade)
                    Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                            timestamp_utc = [DateTime]::UtcNow.ToString("o")
                            control_operation_id = $ControlOpId
                            controller_id = $ControllerId
                            queue_file = $QueuePath
                            action = $PlanAction
                            result = "replica_circuit_blocked"
                            error = ("rule=" + $script:StateReplicaCircuitCurrentRule + " grade=" + $script:StateReplicaCircuitCurrentGrade)
                        })
                }
                $stop = $true
            }
        }
        $siteRepAgingCycle = Apply-SiteConflictReputationAging
        foreach ($ev in @($siteRepAgingCycle)) {
            Write-Host ("rollout_control_site_reputation_aging: source=cycle site={0} old_penalty={1} new_penalty={2} reputation_score={3}" -f $ev.site_id, $ev.old_penalty, $ev.new_penalty, $ev.reputation_score)
            Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                    timestamp_utc = [DateTime]::UtcNow.ToString("o")
                    control_operation_id = $ControlOpId
                    controller_id = $ControllerId
                    queue_file = $QueuePath
                    action = $PlanAction
                    result = "consensus_accountability_reputation_aging"
                    source = "cycle"
                    site_id = [string]$ev.site_id
                    old_penalty = [int]$ev.old_penalty
                    new_penalty = [int]$ev.new_penalty
                    recover_points = [int]$ev.recover_points
                    base_priority = [int]$ev.base_priority
                    effective_priority = [int]$ev.effective_priority
                    reputation_score = [double]$ev.reputation_score
                    error = ""
                })
        }
        $siteRiskCycle = Predict-SiteConflictRisk
        if ([bool]$siteRiskCycle.enabled) {
            Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                    timestamp_utc = [DateTime]::UtcNow.ToString("o")
                    control_operation_id = $ControlOpId
                    controller_id = $ControllerId
                    queue_file = $QueuePath
                    action = $PlanAction
                    result = "consensus_accountability_risk_predict"
                    source = "cycle"
                    worst_site_id = [string]$siteRiskCycle.worst_site_id
                    worst_level = [string]$siteRiskCycle.worst_level
                    worst_score = [double]$siteRiskCycle.worst_score
                    total_sites = [int]$siteRiskCycle.total_sites
                    error = ""
                })
            $riskPolicy = Resolve-SiteConflictRiskThrottlePolicy -Risk $siteRiskCycle -Source "cycle" -SiteId ([string]$siteRiskCycle.worst_site_id)
            Emit-SiteRiskThrottlePolicyAuditIfChanged -Source "cycle" -Risk $siteRiskCycle -Policy $riskPolicy -AuditPath $AuditPath -QueuePath $QueuePath -PlanAction $PlanAction -ControlOpId $ControlOpId -ControllerId $ControllerId
            if ([int]$riskPolicy.cap_concurrent -gt 0) {
                $activeConcurrent = [Math]::Max(1, [Math]::Min($activeConcurrent, [int]$riskPolicy.cap_concurrent))
            }
            if ([int]$riskPolicy.pause_seconds -gt 0) {
                $activePause = [Math]::Max($activePause, [int]$riskPolicy.pause_seconds)
            }
            $worstSiteId = [string]$siteRiskCycle.worst_site_id
            $worstLevel = [string]$siteRiskCycle.worst_level
            $worstScore = [double]$siteRiskCycle.worst_score
            $worstTrend = 0.0
            if (-not [string]::IsNullOrWhiteSpace($worstSiteId) -and $null -ne $script:SiteConflictRiskState -and $null -ne $script:SiteConflictRiskState.sites -and $script:SiteConflictRiskState.sites.ContainsKey($worstSiteId)) {
                $trendRaw = $script:SiteConflictRiskState.sites[$worstSiteId].trend
                if ($null -ne $trendRaw) {
                    $worstTrend = [double]$trendRaw
                }
            }
            $forecastScore = $worstScore
            if ($worstTrend -gt 0.0) {
                $forecastScore = [Math]::Min(100.0, $worstScore + ([Math]::Abs($worstTrend) * 3.0))
            }
            $forecastLevel = Score-ToRiskLevel -Score $forecastScore
            $rankMap = @{
                "green" = 0
                "yellow" = 1
                "orange" = 2
                "red" = 3
            }
            $worstRank = 0
            $forecastRank = 0
            if ($rankMap.ContainsKey($worstLevel)) { $worstRank = [int]$rankMap[$worstLevel] }
            if ($rankMap.ContainsKey($forecastLevel)) { $forecastRank = [int]$rankMap[$forecastLevel] }
            if ($forecastRank -gt $worstRank -and $forecastRank -ge 2) {
                Write-Host ("rollout_control_site_risk_forecast_warn: site={0} now_level={1} now_score={2} trend={3} forecast_level={4} forecast_score={5}" -f $worstSiteId, $worstLevel, [Math]::Round($worstScore, 4), [Math]::Round($worstTrend, 4), $forecastLevel, [Math]::Round($forecastScore, 4))
                Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                        timestamp_utc = [DateTime]::UtcNow.ToString("o")
                        control_operation_id = $ControlOpId
                        controller_id = $ControllerId
                        queue_file = $QueuePath
                        action = $PlanAction
                        result = "consensus_accountability_risk_forecast"
                        source = "cycle"
                        worst_site_id = $worstSiteId
                        risk_level_now = $worstLevel
                        risk_score_now = [Math]::Round($worstScore, 4)
                        risk_trend = [Math]::Round($worstTrend, 4)
                        forecast_level = $forecastLevel
                        forecast_score = [Math]::Round($forecastScore, 4)
                        error = ""
                    })
                $forecastRisk = [pscustomobject]@{
                    worst_site_id = $worstSiteId
                    worst_level = $forecastLevel
                    worst_score = $forecastScore
                    total_sites = [int]$siteRiskCycle.total_sites
                }
                $forecastPolicy = Resolve-SiteConflictRiskThrottlePolicy -Risk $forecastRisk -Source "cycle" -SiteId $worstSiteId
                Emit-SiteRiskThrottlePolicyAuditIfChanged -Source "cycle_forecast" -Risk $forecastRisk -Policy $forecastPolicy -AuditPath $AuditPath -QueuePath $QueuePath -PlanAction $PlanAction -ControlOpId $ControlOpId -ControllerId $ControllerId
                if ([int]$forecastPolicy.cap_concurrent -gt 0) {
                    $activeConcurrent = [Math]::Max(1, [Math]::Min($activeConcurrent, [int]$forecastPolicy.cap_concurrent))
                }
                if ([int]$forecastPolicy.pause_seconds -gt 0) {
                    $activePause = [Math]::Max($activePause, [int]$forecastPolicy.pause_seconds)
                }
                if ([bool]$forecastPolicy.block_dispatch -and -not $stop) {
                    Write-Host ("rollout_control_site_risk_forecast_block: site={0} level={1} score={2}" -f $worstSiteId, $forecastLevel, [Math]::Round($forecastScore, 4))
                    Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                            timestamp_utc = [DateTime]::UtcNow.ToString("o")
                            control_operation_id = $ControlOpId
                            controller_id = $ControllerId
                            queue_file = $QueuePath
                            action = $PlanAction
                            result = "site_risk_forecast_throttle_blocked"
                            source = "cycle"
                            worst_site_id = $worstSiteId
                            risk_level = $forecastLevel
                            risk_score = [Math]::Round($forecastScore, 4)
                            risk_policy_scope = [string]$forecastPolicy.scope
                            risk_policy_rule_source = [string]$forecastPolicy.rule_source
                            risk_policy_site_priority = [int]$forecastPolicy.site_priority
                            risk_policy_min_site_priority = [int]$forecastPolicy.min_site_priority
                            risk_policy_priority_gate = [string]$forecastPolicy.priority_gate
                            error = "dispatch blocked by forecast risk policy"
                        })
                    $stop = $true
                }
                Emit-RolloutDecisionSummaryIfChanged -Source "cycle_forecast" -Risk $forecastRisk -RiskPolicy $forecastPolicy -Role $controllerRole -EffectiveConcurrent $activeConcurrent -EffectivePauseSeconds $activePause -DispatchBlocked ([bool]$stop) -AuditPath $AuditPath -QueuePath $QueuePath -PlanAction $PlanAction -ControlOpId $ControlOpId -ControllerId $ControllerId
            }
            if ([bool]$riskPolicy.block_dispatch -and -not $stop) {
                Write-Host ("rollout_control_site_risk_block: level={0} worst_site={1} score={2}" -f $riskPolicy.level, $siteRiskCycle.worst_site_id, $siteRiskCycle.worst_score)
                Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                        timestamp_utc = [DateTime]::UtcNow.ToString("o")
                        control_operation_id = $ControlOpId
                        controller_id = $ControllerId
                        queue_file = $QueuePath
                        action = $PlanAction
                        result = "site_risk_throttle_blocked"
                        source = "cycle"
                        worst_site_id = [string]$siteRiskCycle.worst_site_id
                        risk_level = [string]$riskPolicy.level
                        risk_score = [double]$siteRiskCycle.worst_score
                        risk_policy_scope = [string]$riskPolicy.scope
                        risk_policy_rule_source = [string]$riskPolicy.rule_source
                        risk_policy_site_priority = [int]$riskPolicy.site_priority
                        risk_policy_min_site_priority = [int]$riskPolicy.min_site_priority
                        risk_policy_priority_gate = [string]$riskPolicy.priority_gate
                        error = "dispatch blocked by site conflict risk policy"
                    })
                $stop = $true
            }
            Emit-RolloutDecisionSummaryIfChanged -Source "cycle" -Risk $siteRiskCycle -RiskPolicy $riskPolicy -Role $controllerRole -EffectiveConcurrent $activeConcurrent -EffectivePauseSeconds $activePause -DispatchBlocked ([bool]$stop) -AuditPath $AuditPath -QueuePath $QueuePath -PlanAction $PlanAction -ControlOpId $ControlOpId -ControllerId $ControllerId
        }
        Invoke-DecisionDashboardExportIfDue -Source "cycle" -AuditPath $AuditPath -QueuePath $QueuePath -PlanAction $PlanAction -ControlOpId $ControlOpId -ControllerId $ControllerId
        Invoke-DecisionDashboardConsumerIfDue -Source "cycle" -AuditPath $AuditPath -QueuePath $QueuePath -PlanAction $PlanAction -ControlOpId $ControlOpId -ControllerId $ControllerId

        if ((Now-Ms) -ge $nextLeaseHeartbeatMs) {
            Write-LeaseHeartbeat -Stream $leaseStream -Controller $ControllerId -Operation $ControlOpId -TtlSec $effectiveLeaseTtl
            $nextLeaseHeartbeatMs = (Now-Ms) + ([int64]$effectiveLeaseHeartbeat * 1000)
        }
        if ($script:StateRecoveryEnabled -and $script:StateReplicaValidationEnabled -and (Now-Ms) -ge $nextReplicaValidationMs) {
            $vcCycle = Validate-StateReplicas -AllowedLagEntries $script:StateReplicaAllowedLagEntries
            $foCycle = [pscustomobject]@{
                changed = $false
                details = @()
                skipped = ""
            }
            if (-not $vcCycle.ok -and $script:StateReplicaAutoFailoverEnabled) {
                $foCycle = Try-AutoFailoverStateReplicas -AllowedLagEntries $script:StateReplicaAllowedLagEntries -Source "cycle" -Validation $vcCycle
                if ($foCycle.changed) {
                    Write-Host ("rollout_control_replica_failover: source=cycle details={0}" -f ($foCycle.details -join ","))
                    Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                            timestamp_utc = [DateTime]::UtcNow.ToString("o")
                            control_operation_id = $ControlOpId
                            controller_id = $ControllerId
                            queue_file = $QueuePath
                            action = $PlanAction
                            result = "replica_failover"
                            error = ("source=cycle details=" + ($foCycle.details -join ","))
                        })
                    $vcCycle = Validate-StateReplicas -AllowedLagEntries $script:StateReplicaAllowedLagEntries
                } elseif (-not [string]::IsNullOrWhiteSpace([string]$foCycle.skipped) -and ([string]$foCycle.skipped).StartsWith("policy_blocked")) {
                    Write-Host ("rollout_control_replica_failover_policy_blocked: source=cycle {0}" -f $foCycle.skipped)
                    Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                            timestamp_utc = [DateTime]::UtcNow.ToString("o")
                            control_operation_id = $ControlOpId
                            controller_id = $ControllerId
                            queue_file = $QueuePath
                            action = $PlanAction
                            result = "replica_failover_policy_blocked"
                            error = ("source=cycle " + [string]$foCycle.skipped)
                        })
                }
            }
            if (-not [bool]$vcCycle.ok -and $script:StateReplicaFailoverMode) {
                $script:StateReplicaStableCycles = 0
            }
            $sbCycle = [pscustomobject]@{
                changed = $false
                details = @()
                skipped = ""
            }
            if ([bool]$vcCycle.ok) {
                $sbCycle = Try-SwitchbackStateReplicas -Validation $vcCycle
                if ($sbCycle.changed) {
                    Write-Host ("rollout_control_replica_switchback: source=cycle details={0}" -f ($sbCycle.details -join ","))
                    Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                            timestamp_utc = [DateTime]::UtcNow.ToString("o")
                            control_operation_id = $ControlOpId
                            controller_id = $ControllerId
                            queue_file = $QueuePath
                            action = $PlanAction
                            result = "replica_switchback"
                            error = ("source=cycle details=" + ($sbCycle.details -join ","))
                        })
                    $vcCycle = Validate-StateReplicas -AllowedLagEntries $script:StateReplicaAllowedLagEntries
                }
            }
            foreach ($warn in @($vcCycle.warnings)) {
                Write-Host ("rollout_control_replica_warn: " + $warn)
                Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                        timestamp_utc = [DateTime]::UtcNow.ToString("o")
                        control_operation_id = $ControlOpId
                        controller_id = $ControllerId
                        queue_file = $QueuePath
                        action = $PlanAction
                        result = "replica_warning"
                        error = $warn
                    })
            }
            Save-ReplicaHealthState -Path $script:StateReplicaHealthPath -QueuePath $QueuePath -PlanAction $PlanAction -ControllerId $ControllerId -ControlOpId $ControlOpId -Source "cycle" -Validation $vcCycle -FailoverTriggered ([bool]$foCycle.changed) -FailoverDetails @($foCycle.details)
            Apply-ReplicaSloPolicy -Source "cycle" -QueuePath $QueuePath -PlanAction $PlanAction -ControlOpId $ControlOpId -ControllerId $ControllerId -Validation $vcCycle -AuditPath $AuditPath
            if (-not $vcCycle.ok) {
                $errMsg = (($vcCycle.errors -join "; "))
                throw ("state replica validation failed during run: " + $errMsg)
            }
            $nextReplicaValidationMs = (Now-Ms) + ([int64]$script:StateReplicaValidationIntervalSec * 1000)
        }

        while (-not $stop) {
            $now = Now-Ms
            $rows = @()
            for ($i = 0; $i -lt $pending.Count; $i++) {
                $e = $pending[$i]
                if ([int64]$e.next_run -le $now) {
                    $rows += [pscustomobject]@{ idx = $i; pr = [int]$e.priority; nr = [int64]$e.next_run; nm = [string]$e.name }
                }
            }
            if ($rows.Count -eq 0) { break }
            $rows = $rows | Sort-Object @{ Expression = "pr"; Descending = $true }, @{ Expression = "nr"; Descending = $false }, @{ Expression = "nm"; Descending = $false }

            $selected = -1
            if ($running.Count -lt $activeConcurrent) {
                foreach ($r in $rows) {
                    $cand = $pending[[int]$r.idx]
                    $pendingCount = Pending-InRegion -Pending $pending -Region ([string]$cand.region)
                    $cap = Effective-RegionCap -Caps $regionCaps -Region ([string]$cand.region) -Fallback $activeConcurrent -PendingCount $pendingCount
                    $used = Running-InRegion -Running $running -Region ([string]$cand.region)
                    if ($used -lt $cap) {
                        $selected = [int]$r.idx
                        break
                    }
                }
            }

            if ($selected -lt 0) {
                if ($effectivePreempt -and $running.Count -gt 0) {
                    $headPr = [int]$pending[[int]$rows[0].idx].priority
                    $victim = $running | Where-Object { [bool]$_.preemptible -and ([int]$_.priority -lt $headPr) } | Sort-Object @{ Expression = "priority"; Descending = $false }, @{ Expression = "started"; Descending = $false } | Select-Object -First 1
                    if ($null -ne $victim) {
                        Stop-Process -Id $victim.process.Id -Force -ErrorAction SilentlyContinue
                        $running = @($running | Where-Object { $_.process.Id -ne $victim.process.Id })
                        $re = Clone-Entry -Source $victim -Attempt ([int]$victim.attempt) -NextRun ((Now-Ms) + ([int64]$victim.preempt_requeue * 1000))
                        $pending += $re
                        Write-Host ("rollout_control_preempt: victim={0} victim_priority={1} incoming_priority={2}" -f $victim.name, $victim.priority, $headPr)
                        Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                                timestamp_utc = [DateTime]::UtcNow.ToString("o")
                                control_operation_id = $ControlOpId
                                controller_id = $ControllerId
                                queue_file = $QueuePath
                                plan = $victim.name
                                plan_operation_id = $victim.opid
                                action = $victim.action
                                priority = $victim.priority
                                attempt = $victim.attempt
                                region = $victim.region
                                result = "preempted"
                                error = ("preempted by priority=" + $headPr)
                            })
                        $progress = $true
                        continue
                    }
                }
                break
            }

            $entry = $pending[$selected]
            $pending = Remove-At -ArrayValue $pending -Index $selected

            $reserve = Try-Reserve-Dedupe -Entry $entry -ControlOpId $ControlOpId
            if (-not $reserve.ok) {
                $doneSkip += 1
                Write-Host ("rollout_control_skip: plan={0} reason={1}" -f $entry.name, $reserve.reason)
                Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                        timestamp_utc = [DateTime]::UtcNow.ToString("o")
                        control_operation_id = $ControlOpId
                        controller_id = $ControllerId
                        queue_file = $QueuePath
                        plan = $entry.name
                        plan_operation_id = $entry.opid
                        action = $entry.action
                        priority = $entry.priority
                        attempt = $entry.attempt
                        region = $entry.region
                        result = "dedupe_blocked"
                        error = $reserve.reason
                    })
                Write-ReplayEvent -Path $script:StateReplayPath -Obj ([pscustomobject][ordered]@{
                        timestamp_utc = [DateTime]::UtcNow.ToString("o")
                        control_operation_id = $ControlOpId
                        controller_id = $ControllerId
                        queue_file = $QueuePath
                        plan = $entry.name
                        action = $entry.action
                        attempt = $entry.attempt
                        result = "dedupe_blocked"
                        error = $reserve.reason
                    })
                $progress = $true
                continue
            }

            $consensusGate = Try-SiteConsensus -Entry $entry -ControlOpId $ControlOpId
            foreach ($acc in @($consensusGate.accountability_events)) {
                Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                        timestamp_utc = [DateTime]::UtcNow.ToString("o")
                        control_operation_id = $ControlOpId
                        controller_id = $ControllerId
                        queue_file = $QueuePath
                        plan = $entry.name
                        plan_operation_id = $entry.opid
                        action = $entry.action
                        priority = $entry.priority
                        attempt = $entry.attempt
                        region = $entry.region
                        result = "consensus_accountability"
                        site_id = [string]$acc.site_id
                        accountability_event = [string]$acc.event
                        accountability_role = [string]$acc.role
                        accountability_rule = [string]$acc.rule
                        accountability_delta = [int]$acc.delta
                        accountability_old_penalty = [int]$acc.old_penalty
                        accountability_new_penalty = [int]$acc.new_penalty
                        accountability_base_priority = [int]$acc.base_priority
                        accountability_effective_priority = [int]$acc.effective_priority
                        accountability_reputation_score = [double]$acc.reputation_score
                        error = [string]$acc.reason
                    })
            }
            if (-not $consensusGate.ok) {
                if ([string]$consensusGate.status -eq "waiting_quorum") {
                    $pending += (Clone-Entry -Source $entry -Attempt ([int]$entry.attempt) -NextRun ((Now-Ms) + ([int64]$script:SiteConsensusRetrySec * 1000)))
                    Write-Host ("rollout_control_consensus_wait: plan={0} reason={1}" -f $entry.name, $consensusGate.reason)
                    Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                            timestamp_utc = [DateTime]::UtcNow.ToString("o")
                            control_operation_id = $ControlOpId
                            controller_id = $ControllerId
                            queue_file = $QueuePath
                            plan = $entry.name
                            plan_operation_id = $entry.opid
                            action = $entry.action
                            priority = $entry.priority
                            attempt = $entry.attempt
                            region = $entry.region
                            result = "consensus_wait"
                            error = $consensusGate.reason
                        })
                    Write-ReplayEvent -Path $script:StateReplayPath -Obj ([pscustomobject][ordered]@{
                            timestamp_utc = [DateTime]::UtcNow.ToString("o")
                            control_operation_id = $ControlOpId
                            controller_id = $ControllerId
                            queue_file = $QueuePath
                            plan = $entry.name
                            action = $entry.action
                            attempt = $entry.attempt
                            result = "consensus_wait"
                            error = $consensusGate.reason
                        })
                    $progress = $true
                    if ($activePause -gt 0) { Start-Sleep -Seconds $activePause }
                    continue
                }
                $doneSkip += 1
                Finalize-Dedupe -Entry $entry -ControlOpId $ControlOpId -Status "failed"
                Write-Host ("rollout_control_skip: plan={0} reason={1}" -f $entry.name, $consensusGate.reason)
                Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                        timestamp_utc = [DateTime]::UtcNow.ToString("o")
                        control_operation_id = $ControlOpId
                        controller_id = $ControllerId
                        queue_file = $QueuePath
                        plan = $entry.name
                        plan_operation_id = $entry.opid
                        action = $entry.action
                        priority = $entry.priority
                        attempt = $entry.attempt
                        region = $entry.region
                        result = "consensus_blocked"
                        error = $consensusGate.reason
                    })
                Write-ReplayEvent -Path $script:StateReplayPath -Obj ([pscustomobject][ordered]@{
                        timestamp_utc = [DateTime]::UtcNow.ToString("o")
                        control_operation_id = $ControlOpId
                        controller_id = $ControllerId
                        queue_file = $QueuePath
                        plan = $entry.name
                        action = $entry.action
                        attempt = $entry.attempt
                        result = "consensus_blocked"
                        error = $consensusGate.reason
                    })
                $progress = $true
                continue
            }

            $relayDiscovery = Try-DiscoverOverlayRelays -RepoRoot $RepoRoot -Entry $entry
            if ($null -ne $relayDiscovery -and [bool]$relayDiscovery.invoked) {
                if ([bool]$relayDiscovery.ok) {
                    Write-Host ("rollout_control_relay_discovery: plan={0} status=ok" -f $entry.name)
                    Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                            timestamp_utc = [DateTime]::UtcNow.ToString("o")
                            control_operation_id = $ControlOpId
                            controller_id = $ControllerId
                            queue_file = $QueuePath
                            plan = $entry.name
                            plan_operation_id = $entry.opid
                            action = $entry.action
                            priority = $entry.priority
                            attempt = $entry.attempt
                            region = $entry.region
                            result = "relay_discovery_merged"
                            error = [string]$relayDiscovery.message
                            overlay_route_relay_discovery_file = [string]$entry.overlay_route_relay_discovery_file
                            overlay_route_relay_discovery_cooldown_seconds = [string]$entry.overlay_route_relay_discovery_cooldown_seconds
                            overlay_route_relay_discovery_default_health = [string]$relayDiscovery.default_health
                            overlay_route_relay_discovery_default_enabled = [string]$relayDiscovery.default_enabled
                            overlay_route_relay_discovery_http_urls = [string]$relayDiscovery.http_urls
                            overlay_route_relay_discovery_http_timeout_ms = [string]$relayDiscovery.http_timeout_ms
                            overlay_route_relay_discovery_source_weights = [string]$entry.overlay_route_relay_discovery_source_weights
                            overlay_route_relay_discovery_source_reputation_file = [string]$relayDiscovery.source_reputation_file
                            overlay_route_relay_discovery_source_decay = [string]$relayDiscovery.source_decay
                            overlay_route_relay_discovery_source_penalty_on_fail = [string]$relayDiscovery.source_penalty_on_fail
                            overlay_route_relay_discovery_source_recover_on_success = [string]$relayDiscovery.source_recover_on_success
                            overlay_route_relay_discovery_source_blacklist_threshold = [string]$relayDiscovery.source_blacklist_threshold
                            overlay_route_relay_discovery_source_denylist = [string]$relayDiscovery.source_denylist
                            overlay_route_relay_discovery_http_urls_file = [string]$relayDiscovery.http_urls_file
                            overlay_route_relay_discovery_seed_region = [string]$relayDiscovery.seed_region
                            overlay_route_relay_discovery_seed_mode = [string]$relayDiscovery.seed_mode
                            overlay_route_relay_discovery_seed_profile = [string]$relayDiscovery.seed_profile
                            overlay_route_relay_discovery_seed_failover_state_file = [string]$relayDiscovery.seed_failover_state_file
                            overlay_route_relay_discovery_seed_priority = [string]$relayDiscovery.seed_priority
                            overlay_route_relay_discovery_seed_success_rate_threshold = [string]$relayDiscovery.seed_success_rate_threshold
                            overlay_route_relay_discovery_seed_cooldown_seconds = [string]$relayDiscovery.seed_cooldown_seconds
                            overlay_route_relay_discovery_seed_max_consecutive_failures = [string]$relayDiscovery.seed_max_consecutive_failures
                            overlay_route_relay_discovery_region_priority = [string]$relayDiscovery.region_priority
                            overlay_route_relay_discovery_region_failover_threshold = [string]$relayDiscovery.region_failover_threshold
                            overlay_route_relay_discovery_region_cooldown_seconds = [string]$relayDiscovery.region_cooldown_seconds
                            overlay_route_relay_discovery_relay_score_smoothing_alpha = [string]$relayDiscovery.relay_score_smoothing_alpha
                            overlay_route_relay_discovery_seed_selected = [string]$relayDiscovery.seed_selected
                            overlay_route_relay_discovery_seed_failover_reason = [string]$relayDiscovery.seed_failover_reason
                            overlay_route_relay_discovery_seed_recover_at_unix_ms = [string]$relayDiscovery.seed_recover_at_unix_ms
                            overlay_route_relay_discovery_seed_cooldown_skip = [string]$relayDiscovery.seed_cooldown_skip
                            overlay_route_relay_discovery_relay_selected = [string]$relayDiscovery.relay_selected
                            overlay_route_relay_discovery_relay_score = [string]$relayDiscovery.relay_score
                            overlay_route_relay_discovery_region_failover_reason = [string]$relayDiscovery.region_failover_reason
                            overlay_route_relay_discovery_region_recover_at_unix_ms = [string]$relayDiscovery.region_recover_at_unix_ms
                            overlay_route_relay_directory_file = [string]$entry.overlay_route_relay_directory_file
                        })
                } else {
                    Write-Host ("rollout_control_warn: relay discovery failed, plan={0}, err={1}" -f $entry.name, [string]$relayDiscovery.message)
                    Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                            timestamp_utc = [DateTime]::UtcNow.ToString("o")
                            control_operation_id = $ControlOpId
                            controller_id = $ControllerId
                            queue_file = $QueuePath
                            plan = $entry.name
                            plan_operation_id = $entry.opid
                            action = $entry.action
                            priority = $entry.priority
                            attempt = $entry.attempt
                            region = $entry.region
                            result = "relay_discovery_error"
                            error = [string]$relayDiscovery.message
                            overlay_route_relay_discovery_reason = [string]$relayDiscovery.reason
                            overlay_route_relay_discovery_file = [string]$entry.overlay_route_relay_discovery_file
                            overlay_route_relay_discovery_cooldown_seconds = [string]$entry.overlay_route_relay_discovery_cooldown_seconds
                            overlay_route_relay_discovery_default_health = [string]$relayDiscovery.default_health
                            overlay_route_relay_discovery_default_enabled = [string]$relayDiscovery.default_enabled
                            overlay_route_relay_discovery_http_urls = [string]$relayDiscovery.http_urls
                            overlay_route_relay_discovery_http_timeout_ms = [string]$relayDiscovery.http_timeout_ms
                            overlay_route_relay_discovery_source_weights = [string]$entry.overlay_route_relay_discovery_source_weights
                            overlay_route_relay_discovery_source_reputation_file = [string]$relayDiscovery.source_reputation_file
                            overlay_route_relay_discovery_source_decay = [string]$relayDiscovery.source_decay
                            overlay_route_relay_discovery_source_penalty_on_fail = [string]$relayDiscovery.source_penalty_on_fail
                            overlay_route_relay_discovery_source_recover_on_success = [string]$relayDiscovery.source_recover_on_success
                            overlay_route_relay_discovery_source_blacklist_threshold = [string]$relayDiscovery.source_blacklist_threshold
                            overlay_route_relay_discovery_source_denylist = [string]$relayDiscovery.source_denylist
                            overlay_route_relay_discovery_http_urls_file = [string]$relayDiscovery.http_urls_file
                            overlay_route_relay_discovery_seed_region = [string]$relayDiscovery.seed_region
                            overlay_route_relay_discovery_seed_mode = [string]$relayDiscovery.seed_mode
                            overlay_route_relay_discovery_seed_profile = [string]$relayDiscovery.seed_profile
                            overlay_route_relay_discovery_seed_failover_state_file = [string]$relayDiscovery.seed_failover_state_file
                            overlay_route_relay_discovery_seed_priority = [string]$relayDiscovery.seed_priority
                            overlay_route_relay_discovery_seed_success_rate_threshold = [string]$relayDiscovery.seed_success_rate_threshold
                            overlay_route_relay_discovery_seed_cooldown_seconds = [string]$relayDiscovery.seed_cooldown_seconds
                            overlay_route_relay_discovery_seed_max_consecutive_failures = [string]$relayDiscovery.seed_max_consecutive_failures
                            overlay_route_relay_discovery_region_priority = [string]$relayDiscovery.region_priority
                            overlay_route_relay_discovery_region_failover_threshold = [string]$relayDiscovery.region_failover_threshold
                            overlay_route_relay_discovery_region_cooldown_seconds = [string]$relayDiscovery.region_cooldown_seconds
                            overlay_route_relay_discovery_relay_score_smoothing_alpha = [string]$relayDiscovery.relay_score_smoothing_alpha
                            overlay_route_relay_discovery_seed_selected = [string]$relayDiscovery.seed_selected
                            overlay_route_relay_discovery_seed_failover_reason = [string]$relayDiscovery.seed_failover_reason
                            overlay_route_relay_discovery_seed_recover_at_unix_ms = [string]$relayDiscovery.seed_recover_at_unix_ms
                            overlay_route_relay_discovery_seed_cooldown_skip = [string]$relayDiscovery.seed_cooldown_skip
                            overlay_route_relay_discovery_relay_selected = [string]$relayDiscovery.relay_selected
                            overlay_route_relay_discovery_relay_score = [string]$relayDiscovery.relay_score
                            overlay_route_relay_discovery_region_failover_reason = [string]$relayDiscovery.region_failover_reason
                            overlay_route_relay_discovery_region_recover_at_unix_ms = [string]$relayDiscovery.region_recover_at_unix_ms
                            overlay_route_relay_directory_file = [string]$entry.overlay_route_relay_directory_file
                        })
                }
            }

            $relayHealthRefresh = Try-RefreshOverlayRelayDirectoryHealth -RepoRoot $RepoRoot -Entry $entry
            if ($null -ne $relayHealthRefresh -and [bool]$relayHealthRefresh.invoked) {
                if ([bool]$relayHealthRefresh.ok) {
                    Write-Host ("rollout_control_relay_health_refresh: plan={0} status=ok mode={1}" -f $entry.name, [string]$relayHealthRefresh.mode)
                    Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                            timestamp_utc = [DateTime]::UtcNow.ToString("o")
                            control_operation_id = $ControlOpId
                            controller_id = $ControllerId
                            queue_file = $QueuePath
                            plan = $entry.name
                            plan_operation_id = $entry.opid
                            action = $entry.action
                            priority = $entry.priority
                            attempt = $entry.attempt
                            region = $entry.region
                            result = "relay_health_refreshed"
                            error = [string]$relayHealthRefresh.message
                            overlay_route_relay_health_refresh_mode = [string]$relayHealthRefresh.mode
                            overlay_route_relay_health_refresh_timeout_ms = [string]$relayHealthRefresh.timeout_ms
                            overlay_route_relay_health_refresh_alpha = [string]$relayHealthRefresh.alpha
                            overlay_route_relay_directory_file = [string]$entry.overlay_route_relay_directory_file
                        })
                } else {
                    Write-Host ("rollout_control_warn: relay health refresh failed, plan={0}, err={1}" -f $entry.name, [string]$relayHealthRefresh.message)
                    Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                            timestamp_utc = [DateTime]::UtcNow.ToString("o")
                            control_operation_id = $ControlOpId
                            controller_id = $ControllerId
                            queue_file = $QueuePath
                            plan = $entry.name
                            plan_operation_id = $entry.opid
                            action = $entry.action
                            priority = $entry.priority
                            attempt = $entry.attempt
                            region = $entry.region
                            result = "relay_health_refresh_error"
                            error = [string]$relayHealthRefresh.message
                            overlay_route_relay_health_refresh_reason = [string]$relayHealthRefresh.reason
                            overlay_route_relay_health_refresh_mode = [string]$relayHealthRefresh.mode
                            overlay_route_relay_health_refresh_timeout_ms = [string]$relayHealthRefresh.timeout_ms
                            overlay_route_relay_health_refresh_alpha = [string]$relayHealthRefresh.alpha
                            overlay_route_relay_directory_file = [string]$entry.overlay_route_relay_directory_file
                        })
                }
            }

            try {
                $job = Start-Entry -RepoRoot $RepoRoot -RolloutScript $RolloutScript -Entry $entry
                $running += $job
                Write-Host ("rollout_control_dispatch: plan={0} pid={1} priority={2} attempt={3} region={4}" -f $job.name, $job.process.Id, $job.priority, $job.attempt, $job.region)
                Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                        timestamp_utc = [DateTime]::UtcNow.ToString("o")
                        control_operation_id = $ControlOpId
                        controller_id = $ControllerId
                        queue_file = $QueuePath
                        plan = $job.name
                        plan_operation_id = $job.opid
                        action = $job.action
                        priority = $job.priority
                        attempt = $job.attempt
                        region = $job.region
                        result = "dispatched"
                        error = ""
                    })
            } catch {
                $msg = $_.Exception.Message
                Adaptive-Observe -Region ([string]$entry.region) -Failed $true
                if ($entry.attempt -le $entry.retry_max) {
                    $baseDelay = [int]([Math]::Max(1, $entry.retry_backoff_sec * [Math]::Pow($entry.retry_backoff_factor, ([int]$entry.attempt - 1))))
                    $delay = Effective-RetryDelaySec -BaseDelaySec $baseDelay -Region ([string]$entry.region)
                    $autoPenalty = Try-ApplyAutoRelayPenalty -Entry $entry
                    $autoPenaltyRelayId = ""
                    $autoPenaltyStep = ""
                    $autoPenaltyBaseStep = ""
                    $autoPenaltyStreakBoost = ""
                    $autoPenaltyHealthFactor = ""
                    $autoPenaltyRelayHealth = ""
                    if ($null -ne $autoPenalty) {
                        $autoPenaltyRelayId = [string]$autoPenalty.relay_id
                        $autoPenaltyStep = [string]$autoPenalty.step
                        $autoPenaltyBaseStep = [string]$autoPenalty.base_step
                        $autoPenaltyStreakBoost = [string]$autoPenalty.streak_boost
                        $autoPenaltyHealthFactor = [string]$autoPenalty.health_factor
                        $autoPenaltyRelayHealth = [string]$autoPenalty.relay_health
                    }
                    $pending += (Clone-Entry -Source $entry -Attempt ([int]$entry.attempt + 1) -NextRun ((Now-Ms) + ([int64]$delay * 1000)))
                    Write-Host ("rollout_control_retry_schedule: plan={0} next_attempt={1} delay_sec={2}" -f $entry.name, ([int]$entry.attempt + 1), $delay)
                    Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                            timestamp_utc = [DateTime]::UtcNow.ToString("o")
                            control_operation_id = $ControlOpId
                            controller_id = $ControllerId
                            queue_file = $QueuePath
                            plan = $entry.name
                            plan_operation_id = $entry.opid
                            action = $entry.action
                            priority = $entry.priority
                            attempt = $entry.attempt
                            region = $entry.region
                            result = "retry_scheduled"
                            error = ("dispatch error: " + $msg)
                            overlay_route_auto_penalty_relay_id = $autoPenaltyRelayId
                            overlay_route_auto_penalty_step = $autoPenaltyStep
                            overlay_route_auto_penalty_base_step = $autoPenaltyBaseStep
                            overlay_route_auto_penalty_streak_boost = $autoPenaltyStreakBoost
                            overlay_route_auto_penalty_health_factor = $autoPenaltyHealthFactor
                            overlay_route_auto_penalty_relay_health = $autoPenaltyRelayHealth
                            overlay_route_relay_penalty_delta = [string]$entry.overlay_route_relay_penalty_delta
                        })
                } else {
                    $doneErr += 1
                    Finalize-Dedupe -Entry $entry -ControlOpId $ControlOpId -Status "failed"
                    Write-Host ("rollout_control_error: plan={0} err={1}" -f $entry.name, $msg)
                    Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                            timestamp_utc = [DateTime]::UtcNow.ToString("o")
                            control_operation_id = $ControlOpId
                            controller_id = $ControllerId
                            queue_file = $QueuePath
                            plan = $entry.name
                            plan_operation_id = $entry.opid
                            action = $entry.action
                            priority = $entry.priority
                            attempt = $entry.attempt
                            region = $entry.region
                            result = "error"
                            error = ("dispatch error: " + $msg)
                        })
                    if (-not $ContinueOnPlanFailure) { $stop = $true }
                }
            }

            $progress = $true
            if ($activePause -gt 0) { Start-Sleep -Seconds $activePause }
        }

        if ($running.Count -gt 0) {
            $still = @()
            foreach ($job in $running) {
                $p = Get-Process -Id $job.process.Id -ErrorAction SilentlyContinue
                if ($null -eq $p -or $p.HasExited) {
                    $code = 1
                    try { $code = $job.process.ExitCode } catch { $code = 1 }
                    if ($code -eq 0) {
                        $doneOk += 1
                        Adaptive-Observe -Region ([string]$job.region) -Failed $false
                        Finalize-Dedupe -Entry $job -ControlOpId $ControlOpId -Status "done"
                        Write-Host ("rollout_control_done: plan={0} result=ok" -f $job.name)
                        Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                                timestamp_utc = [DateTime]::UtcNow.ToString("o")
                                control_operation_id = $ControlOpId
                                controller_id = $ControllerId
                                queue_file = $QueuePath
                                plan = $job.name
                                plan_operation_id = $job.opid
                                action = $job.action
                                priority = $job.priority
                                attempt = $job.attempt
                                region = $job.region
                                result = "ok"
                                error = ""
                                stdout_log = $job.stdout_log
                                stderr_log = $job.stderr_log
                            })
                    } else {
                        Adaptive-Observe -Region ([string]$job.region) -Failed $true
                        if ($job.attempt -le $job.retry_max) {
                            $baseDelay = [int]([Math]::Max(1, $job.retry_backoff_sec * [Math]::Pow($job.retry_backoff_factor, ([int]$job.attempt - 1))))
                            $delay = Effective-RetryDelaySec -BaseDelaySec $baseDelay -Region ([string]$job.region)
                            $autoPenalty = Try-ApplyAutoRelayPenalty -Entry $job
                            $autoPenaltyRelayId = ""
                            $autoPenaltyStep = ""
                            $autoPenaltyBaseStep = ""
                            $autoPenaltyStreakBoost = ""
                            $autoPenaltyHealthFactor = ""
                            $autoPenaltyRelayHealth = ""
                            if ($null -ne $autoPenalty) {
                                $autoPenaltyRelayId = [string]$autoPenalty.relay_id
                                $autoPenaltyStep = [string]$autoPenalty.step
                                $autoPenaltyBaseStep = [string]$autoPenalty.base_step
                                $autoPenaltyStreakBoost = [string]$autoPenalty.streak_boost
                                $autoPenaltyHealthFactor = [string]$autoPenalty.health_factor
                                $autoPenaltyRelayHealth = [string]$autoPenalty.relay_health
                            }
                            $pending += (Clone-Entry -Source $job -Attempt ([int]$job.attempt + 1) -NextRun ((Now-Ms) + ([int64]$delay * 1000)))
                            Write-Host ("rollout_control_retry_schedule: plan={0} next_attempt={1} delay_sec={2}" -f $job.name, ([int]$job.attempt + 1), $delay)
                            Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                                    timestamp_utc = [DateTime]::UtcNow.ToString("o")
                                    control_operation_id = $ControlOpId
                                    controller_id = $ControllerId
                                    queue_file = $QueuePath
                                    plan = $job.name
                                    plan_operation_id = $job.opid
                                    action = $job.action
                                    priority = $job.priority
                                    attempt = $job.attempt
                                    region = $job.region
                                    result = "retry_scheduled"
                                    error = ("exit_code=" + $code)
                                    overlay_route_auto_penalty_relay_id = $autoPenaltyRelayId
                                    overlay_route_auto_penalty_step = $autoPenaltyStep
                                    overlay_route_auto_penalty_base_step = $autoPenaltyBaseStep
                                    overlay_route_auto_penalty_streak_boost = $autoPenaltyStreakBoost
                                    overlay_route_auto_penalty_health_factor = $autoPenaltyHealthFactor
                                    overlay_route_auto_penalty_relay_health = $autoPenaltyRelayHealth
                                    overlay_route_relay_penalty_delta = [string]$job.overlay_route_relay_penalty_delta
                                })
                        } else {
                            $doneErr += 1
                            Finalize-Dedupe -Entry $job -ControlOpId $ControlOpId -Status "failed"
                            $msg = ("exit_code=" + $code)
                            Write-Host ("rollout_control_done: plan={0} result=error {1}" -f $job.name, $msg)
                            Write-Audit -Path $AuditPath -Obj ([pscustomobject][ordered]@{
                                    timestamp_utc = [DateTime]::UtcNow.ToString("o")
                                    control_operation_id = $ControlOpId
                                    controller_id = $ControllerId
                                    queue_file = $QueuePath
                                    plan = $job.name
                                    plan_operation_id = $job.opid
                                    action = $job.action
                                    priority = $job.priority
                                    attempt = $job.attempt
                                    region = $job.region
                                    result = "error"
                                    error = $msg
                                    stdout_log = $job.stdout_log
                                    stderr_log = $job.stderr_log
                                })
                            if (-not $ContinueOnPlanFailure) { $stop = $true }
                        }
                    }
                    $progress = $true
                } else {
                    $still += $job
                }
            }
            $running = $still
        }

        if ($script:StateRecoveryEnabled -and $progress) {
            Save-StateRecoverySnapshot -Path $script:StateSnapshotPath -QueuePath $QueuePath -PlanAction $PlanAction -ControllerId $ControllerId -ControlOpId $ControlOpId -Status "running" -Pending $pending -Running $running -DoneOk $doneOk -DoneErr $doneErr -DoneSkip $doneSkip
        }

        if (-not $progress) {
            Start-Sleep -Seconds $effectivePoll
        }
    }

    $stateStatus = "completed"
    Write-Host ("rollout_control_out: ok={0} err={1} skip={2} op={3}" -f $doneOk, $doneErr, $doneSkip, $ControlOpId)
    if ($doneErr -gt 0) {
        $stateStatus = "failed"
        if ($script:StateRecoveryEnabled) {
            Save-StateRecoverySnapshot -Path $script:StateSnapshotPath -QueuePath $QueuePath -PlanAction $PlanAction -ControllerId $ControllerId -ControlOpId $ControlOpId -Status $stateStatus -Pending $pending -Running $running -DoneOk $doneOk -DoneErr $doneErr -DoneSkip $doneSkip
        }
        throw ("rollout control completed with errors: " + $doneErr)
    }
    if ($script:StateRecoveryEnabled) {
        Save-StateRecoverySnapshot -Path $script:StateSnapshotPath -QueuePath $QueuePath -PlanAction $PlanAction -ControllerId $ControllerId -ControlOpId $ControlOpId -Status $stateStatus -Pending $pending -Running $running -DoneOk $doneOk -DoneErr $doneErr -DoneSkip $doneSkip
    }
}
finally {
    if ($script:StateRecoveryEnabled) {
        if ($stateStatus -eq "init") {
            $stateStatus = "aborted"
        }
        Save-StateRecoverySnapshot -Path $script:StateSnapshotPath -QueuePath $QueuePath -PlanAction $PlanAction -ControllerId $ControllerId -ControlOpId $ControlOpId -Status $stateStatus -Pending $pending -Running $running -DoneOk $doneOk -DoneErr $doneErr -DoneSkip $doneSkip
    }
    if ($script:SiteConsensusEnabled) {
        Save-SiteConsensusState -Path $script:SiteConsensusPath -State $script:SiteConsensusState
        if ($script:SiteConflictAccountabilityEnabled) {
            Save-SiteConflictAccountabilityState -Path $script:SiteConflictAccountabilityPath -State $script:SiteConflictAccountabilityState
            if ($script:SiteConflictRiskPredictorEnabled) {
                Save-SiteConflictRiskState -Path $script:SiteConflictRiskStatePath -State $script:SiteConflictRiskState
            }
        }
    }
    if ($script:AdaptiveEnabled -and $script:AdaptiveDirty) {
        Save-AdaptiveState -Path $script:AdaptiveStatePath -State $script:AdaptiveState
    }
    if ($script:StateRecoveryEnabled -and $script:StateReplicaSloEnabled) {
        Save-ReplicaSloState -Path $script:StateReplicaSloPath -State $script:StateReplicaSloState
    }
    if ($script:StateRecoveryEnabled -and $script:StateReplicaAdaptiveEnabled) {
        Save-ReplicaAdaptiveState -Path $script:StateReplicaAdaptivePath -State $script:StateReplicaAdaptiveState
    }
    if ($script:StateRecoveryEnabled -and $script:StateReplicaDrillScoreEnabled) {
        Save-ReplicaDrillScoreState -Path $script:StateReplicaDrillScorePath -State $script:StateReplicaDrillScoreState
    }
    if ($null -ne $leaseStream) {
        try { $leaseStream.Dispose() } catch {}
    }
}




