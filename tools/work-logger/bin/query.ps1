# PowerShell Query Command Wrapper
# Usage: .\query.ps1 --recent 7

param(
    [Parameter(Mandatory=$false)]
    [ValidateSet('recent', 'module', 'search', 'stats', 'export', 'daily')]
    [string]$Command = 'recent',
    
    [Parameter(Mandatory=$false)]
    [int]$Days = 7,
    
    [Parameter(Mandatory=$false)]
    [string]$Module,
    
    [Parameter(Mandatory=$false)]
    [string]$SessionId,
    
    [Parameter(Mandatory=$false)]
    [string]$Keyword
)

# 配置
$toolRoot = Split-Path -Parent $PSScriptRoot
$pythonScript = Join-Path $toolRoot 'lib\query.py'
$pythonExe = Join-Path $env:USERPROFILE '.cargo\bin\python.exe'

# 验证 Python 可用
if (-not (Test-Path $pythonExe)) {
    Write-Host "❌ Python not found at: $pythonExe" -ForegroundColor Red
    Write-Host "Trying system Python..." -ForegroundColor Yellow
    $pythonExe = 'python'
}

# 验证查询脚本
if (-not (Test-Path $pythonScript)) {
    Write-Host "❌ Query script not found: $pythonScript" -ForegroundColor Red
    exit 1
}

# 构建命令行
$queryArgs = @()

if ($days) {
    $queryArgs += "--recent"
    $queryArgs += $days
}

if ($Module) {
    $queryArgs += "--module"
    $queryArgs += $Module
}

if ($Keyword) {
    $queryArgs += "--search"
    $queryArgs += $Keyword
}

if ($SessionId) {
    $queryArgs += "--export"
    $queryArgs += $SessionId
}

if ($Command -eq 'stats') {
    $queryArgs += "--stats"
}

if ($Command -eq 'daily') {
    $queryArgs += "--daily"
    $queryArgs += $Days
}

# 执行查询
Write-Host "🔍 Querying work sessions..." -ForegroundColor Cyan
& $pythonExe $pythonScript @queryArgs

if ($LASTEXITCODE -ne 0) {
    Write-Host "❌ Query failed" -ForegroundColor Red
    exit 1
}
