#!/usr/bin/env pwsh
# SuperVM Work Logger - 智能结束（支持自动保存）

param(
    [switch]$Auto  # 自动模式（VS Code 关闭时，仅保存状态）
)

$repoRoot = Split-Path (Split-Path (Split-Path $PSScriptRoot -Parent) -Parent) -Parent
$toolRoot = Join-Path $repoRoot "tools\work-logger"
Set-Location $repoRoot

$pidFile = Join-Path $toolRoot "data\watcher.pid"
if (-not (Test-Path $pidFile)) {
    if (-not $Auto) {
        Write-Host "⚠️  未找到运行中的监听器" -ForegroundColor Yellow
    }
    exit 0
}

$watcherPid = Get-Content $pidFile
$process = Get-Process -Id $watcherPid -ErrorAction SilentlyContinue
if (-not $process) {
    Remove-Item $pidFile -ErrorAction SilentlyContinue
    if (-not $Auto) {
        Write-Host "⚠️  进程已停止" -ForegroundColor Yellow
    }
    exit 0
}

# 如果是自动模式（VS Code 关闭），仅保存会话状态
if ($Auto) {
    Write-Host "💾 会话已保存，下次打开将继续记录" -ForegroundColor Green
    exit 0
}

# 手动模式：交互式输入工作内容
Write-Host "`n📝 请输入今天的工作内容（生成工作笔记）" -ForegroundColor Cyan
Write-Host "="*50 -ForegroundColor Gray

Write-Host "`n💡 Tip: 必填第1项，其他可选（回车跳过）" -ForegroundColor Gray

Write-Host "`n1️⃣  今日主要做了什么？ *必填" -ForegroundColor Yellow
$workSummary = Read-Host "   简述"

if ([string]::IsNullOrWhiteSpace($workSummary)) {
    Write-Host "⚠️  至少需要填写工作内容" -ForegroundColor Red
    exit 1
}

Write-Host "`n2️⃣  遇到了什么问题/挑战？（可选）" -ForegroundColor Yellow
$problems = Read-Host "   问题"

Write-Host "`n3️⃣  如何解决的？（可选）" -ForegroundColor Yellow
$solutions = Read-Host "   解决方案"

Write-Host "`n4️⃣  与 Copilot 的关键对话？（可选，多条用分号分隔）" -ForegroundColor Yellow
Write-Host "   示例: '讨论多根工作区问题; 建议自动启动方案'" -ForegroundColor Gray
$chatSummary = Read-Host "   聊天摘要"

Write-Host "`n5️⃣  下一步计划/待办？（可选）" -ForegroundColor Yellow
$nextSteps = Read-Host "   计划"

# 保存工作内容
$workNoteData = @{
    summary = $workSummary
    problems = $problems
    solutions = $solutions
    chat = $chatSummary
    next_steps = $nextSteps
} | ConvertTo-Json
$workNoteData | Out-File -FilePath (Join-Path $toolRoot "data\work_note_input.json") -Encoding utf8

# 停止进程
Write-Host "`n📝 正在生成工作笔记..." -ForegroundColor Cyan
Stop-Process -Id $watcherPid -Force
Remove-Item $pidFile -ErrorAction SilentlyContinue

Start-Sleep -Seconds 2

# 检查生成的报告
$outputDir = Join-Path $toolRoot "output"
$latestReport = Get-ChildItem (Join-Path $outputDir "WORK-NOTE-*.md") -ErrorAction SilentlyContinue | 
    Sort-Object LastWriteTime -Descending | 
    Select-Object -First 1

if ($latestReport) {
    Write-Host "✅ 工作笔记已生成" -ForegroundColor Green
    Write-Host "📝 文件: $($latestReport.Name)" -ForegroundColor Cyan
    Write-Host "`n前 30 行预览:" -ForegroundColor Yellow
    Get-Content $latestReport.FullName | Select-Object -First 30
    Write-Host "`n..." -ForegroundColor Gray
    Write-Host "`n💡 完整内容: tools\work-logger\output\$($latestReport.Name)" -ForegroundColor Gray
} else {
    Write-Host "✅ 监听器已停止" -ForegroundColor Green
}
