param(
    [string]$RepoRoot = "",
    [string]$GethRoot = "D:\WEB3_AI\go-ethereum",
    [string]$Since = "",
    [UInt64]$SinceHours = 48,
    [UInt64]$MaxCommits = 80,
    [string]$BaselineCommit = "",
    [string]$StatePath = "artifacts/migration/state/geth-upstream-impact-scan-state.json",
    [string]$SummaryOut = "artifacts/migration/geth-upstream-impact-summary.json",
    [string]$MarkdownOut = "artifacts/migration/geth-upstream-impact-summary.md",
    [switch]$NoUpdateState
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

function Ensure-DirectoryForFile {
    param([string]$Path)
    $dir = Split-Path -Parent $Path
    if ($dir -and -not (Test-Path $dir)) {
        New-Item -ItemType Directory -Path $dir -Force | Out-Null
    }
}

function Invoke-Git {
    param(
        [string]$WorkTree,
        [string[]]$GitArgs
    )
    $output = & git -C $WorkTree @GitArgs 2>&1
    if ($LASTEXITCODE -ne 0) {
        $joined = ($output | Out-String).Trim()
        throw ("git command failed in {0}: git {1}`n{2}" -f $WorkTree, ($GitArgs -join " "), $joined)
    }
    return $output
}

function Test-GitCommitExists {
    param(
        [string]$WorkTree,
        [string]$Commit
    )
    if ([string]::IsNullOrWhiteSpace($Commit)) {
        return $false
    }
    & git -C $WorkTree cat-file -e "$Commit`^{commit}" 2>$null
    return ($LASTEXITCODE -eq 0)
}

function Read-JsonFileOrNull {
    param([string]$Path)
    if (-not (Test-Path $Path)) {
        return $null
    }
    return (Get-Content -Path $Path -Raw | ConvertFrom-Json)
}

function Get-FirstOutputLine {
    param($Value)
    if ($Value -is [System.Array]) {
        if ($Value.Count -eq 0) {
            return ""
        }
        return [string]$Value[0]
    }
    return [string]$Value
}

function Get-RiskRank {
    param([string]$Risk)
    switch ($Risk) {
        "none" { return 0 }
        "low" { return 1 }
        "medium" { return 2 }
        "high" { return 3 }
        "critical" { return 4 }
        default { return 0 }
    }
}

function Get-RiskPriority {
    param([string]$Risk)
    switch ($Risk) {
        "critical" { return "P0-immediate" }
        "high" { return "P1-soon" }
        "medium" { return "P2-watch" }
        "low" { return "P3-monitor" }
        default { return "P4-ignore" }
    }
}

function Get-PathImpact {
    param([string]$Path)

    $normalized = $Path.Replace("\", "/")
    if ($normalized -match "^p2p/" -or $normalized -match "^crypto/ecies/" -or $normalized -match "^rlp/") {
        return [ordered]@{
            area = "devp2p_rlpx"
            risk = "critical"
            reason = "wire-handshake transport compatibility changed"
        }
    }
    if ($normalized -match "^eth/protocols/") {
        return [ordered]@{
            area = "eth_subprotocol"
            risk = "critical"
            reason = "eth subprotocol message behavior changed"
        }
    }
    if ($normalized -match "^core/types/") {
        return [ordered]@{
            area = "tx_wire_types"
            risk = "critical"
            reason = "transaction/header wire types changed"
        }
    }
    if (
        $normalized -match "^eth/handler(\.go|/)" -or
        $normalized -match "^eth/sync(\.go|/)" -or
        $normalized -match "^eth/downloader/" -or
        $normalized -match "^eth/syncer/"
    ) {
        return [ordered]@{
            area = "eth_runtime_sync"
            risk = "high"
            reason = "runtime sync/handler behavior changed"
        }
    }
    if ($normalized -match "^core/txpool/") {
        return [ordered]@{
            area = "txpool_semantics"
            risk = "high"
            reason = "txpool interfaces/selection behavior changed"
        }
    }
    if ($normalized -match "^internal/ethapi/" -or $normalized -match "^eth/api_backend\.go") {
        return [ordered]@{
            area = "rpc_api_surface"
            risk = "medium"
            reason = "rpc api behavior/surface changed"
        }
    }
    if (
        $normalized -match "^miner/" -or
        $normalized -match "^consensus/" -or
        $normalized -match "^eth/catalyst/"
    ) {
        return [ordered]@{
            area = "block_building"
            risk = "medium"
            reason = "block construction path changed"
        }
    }
    if (
        $normalized -match "^core/state/" -or
        $normalized -match "^triedb/" -or
        $normalized -match "^trie/"
    ) {
        return [ordered]@{
            area = "state_storage"
            risk = "medium"
            reason = "state storage/trie behavior changed"
        }
    }
    if ($normalized -match "^cmd/evm/") {
        return [ordered]@{
            area = "tooling_cli"
            risk = "low"
            reason = "cli or t8n tooling changed"
        }
    }
    return [ordered]@{
        area = "other"
        risk = "low"
        reason = "non-critical path for current supervm evm plugin"
    }
}

function Get-AreaWatchPaths {
    param([string]$Area)
    switch ($Area) {
        "devp2p_rlpx" {
            return @(
                "crates/gateways/evm-gateway/src/rpc_gateway_exec_cfg.rs",
                "crates/novovm-network/src/transport.rs"
            )
        }
        "eth_subprotocol" {
            return @(
                "crates/gateways/evm-gateway/src/rpc_gateway_exec_cfg.rs",
                "crates/plugins/evm/plugin/src/lib.rs"
            )
        }
        "tx_wire_types" {
            return @(
                "crates/novovm-adapter-novovm/src/lib.rs",
                "crates/plugins/evm/plugin/src/lib.rs"
            )
        }
        "eth_runtime_sync" {
            return @(
                "crates/gateways/evm-gateway/src/rpc_eth_sync.rs",
                "crates/gateways/evm-gateway/src/rpc_gateway_exec_cfg.rs",
                "crates/novovm-network/src/runtime_status.rs"
            )
        }
        "txpool_semantics" {
            return @(
                "crates/plugins/evm/plugin/src/lib.rs",
                "crates/gateways/evm-gateway/src/rpc_gateway_exec_cfg.rs",
                "crates/gateways/evm-gateway/src/rpc_eth_state.rs"
            )
        }
        "rpc_api_surface" {
            return @(
                "crates/gateways/evm-gateway/src/main.rs",
                "crates/gateways/evm-gateway/src/rpc_gateway_exec_cfg.rs"
            )
        }
        "block_building" {
            return @(
                "crates/novovm-adapter-novovm/src/lib.rs",
                "scripts/migration/tmp_run_step2_execproof.ps1",
                "scripts/migration/run_evm_full_lifecycle_autopilot.ps1"
            )
        }
        "state_storage" {
            return @(
                "crates/novovm-adapter-novovm/src/lib.rs",
                "scripts/migration/tmp_run_step2_execproof.ps1"
            )
        }
        "tooling_cli" {
            return @("scripts/migration/")
        }
        default {
            return @()
        }
    }
}

function Get-AreaChecks {
    param([string]$Area)
    switch ($Area) {
        "devp2p_rlpx" {
            return @(
                "powershell -ExecutionPolicy Bypass -File scripts/migration/run_evm_eth_plugin_session_canary.ps1 -SkipBuild -DurationSeconds 60"
            )
        }
        "eth_subprotocol" {
            return @(
                "powershell -ExecutionPolicy Bypass -File scripts/migration/run_evm_uniswap_observation_window.ps1 -SkipBuild -EnablePluginMempoolIngest -DurationMinutes 2 -IntervalSeconds 5 -WarmupSeconds 6"
            )
        }
        "txpool_semantics" {
            return @(
                "powershell -ExecutionPolicy Bypass -File scripts/migration/run_evm_uniswap_observation_window.ps1 -SkipBuild -EnablePluginMempoolIngest -DurationMinutes 2 -IntervalSeconds 5 -WarmupSeconds 6"
            )
        }
        "eth_runtime_sync" {
            return @(
                "powershell -ExecutionPolicy Bypass -File scripts/migration/run_evm_mainnet_read_attach.ps1 -SkipBuild"
            )
        }
        "rpc_api_surface" {
            return @(
                "powershell -ExecutionPolicy Bypass -File scripts/migration/run_evm_full_lifecycle_autopilot.ps1 -SkipBuild -AutopilotDurationMinutes 1 -ExecproofDurationMinutes 1"
            )
        }
        "block_building" {
            return @(
                "powershell -ExecutionPolicy Bypass -File scripts/migration/run_evm_full_lifecycle_autopilot.ps1 -SkipBuild -AutopilotDurationMinutes 1 -ExecproofDurationMinutes 1"
            )
        }
        "state_storage" {
            return @(
                "powershell -ExecutionPolicy Bypass -File scripts/migration/tmp_run_step2_execproof.ps1 -SkipBuild -DurationMinutes 1"
            )
        }
        default {
            return @()
        }
    }
}

$RepoRoot = Resolve-RootPath -Root $RepoRoot
Set-Location $RepoRoot

$GethRoot = Resolve-FullPath -Root $RepoRoot -Value $GethRoot
if (-not (Test-Path $GethRoot)) {
    throw ("geth root does not exist: {0}" -f $GethRoot)
}
if (-not (Test-Path (Join-Path $GethRoot ".git"))) {
    throw ("not a git repo: {0}" -f $GethRoot)
}

$StatePath = Resolve-FullPath -Root $RepoRoot -Value $StatePath
$SummaryOut = Resolve-FullPath -Root $RepoRoot -Value $SummaryOut
$MarkdownOut = Resolve-FullPath -Root $RepoRoot -Value $MarkdownOut
Ensure-DirectoryForFile -Path $StatePath
Ensure-DirectoryForFile -Path $SummaryOut
Ensure-DirectoryForFile -Path $MarkdownOut

$state = Read-JsonFileOrNull -Path $StatePath
$head = (Get-FirstOutputLine -Value (Invoke-Git -WorkTree $GethRoot -GitArgs @("rev-parse", "HEAD"))).Trim()
$headShort = (Get-FirstOutputLine -Value (Invoke-Git -WorkTree $GethRoot -GitArgs @("rev-parse", "--short", "HEAD"))).Trim()

$scanMode = ""
$rangeArg = ""
$previousHead = ""
$scanSinceExpr = ""

if (-not [string]::IsNullOrWhiteSpace($BaselineCommit)) {
    if (-not (Test-GitCommitExists -WorkTree $GethRoot -Commit $BaselineCommit)) {
        throw ("baseline commit does not exist in geth repo: {0}" -f $BaselineCommit)
    }
    $scanMode = "baseline_range"
    $rangeArg = "{0}..HEAD" -f $BaselineCommit
    $previousHead = $BaselineCommit
} elseif (-not [string]::IsNullOrWhiteSpace($Since)) {
    $scanMode = "since_expr"
    $scanSinceExpr = $Since
} elseif ($null -ne $state -and $null -ne $state.last_scanned_head -and (Test-GitCommitExists -WorkTree $GethRoot -Commit ([string]$state.last_scanned_head))) {
    $scanMode = "state_range"
    $rangeArg = "{0}..HEAD" -f ([string]$state.last_scanned_head)
    $previousHead = [string]$state.last_scanned_head
} else {
    $scanMode = "since_hours"
    $scanSinceExpr = "{0} hours ago" -f [UInt64]$SinceHours
}

$logArgs = @(
    "log",
    "--date=iso-strict",
    "--pretty=format:%H`t%h`t%ad`t%s",
    "-n", ([string][UInt64]$MaxCommits)
)
if (-not [string]::IsNullOrWhiteSpace($rangeArg)) {
    $logArgs += $rangeArg
} else {
    $logArgs += ("--since={0}" -f $scanSinceExpr)
}

$logLinesRaw = Invoke-Git -WorkTree $GethRoot -GitArgs $logArgs
$logLines = @()
foreach ($line in $logLinesRaw) {
    $s = [string]$line
    if (-not [string]::IsNullOrWhiteSpace($s)) {
        $logLines += $s
    }
}

$riskCounts = [ordered]@{
    critical = 0
    high = 0
    medium = 0
    low = 0
    none = 0
}
$areaCounts = @{}
$allAreas = New-Object System.Collections.Generic.HashSet[string]
$globalChecks = New-Object System.Collections.Generic.HashSet[string]
$commits = New-Object System.Collections.ArrayList
$highestRisk = "none"
$highestRank = 0

foreach ($line in $logLines) {
    $parts = $line -split "`t", 4
    if ($parts.Count -lt 4) {
        continue
    }
    $fullHash = [string]$parts[0]
    $shortHash = [string]$parts[1]
    $dateIso = [string]$parts[2]
    $subject = [string]$parts[3]

    $filesRaw = Invoke-Git -WorkTree $GethRoot -GitArgs @("show", "--name-only", "--pretty=format:", $fullHash)
    $changedFiles = @()
    foreach ($f in $filesRaw) {
        $p = ([string]$f).Trim()
        if (-not [string]::IsNullOrWhiteSpace($p)) {
            $changedFiles += $p
        }
    }

    $commitAreasSet = New-Object System.Collections.Generic.HashSet[string]
    $watchPathsSet = New-Object System.Collections.Generic.HashSet[string]
    $reasonsSet = New-Object System.Collections.Generic.HashSet[string]
    $commitChecksSet = New-Object System.Collections.Generic.HashSet[string]
    $commitRisk = "none"
    $commitRank = 0

    foreach ($path in $changedFiles) {
        $impact = Get-PathImpact -Path $path
        $area = [string]$impact.area
        $risk = [string]$impact.risk
        $reason = [string]$impact.reason
        $rank = Get-RiskRank -Risk $risk

        [void]$commitAreasSet.Add($area)
        [void]$reasonsSet.Add($reason)
        [void]$allAreas.Add($area)

        if ($areaCounts.ContainsKey($area)) {
            $areaCounts[$area] = [int]$areaCounts[$area] + 1
        } else {
            $areaCounts[$area] = 1
        }

        foreach ($wp in (Get-AreaWatchPaths -Area $area)) {
            [void]$watchPathsSet.Add($wp)
        }
        foreach ($cmd in (Get-AreaChecks -Area $area)) {
            [void]$commitChecksSet.Add($cmd)
            [void]$globalChecks.Add($cmd)
        }

        if ($rank -gt $commitRank) {
            $commitRank = $rank
            $commitRisk = $risk
        }
    }

    if (-not $riskCounts.Contains($commitRisk)) {
        $riskCounts[$commitRisk] = 0
    }
    $riskCounts[$commitRisk] = [int]$riskCounts[$commitRisk] + 1

    if ($commitRank -gt $highestRank) {
        $highestRank = $commitRank
        $highestRisk = $commitRisk
    }

    $commitEntry = [ordered]@{
        hash = $fullHash
        short = $shortHash
        date = $dateIso
        subject = $subject
        changed_files_count = $changedFiles.Count
        changed_files = $changedFiles
        areas = @($commitAreasSet | Sort-Object)
        impact_reasons = @($reasonsSet | Sort-Object)
        watch_supervm_paths = @($watchPathsSet | Sort-Object)
        recommended_checks = @($commitChecksSet | Sort-Object)
        risk_level = $commitRisk
        sync_priority = (Get-RiskPriority -Risk $commitRisk)
        direct_plugin_impact = ($commitRank -ge (Get-RiskRank -Risk "high"))
    }
    [void]$commits.Add($commitEntry)
}

$directImpactCount = @($commits | Where-Object { $_.direct_plugin_impact }).Count
$actionRequired = ($highestRank -ge (Get-RiskRank -Risk "high"))

$summary = [ordered]@{
    generated_at_utc = [DateTimeOffset]::UtcNow.ToString("o")
    repo_root = $RepoRoot
    geth_root = $GethRoot
    scan = [ordered]@{
        mode = $scanMode
        range = $rangeArg
        since = $scanSinceExpr
        max_commits = [UInt64]$MaxCommits
        previous_head = $previousHead
        current_head = $head
        current_head_short = $headShort
    }
    totals = [ordered]@{
        commits_scanned = $commits.Count
        direct_plugin_impact_commits = $directImpactCount
        highest_risk = $highestRisk
        action_required = $actionRequired
    }
    risk_counts = $riskCounts
    area_counts = $areaCounts
    recommended_checks = @($globalChecks | Sort-Object)
    commits = $commits
}

$summary | ConvertTo-Json -Depth 64 | Set-Content -Path $SummaryOut -Encoding UTF8

$md = New-Object System.Text.StringBuilder
[void]$md.AppendLine("# Geth Upstream Impact Scan")
[void]$md.AppendLine("")
[void]$md.AppendLine(("- generated_at_utc: {0}" -f $summary.generated_at_utc))
[void]$md.AppendLine(("- geth_root: {0}" -f $GethRoot))
[void]$md.AppendLine(("- scan_mode: {0}" -f $scanMode))
if (-not [string]::IsNullOrWhiteSpace($rangeArg)) {
    [void]$md.AppendLine(("- range: {0}" -f $rangeArg))
} else {
    [void]$md.AppendLine(("- since: {0}" -f $scanSinceExpr))
}
[void]$md.AppendLine(("- current_head: {0}" -f $head))
[void]$md.AppendLine("")
[void]$md.AppendLine("## Summary")
[void]$md.AppendLine("")
[void]$md.AppendLine("| key | value |")
[void]$md.AppendLine("|---|---:|")
[void]$md.AppendLine(("| commits_scanned | {0} |" -f $commits.Count))
[void]$md.AppendLine(("| direct_plugin_impact_commits | {0} |" -f $directImpactCount))
[void]$md.AppendLine(("| highest_risk | {0} |" -f $highestRisk))
[void]$md.AppendLine(("| action_required | {0} |" -f $actionRequired))
[void]$md.AppendLine("")

[void]$md.AppendLine("## Risk Counts")
[void]$md.AppendLine("")
[void]$md.AppendLine("| risk | count |")
[void]$md.AppendLine("|---|---:|")
foreach ($risk in @("critical", "high", "medium", "low", "none")) {
    [void]$md.AppendLine(("| {0} | {1} |" -f $risk, [int]$riskCounts[$risk]))
}
[void]$md.AppendLine("")

[void]$md.AppendLine("## Area Counts")
[void]$md.AppendLine("")
[void]$md.AppendLine("| area | count |")
[void]$md.AppendLine("|---|---:|")
foreach ($kv in ($areaCounts.GetEnumerator() | Sort-Object -Property Name)) {
    [void]$md.AppendLine(("| {0} | {1} |" -f $kv.Key, [int]$kv.Value))
}
[void]$md.AppendLine("")

[void]$md.AppendLine("## Recommended Checks")
[void]$md.AppendLine("")
if ($globalChecks.Count -eq 0) {
    [void]$md.AppendLine("- no action needed for current scan window.")
} else {
    foreach ($cmd in ($globalChecks | Sort-Object)) {
        [void]$md.AppendLine(("- {0}" -f $cmd))
    }
}
[void]$md.AppendLine("")

[void]$md.AppendLine("## Commit Details")
[void]$md.AppendLine("")
if ($commits.Count -eq 0) {
    [void]$md.AppendLine("- no upstream commits in selected scan window.")
} else {
    foreach ($c in $commits) {
        [void]$md.AppendLine(("### [{0}] {1} {2}" -f $c.risk_level.ToUpperInvariant(), $c.short, $c.subject))
        [void]$md.AppendLine(("- date: {0}" -f $c.date))
        [void]$md.AppendLine(("- sync_priority: {0}" -f $c.sync_priority))
        [void]$md.AppendLine(("- direct_plugin_impact: {0}" -f $c.direct_plugin_impact))
        [void]$md.AppendLine(("- areas: {0}" -f (($c.areas -join ", "))))
        [void]$md.AppendLine(("- watch_supervm_paths: {0}" -f (($c.watch_supervm_paths -join ", "))))
        [void]$md.AppendLine("- changed_files:")
        foreach ($f in $c.changed_files) {
            [void]$md.AppendLine(("  - {0}" -f $f))
        }
        if ($c.recommended_checks.Count -gt 0) {
            [void]$md.AppendLine("- recommended_checks:")
            foreach ($cmd in $c.recommended_checks) {
                [void]$md.AppendLine(("  - {0}" -f $cmd))
            }
        }
        [void]$md.AppendLine("")
    }
}

[void]$md.AppendLine("## Daily Usage")
[void]$md.AppendLine("")
[void]$md.AppendLine("powershell:")
[void]$md.AppendLine("powershell -ExecutionPolicy Bypass -File scripts/migration/run_evm_geth_upstream_impact_scan.ps1")
[void]$md.AppendLine("")
[void]$md.AppendLine("This command uses state-based incremental scan by default (from last scanned head to current HEAD).")

$md.ToString() | Set-Content -Path $MarkdownOut -Encoding UTF8

if (-not $NoUpdateState) {
    $newState = [ordered]@{
        updated_at_utc = [DateTimeOffset]::UtcNow.ToString("o")
        geth_root = $GethRoot
        last_scanned_head = $head
        last_scanned_head_short = $headShort
        last_scan_mode = $scanMode
        last_scan_range = $rangeArg
        last_scan_since = $scanSinceExpr
        last_commits_scanned = $commits.Count
        last_highest_risk = $highestRisk
    }
    $newState | ConvertTo-Json -Depth 16 | Set-Content -Path $StatePath -Encoding UTF8
}

Write-Host ("[geth-impact-scan] summary: {0}" -f $SummaryOut)
Write-Host ("[geth-impact-scan] markdown: {0}" -f $MarkdownOut)
Write-Host ("[geth-impact-scan] commits={0} highest_risk={1} action_required={2}" -f $commits.Count, $highestRisk, $actionRequired)
