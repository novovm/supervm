param(
    [string]$RepoRoot = "",
    [string]$GatewayBind = "127.0.0.1:9899",
    [UInt64]$ChainId = 1,
    [UInt64]$DurationMinutes = 5,
    [UInt64]$IntervalSeconds = 5,
    [UInt64]$WarmupSeconds = 6,
    [switch]$SkipBuild,
    [switch]$FreshRlpxProfile,
    [switch]$EnableSwapPriority = $true,
    [string]$SummaryOut = "artifacts/migration/evm-mempool-autopilot-summary.json"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Resolve-RootPath {
    param([string]$Root)
    if (-not $Root) {
        return (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
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

function Invoke-Observation {
    param(
        [string]$ScriptPath,
        [hashtable]$Profile,
        [string]$SummaryPath,
        [string]$GatewayBind,
        [UInt64]$ChainId,
        [UInt64]$DurationMinutes,
        [UInt64]$IntervalSeconds,
        [UInt64]$WarmupSeconds,
        [bool]$SkipBuild,
        [bool]$FreshRlpxProfile,
        [bool]$EnableSwapPriority
    )
    $args = @(
        "-ExecutionPolicy", "Bypass",
        "-File", $ScriptPath,
        "-GatewayBind", $GatewayBind,
        "-ChainId", ([string][UInt64]$ChainId),
        "-DurationMinutes", ([string][UInt64]$DurationMinutes),
        "-IntervalSeconds", ([string][UInt64]$IntervalSeconds),
        "-WarmupSeconds", ([string][UInt64]$WarmupSeconds),
        "-EnablePluginMempoolIngest",
        "-RlpxHelloProfile", "geth",
        "-PluginMinCandidates", ([string][UInt64]$Profile.PluginMinCandidates),
        "-RlpxMaxPeersPerTick", ([string][UInt64]$Profile.RlpxMaxPeersPerTick),
        "-DnsDiscoveryMaxEnodes", ([string][UInt64]$Profile.DnsDiscoveryMaxEnodes),
        "-RlpxRecentNewHashMin", ([string][UInt64]$Profile.RlpxRecentNewHashMin),
        "-RlpxCoreRecentGossipWindowMs", ([string][UInt64]$Profile.RlpxCoreRecentGossipWindowMs),
        "-RlpxCoreLockMs", ([string][UInt64]$Profile.RlpxCoreLockMs),
        "-SummaryOut", $SummaryPath
    )
    if ($SkipBuild) {
        $args += "-SkipBuild"
    }
    if ($FreshRlpxProfile) {
        $args += "-FreshRlpxProfile"
    }
    if ($EnableSwapPriority) {
        $args += "-EnableSwapPriority"
    }
    Write-Host ("[autopilot] running profile={0} args={1}" -f $Profile.Name, ($args -join " "))
    & powershell @args
}

function Get-ProfileScore {
    param([object]$SummaryJson)
    if ($null -eq $SummaryJson -or $null -eq $SummaryJson.smoke) {
        return [double]-1e12
    }
    $smokePass = [bool]$SummaryJson.smoke.passed
    $coreMax = [double]$SummaryJson.aggregate.max_plugin_tier_core_items
    $ready = [double]$SummaryJson.smoke.observed_ready
    $newHash = [double]$SummaryJson.smoke.observed_new_pooled
    $pooled = [double]$SummaryJson.smoke.observed_pooled
    $top1Unique = [double]$SummaryJson.peer_contribution.top1_unique_hash_share_pct
    $smokePenalty = if ($smokePass) { 0.0 } else { 1e9 }
    # Higher core/newHash/pooled/ready is better; lower top1Unique is better.
    return (20000.0 * $coreMax) + (1000.0 * $ready) + (20.0 * $newHash) + (2.0 * $pooled) + (100.0 - $top1Unique) - $smokePenalty
}

$RepoRoot = Resolve-RootPath -Root $RepoRoot
Set-Location $RepoRoot

$obsScript = Resolve-FullPath -Root $RepoRoot -Value "scripts/migration/run_evm_uniswap_observation_window.ps1"
if (-not (Test-Path $obsScript)) {
    throw ("missing script: {0}" -f $obsScript)
}

$SummaryOut = Resolve-FullPath -Root $RepoRoot -Value $SummaryOut
$SummaryDir = Split-Path -Parent $SummaryOut
if (-not (Test-Path $SummaryDir)) {
    New-Item -ItemType Directory -Force -Path $SummaryDir | Out-Null
}

$timestamp = Get-Date -Format "yyyyMMdd-HHmmss"
$profiles = @(
    [ordered]@{
        Name = "balanced"
        PluginMinCandidates = 900
        RlpxMaxPeersPerTick = 32
        DnsDiscoveryMaxEnodes = 180
        RlpxRecentNewHashMin = 4
        RlpxCoreRecentGossipWindowMs = 1800000
        RlpxCoreLockMs = 1800000
    },
    [ordered]@{
        Name = "conservative"
        PluginMinCandidates = 600
        RlpxMaxPeersPerTick = 24
        DnsDiscoveryMaxEnodes = 120
        RlpxRecentNewHashMin = 8
        RlpxCoreRecentGossipWindowMs = 1200000
        RlpxCoreLockMs = 1200000
    },
    [ordered]@{
        Name = "wide-scan"
        PluginMinCandidates = 1200
        RlpxMaxPeersPerTick = 32
        DnsDiscoveryMaxEnodes = 240
        RlpxRecentNewHashMin = 4
        RlpxCoreRecentGossipWindowMs = 1800000
        RlpxCoreLockMs = 1800000
    }
)

$attempts = New-Object System.Collections.ArrayList
$best = $null
$bestScore = [double]-1e12

for ($i = 0; $i -lt $profiles.Count; $i++) {
    $profile = $profiles[$i]
    $attemptSummary = Join-Path $SummaryDir ("evm-uniswap-observation-window-summary-autopilot-{0}-{1}.json" -f $timestamp, $profile.Name)
    $useFresh = [bool]$FreshRlpxProfile -and ($i -eq 0)
    Invoke-Observation -ScriptPath $obsScript -Profile $profile -SummaryPath $attemptSummary -GatewayBind $GatewayBind -ChainId $ChainId -DurationMinutes $DurationMinutes -IntervalSeconds $IntervalSeconds -WarmupSeconds $WarmupSeconds -SkipBuild:$SkipBuild -FreshRlpxProfile:$useFresh -EnableSwapPriority:$EnableSwapPriority

    if (-not (Test-Path $attemptSummary)) {
        [void]$attempts.Add([ordered]@{
            profile = $profile.Name
            summary = $attemptSummary
            score = [double]-1e12
            error = "missing summary output"
        })
        continue
    }

    $j = Get-Content -Path $attemptSummary -Raw | ConvertFrom-Json
    $score = Get-ProfileScore -SummaryJson $j
    $entry = [ordered]@{
        profile = $profile.Name
        summary = $attemptSummary
        smoke = [bool]$j.smoke.passed
        ready = [UInt64]$j.smoke.observed_ready
        new_hash = [UInt64]$j.smoke.observed_new_pooled
        pooled = [UInt64]$j.smoke.observed_pooled
        core_max = [UInt64]$j.aggregate.max_plugin_tier_core_items
        top1_unique_pct = [double]$j.peer_contribution.top1_unique_hash_share_pct
        score = [double]$score
    }
    [void]$attempts.Add($entry)

    if ($score -gt $bestScore) {
        $bestScore = $score
        $best = $entry
    }

    if ($entry.smoke -and $entry.core_max -ge 4 -and $entry.top1_unique_pct -le 50.0) {
        Write-Host ("[autopilot] early-stop: profile={0} core={1} top1U={2}%" -f $entry.profile, $entry.core_max, $entry.top1_unique_pct)
        break
    }
}

if ($null -eq $best) {
    throw "autopilot failed: no successful run summary"
}

$result = [ordered]@{
    generated_at_utc = [DateTimeOffset]::UtcNow.ToString("o")
    gateway_bind = $GatewayBind
    chain_id = [UInt64]$ChainId
    duration_minutes = [UInt64]$DurationMinutes
    interval_seconds = [UInt64]$IntervalSeconds
    warmup_seconds = [UInt64]$WarmupSeconds
    enable_swap_priority = [bool]$EnableSwapPriority
    skip_build = [bool]$SkipBuild
    fresh_rlpx_profile = [bool]$FreshRlpxProfile
    best_profile = $best
    attempts = $attempts
}

$json = $result | ConvertTo-Json -Depth 10
Set-Content -Path $SummaryOut -Value $json -Encoding UTF8
Write-Host ("autopilot summary written: {0}" -f $SummaryOut)
Write-Host ("autopilot best: profile={0} core={1} top1U={2}% score={3}" -f $best.profile, $best.core_max, $best.top1_unique_pct, $best.score)
