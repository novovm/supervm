#!/usr/bin/env pwsh
# SuperVM Work Logger - 状态查询

$repoRoot = Split-Path (Split-Path (Split-Path $PSScriptRoot -Parent) -Parent) -Parent
$toolRoot = Join-Path $repoRoot "tools\work-logger"
Set-Location $repoRoot

Write-Host "📊 工作日志状态" -ForegroundColor Cyan
Write-Host "="*50 -ForegroundColor Cyan

# 检查监听器进程
$pidFile = Join-Path $toolRoot "data\watcher.pid"
if (Test-Path $pidFile) {
    $watcherPid = Get-Content $pidFile
    $process = Get-Process -Id $watcherPid -ErrorAction SilentlyContinue
    
    if ($process) {
        Write-Host "`n✅ 监听器运行中" -ForegroundColor Green
        Write-Host "   PID: $watcherPid" -ForegroundColor Gray
        Write-Host "   运行时长: $([math]::Round(($process.CPU), 2))s CPU" -ForegroundColor Gray
        Write-Host "   内存: $([math]::Round($process.WorkingSet64 / 1MB, 2)) MB" -ForegroundColor Gray
    } else {
        Write-Host "`n⚠️  监听器未运行（残留 PID 文件）" -ForegroundColor Yellow
        Remove-Item $pidFile -ErrorAction SilentlyContinue
    }
} else {
    Write-Host "`n⏹️  监听器未运行" -ForegroundColor Yellow
}

# 检查当前会话
$currentSession = Join-Path $toolRoot "data\current_session.json"
if (Test-Path $currentSession) {
    Write-Host "`n📝 当前会话:" -ForegroundColor Cyan
    $session = Get-Content $currentSession | ConvertFrom-Json
    $startTime = [DateTime]::Parse($session.start_time)
    $duration = (Get-Date) - $startTime
    
    Write-Host "   Session ID: $($session.session_id)" -ForegroundColor White
    Write-Host "   开始时间: $($startTime.ToString('yyyy-MM-dd HH:mm:ss'))" -ForegroundColor White
    Write-Host "   持续时长: $([math]::Floor($duration.TotalMinutes))m $($duration.Seconds)s" -ForegroundColor White
    Write-Host "   文件变更: $($session.file_changes.PSObject.Properties.Count) 个" -ForegroundColor White
    
    if ($session.file_changes.PSObject.Properties.Count -gt 0) {
        Write-Host "`n   最近变更:" -ForegroundColor Yellow
        $session.file_changes.PSObject.Properties | Select-Object -First 5 | ForEach-Object {
            $file = $_.Name
            $change = $_.Value
            Write-Host "   - $file (+$($change.lines_added) -$($change.lines_removed))" -ForegroundColor Gray
        }
    }
} else {
    Write-Host "`n📭 无活动会话" -ForegroundColor Gray
}

# 历史会话统计
$historySessions = Get-ChildItem (Join-Path $toolRoot "data\session_*.json") -ErrorAction SilentlyContinue
if ($historySessions) {
    Write-Host "`n📚 历史会话: $($historySessions.Count) 个" -ForegroundColor Cyan
    $latestSession = $historySessions | Sort-Object LastWriteTime -Descending | Select-Object -First 1
    Write-Host "   最近: $($latestSession.Name)" -ForegroundColor Gray
}

Write-Host "`n" -NoNewline
