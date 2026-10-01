param(
    [string]$RepoRoot = "",
    [string]$OutputDir = "",
    [string]$ChecklistPath = "",
    [string]$PromotionOutputDir = "",
    [string]$PromotionSummaryPath = "",
    [string]$DraftReportPath = "",
    [string]$FinalReportPath = "",
    [string]$ReadinessSummaryPath = "",
    [switch]$NoRefreshReadinessGate,
    [switch]$NoThrow
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not $RepoRoot) {
    $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
} else {
    $RepoRoot = (Resolve-Path $RepoRoot).Path
}

if (-not $OutputDir) {
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\week1-2026-03-13\week4-closeout"
} elseif (-not [System.IO.Path]::IsPathRooted($OutputDir)) {
    $OutputDir = Join-Path $RepoRoot $OutputDir
}
$OutputDir = [System.IO.Path]::GetFullPath($OutputDir)
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

if (-not $ChecklistPath) {
    $ChecklistPath = Join-Path $RepoRoot "docs_CN\SVM2026-MIGRATION\NOVOVM-OPEN-BUSINESS-SURFACE-CLOSURE-CHECKLIST-2026-03-13.md"
} elseif (-not [System.IO.Path]::IsPathRooted($ChecklistPath)) {
    $ChecklistPath = Join-Path $RepoRoot $ChecklistPath
}
$ChecklistPath = [System.IO.Path]::GetFullPath($ChecklistPath)

if ($PromotionOutputDir) {
    if (-not [System.IO.Path]::IsPathRooted($PromotionOutputDir)) {
        $PromotionOutputDir = Join-Path $RepoRoot $PromotionOutputDir
    }
    $PromotionOutputDir = [System.IO.Path]::GetFullPath($PromotionOutputDir)
    New-Item -ItemType Directory -Force -Path $PromotionOutputDir | Out-Null
}

if ($DraftReportPath) {
    if (-not [System.IO.Path]::IsPathRooted($DraftReportPath)) {
        $DraftReportPath = Join-Path $RepoRoot $DraftReportPath
    }
    $DraftReportPath = [System.IO.Path]::GetFullPath($DraftReportPath)
}

if ($FinalReportPath) {
    if (-not [System.IO.Path]::IsPathRooted($FinalReportPath)) {
        $FinalReportPath = Join-Path $RepoRoot $FinalReportPath
    }
    $FinalReportPath = [System.IO.Path]::GetFullPath($FinalReportPath)
}

if ($ReadinessSummaryPath) {
    if (-not [System.IO.Path]::IsPathRooted($ReadinessSummaryPath)) {
        $ReadinessSummaryPath = Join-Path $RepoRoot $ReadinessSummaryPath
    }
    $ReadinessSummaryPath = [System.IO.Path]::GetFullPath($ReadinessSummaryPath)
}

if (-not $PromotionSummaryPath) {
    if ($PromotionOutputDir) {
        $PromotionSummaryPath = Join-Path $PromotionOutputDir "ga-closure-promotion-summary.json"
    } else {
        $PromotionSummaryPath = Join-Path $RepoRoot "artifacts\migration\week1-2026-03-13\ga-closure-promotion\ga-closure-promotion-summary.json"
    }
} elseif (-not [System.IO.Path]::IsPathRooted($PromotionSummaryPath)) {
    $PromotionSummaryPath = Join-Path $RepoRoot $PromotionSummaryPath
}
$PromotionSummaryPath = [System.IO.Path]::GetFullPath($PromotionSummaryPath)

$promotionScript = Join-Path $RepoRoot "scripts\migration\promote_ga_closure_report.ps1"
if (-not (Test-Path $promotionScript)) {
    throw "missing ga closure promotion script: $promotionScript"
}

$promotionArgs = @{
    RepoRoot = $RepoRoot
    NoThrow = $true
}
if ($NoRefreshReadinessGate) { $promotionArgs["NoRefreshReadinessGate"] = $true }
if ($PromotionOutputDir) { $promotionArgs["OutputDir"] = $PromotionOutputDir }
if ($DraftReportPath) { $promotionArgs["DraftReportPath"] = $DraftReportPath }
if ($FinalReportPath) { $promotionArgs["FinalReportPath"] = $FinalReportPath }
if ($ReadinessSummaryPath) { $promotionArgs["ReadinessSummaryPath"] = $ReadinessSummaryPath }
& $promotionScript @promotionArgs | Out-Null

if (-not (Test-Path $PromotionSummaryPath)) {
    throw "missing ga closure promotion summary: $PromotionSummaryPath"
}

$promotion = Get-Content -Path $PromotionSummaryPath -Raw | ConvertFrom-Json
$now = Get-Date
$stamp = $now.ToString("yyyy-MM-dd HH:mm zzz")
$stampShort = $now.ToString("yyyy-MM-dd HH:mm")
$decision = [string]$promotion.decision
$promoted = [bool]$promotion.promoted
$finalReportPath = [string]$promotion.final_report_path
$finalReportExists = $false
if ($finalReportPath) {
    $finalReportExists = Test-Path $finalReportPath
}

$checklistUpdated = $false
$checklistBackupPath = ""
$checklistBackupSha256 = ""
$checklistSha256Before = ""
$checklistSha256After = ""
$changedItemCount = 0
$changedItems = @()
$closeoutLineTag = "run_week4_closeout.ps1 自动回勾"
$week4ItemsAllChecked = $false

if ($promoted) {
    if (-not (Test-Path $ChecklistPath)) {
        throw "missing checklist: $ChecklistPath"
    }

    $checklistSha256Before = (Get-FileHash -Path $ChecklistPath -Algorithm SHA256).Hash.ToLowerInvariant()
    $backupName = "checklist-backup-" + (Get-Date).ToString("yyyyMMdd-HHmmss") + ".md"
    $checklistBackupPath = Join-Path $OutputDir $backupName
    Copy-Item -Path $ChecklistPath -Destination $checklistBackupPath -Force
    $checklistBackupSha256 = (Get-FileHash -Path $checklistBackupPath -Algorithm SHA256).Hash.ToLowerInvariant()

    $lines = Get-Content -Path $ChecklistPath
    $newLines = New-Object System.Collections.Generic.List[string]

    foreach ($line in $lines) {
        $updatedLine = $line
        $isTarget = $false
        $itemName = ""

        if ($line -match "^- \[[ x]\] 完成 GA 候选 RC \+ 稳定性窗口") {
            $isTarget = $true
            $itemName = "ga_rc_stability_window"
        } elseif ($line -match "^- \[[ x]\] 发布最终收口文档") {
            $isTarget = $true
            $itemName = "ga_final_closure_report"
        } elseif ($line -match "^- \[[ x]\] 完成至少 1 轮第三方漏洞审计") {
            $isTarget = $true
            $itemName = "third_party_audit_c_h_zero"
        }

        if ($isTarget) {
            if ($updatedLine.StartsWith("- [ ] ")) {
                $updatedLine = "- [x] " + $updatedLine.Substring(6)
                $changedItemCount += 1
                $changedItems += $itemName
            }
            if ($updatedLine -notmatch [regex]::Escape($closeoutLineTag)) {
                $updatedLine += " `$stamp` 已由 $closeoutLineTag（readiness=GO, promoted=true）。"
                $changedItemCount += 1
                $changedItems += ($itemName + "_note")
            }
        }

        $newLines.Add($updatedLine) | Out-Null
    }

    $tableMarker = "| 日期 | 模块 | 今日完成 | 未完成项 | 阻断 | 明日计划 | 证据路径 |"
    $closeoutRowToken = "| Week4 自动关单 |"
    $hasCloseoutRow = @($newLines | Where-Object { $_ -like "*$closeoutRowToken*" }).Count -gt 0
    if (-not $hasCloseoutRow) {
        $closeoutRow = "| $stampShort | Week4 自动关单 | 1) 执行 `run_week4_closeout.ps1` 并确认 `readiness=GO`；2) 自动晋级 GA 正式收口报告；3) 自动回勾 Week4 三项阻断勾选 | 无 | 无 | 进入发布后巡检与连续监控 | `scripts/migration/run_week4_closeout.ps1`; `artifacts/migration/week1-2026-03-13/week4-closeout/week4-closeout-summary.json`; `artifacts/migration/week1-2026-03-13/ga-closure-promotion/ga-closure-promotion-summary.json`; `docs_CN/SVM2026-MIGRATION/NOVOVM-GA-CLOSURE-REPORT-2026-03-13.md` |"
        $inserted = $false
        for ($i = 0; $i -lt $newLines.Count; $i++) {
            if ($newLines[$i] -eq $tableMarker) {
                for ($j = $i + 1; $j -lt $newLines.Count; $j++) {
                    if ($newLines[$j] -match "^\|---\|---\|---\|---\|---\|---\|---\|$") {
                        $newLines.Insert($j + 1, $closeoutRow)
                        $inserted = $true
                        break
                    }
                }
                break
            }
        }
        if (-not $inserted) {
            $newLines.Add($closeoutRow) | Out-Null
        }
        $changedItemCount += 1
        $changedItems += "daily_update_row"
    }

    if ($changedItemCount -gt 0) {
        $newLines | Set-Content -Path $ChecklistPath -Encoding UTF8
        $checklistUpdated = $true
    }
    $checklistSha256After = (Get-FileHash -Path $ChecklistPath -Algorithm SHA256).Hash.ToLowerInvariant()

    $checkListContent = Get-Content -Path $ChecklistPath -Raw
    $week4ItemsAllChecked = (
        $checkListContent -match "(?m)^- \[x\] 完成 GA 候选 RC \+ 稳定性窗口" -and
        $checkListContent -match "(?m)^- \[x\] 发布最终收口文档" -and
        $checkListContent -match "(?m)^- \[x\] 完成至少 1 轮第三方漏洞审计"
    )
}

$closedOut = ($promoted -and $finalReportExists -and ($week4ItemsAllChecked -or $checklistUpdated))
$summary = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    generated_at_local = $now.ToString("yyyy-MM-dd HH:mm:ss zzz")
    profile = "week4_closeout_v1"
    decision = $decision
    promoted = $promoted
    closed_out = $closedOut
    checklist_updated = $checklistUpdated
    changed_item_count = $changedItemCount
    changed_items = $changedItems
    week4_items_all_checked = $week4ItemsAllChecked
    checklist_backup_path = $checklistBackupPath
    checklist_backup_sha256 = $checklistBackupSha256
    checklist_sha256_before = $checklistSha256Before
    checklist_sha256_after = $checklistSha256After
    final_report_path = $finalReportPath
    final_report_exists = $finalReportExists
    checklist_path = $ChecklistPath
    promotion_output_dir = $PromotionOutputDir
    promotion_draft_report_path = $DraftReportPath
    promotion_final_report_path = $FinalReportPath
    promotion_readiness_summary_path = $ReadinessSummaryPath
    promotion_summary_json = $PromotionSummaryPath
}

$summaryJson = Join-Path $OutputDir "week4-closeout-summary.json"
$summaryMd = Join-Path $OutputDir "week4-closeout-summary.md"
$summary | ConvertTo-Json -Depth 8 | Set-Content -Path $summaryJson -Encoding UTF8

$md = @(
    "# Week4 Closeout Summary"
    ""
    "- generated_at_local: $($summary.generated_at_local)"
    "- profile: $($summary.profile)"
    "- decision: $($summary.decision)"
    "- promoted: $($summary.promoted)"
    "- closed_out: $($summary.closed_out)"
    "- checklist_updated: $($summary.checklist_updated)"
    "- changed_item_count: $($summary.changed_item_count)"
    "- changed_items: $([string]::Join(', ', @($summary.changed_items)))"
    "- week4_items_all_checked: $($summary.week4_items_all_checked)"
    "- checklist_backup_path: $($summary.checklist_backup_path)"
    "- checklist_backup_sha256: $($summary.checklist_backup_sha256)"
    "- checklist_sha256_before: $($summary.checklist_sha256_before)"
    "- checklist_sha256_after: $($summary.checklist_sha256_after)"
    "- final_report_exists: $($summary.final_report_exists)"
    "- final_report_path: $($summary.final_report_path)"
    "- checklist_path: $($summary.checklist_path)"
    "- promotion_output_dir: $($summary.promotion_output_dir)"
    "- promotion_draft_report_path: $($summary.promotion_draft_report_path)"
    "- promotion_final_report_path: $($summary.promotion_final_report_path)"
    "- promotion_readiness_summary_path: $($summary.promotion_readiness_summary_path)"
    "- promotion_summary_json: $($summary.promotion_summary_json)"
)
$md -join "`n" | Set-Content -Path $summaryMd -Encoding UTF8

Write-Host "week4 closeout summary:"
Write-Host "  decision: $($summary.decision)"
Write-Host "  promoted: $($summary.promoted)"
Write-Host "  closed_out: $($summary.closed_out)"
Write-Host "  checklist_updated: $($summary.checklist_updated)"
Write-Host "  changed_item_count: $($summary.changed_item_count)"
Write-Host "  final_report_exists: $($summary.final_report_exists)"
Write-Host "  summary_json: $summaryJson"
Write-Host "  summary_md: $summaryMd"

if (-not $summary.closed_out -and -not $NoThrow) {
    throw "week4 closeout not completed (decision=$decision, promoted=$promoted, final_report_exists=$finalReportExists)"
}

if ($summary.closed_out) {
    Write-Host "week4 closeout completed"
} else {
    Write-Host "week4 closeout pending (NoThrow=$NoThrow)"
}
