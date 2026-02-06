#!/usr/bin/env pwsh
# SuperVM Work Logger - 启动脚本（后台运行）

$repoRoot = Split-Path (Split-Path (Split-Path $PSScriptRoot -Parent) -Parent) -Parent
$toolRoot = Join-Path $repoRoot "tools\work-logger"
Set-Location $repoRoot

Write-Host "🚀 SuperVM Work Logger - 后台服务" -ForegroundColor Cyan
Write-Host "="*50 -ForegroundColor Cyan

# 配置 Python 和 Git 路径
$pythonPath = "C:\Users\leadb\AppData\Local\Programs\Python\Python311"
$pythonScripts = "$pythonPath\Scripts"
$gitPath = "C:\Program Files\Git\bin"

# 检查是否已在运行
$pidFile = ".work-logger\watcher.pid"
if (Test-Path $pidFile) {
    $oldPid = Get-Content $pidFile
    $process = Get-Process -Id $oldPid -ErrorAction SilentlyContinue
    if ($process) {
        Write-Host "⚠️  监听器已在运行 (PID: $oldPid)" -ForegroundColor Yellow
        Write-Host "   使用 .\停止工作日志.ps1 来停止" -ForegroundColor Yellow
        exit 0
    }
}

# 验证环境
Write-Host "`n检查环境..." -ForegroundColor Yellow
$env:Path = "$pythonPath;$pythonScripts;$gitPath;$env:Path"

try {
    $pythonVersion = & python --version 2>&1
    Write-Host "✅ $pythonVersion" -ForegroundColor Green
} catch {
    Write-Host "❌ Python 未找到" -ForegroundColor Red
    exit 1
}

# 启动后台监听器
Write-Host "`n📂 开始监听工作区: $repoRoot" -ForegroundColor Cyan
$watcherScript = Join-Path $toolRoot "lib\watcher.py"

# 使用 Start-Process 后台运行
$process = Start-Process -FilePath "python" `
    -ArgumentList $watcherScript, $repoRoot `
    -WindowStyle Hidden `
    -PassThru

# 保存 PID
$dataDir = Join-Path $toolRoot "data"
New-Item -ItemType Directory -Force -Path $dataDir | Out-Null
$pidFile = Join-Path $dataDir "watcher.pid"
$process.Id | Out-File -FilePath $pidFile -Encoding utf8

Write-Host "✅ 监听器已启动（后台运行）" -ForegroundColor Green
Write-Host "   PID: $($process.Id)" -ForegroundColor Gray
Write-Host "`n命令:" -ForegroundColor Cyan
Write-Host "   查看状态: .\tools\work-logger\bin\status.ps1" -ForegroundColor White
Write-Host "   停止记录: .\tools\work-logger\bin\stop.ps1" -ForegroundColor White
Write-Host ""
