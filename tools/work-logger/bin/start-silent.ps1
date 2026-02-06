#!/usr/bin/env pwsh
# SuperVM Work Logger - 智能启动（检测重复）

$repoRoot = "D:\WorksArea\SUPERVM"
$toolRoot = Join-Path $repoRoot "tools\work-logger"
$pidFile = Join-Path $toolRoot "data\watcher.pid"

# 检查是否已经在运行
if (Test-Path $pidFile) {
    $oldPid = Get-Content $pidFile
    $process = Get-Process -Id $oldPid -ErrorAction SilentlyContinue
    if ($process) {
        # 已在运行，静默退出
        exit 0
    }
}

# 配置 PATH
$pythonPath = "C:\Users\leadb\AppData\Local\Programs\Python\Python311"
$pythonScripts = "$pythonPath\Scripts"
$gitPath = "C:\Program Files\Git\bin"
$env:Path = "$pythonPath;$pythonScripts;$gitPath;$env:Path"

# 静默启动（无输出）
Set-Location $repoRoot
$watcherScript = Join-Path $toolRoot "lib\watcher.py"
$process = Start-Process -FilePath "python" `
    -ArgumentList $watcherScript, $repoRoot `
    -WindowStyle Hidden `
    -PassThru

# 保存 PID
$dataDir = Join-Path $toolRoot "data"
New-Item -ItemType Directory -Force -Path $dataDir | Out-Null
$process.Id | Out-File -FilePath $pidFile -Encoding utf8
