# Setup permanent PATH for Python and Cargo
# Run as Administrator!

param(
    [switch]$SkipPython = $false,
    [switch]$SkipCargo = $false
)

$ErrorActionPreference = "Stop"

Write-Host "🔧 Permanent PATH Setup Script" -ForegroundColor Cyan
Write-Host "================================" -ForegroundColor Cyan

# Check if running as admin
$isAdmin = ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole] "Administrator")
if (-not $isAdmin) {
    Write-Host "❌ This script MUST run as Administrator!" -ForegroundColor Red
    Write-Host "   Right-click PowerShell → 'Run as administrator'" -ForegroundColor Yellow
    exit 1
}

# Get current PATH
$currentPath = [Environment]::GetEnvironmentVariable("PATH", "Machine")
$modified = $false

# Add Python
if (-not $SkipPython) {
    $pythonPaths = @(
        "C:\Users\leadb\AppData\Local\Programs\Python\Python311",
        "C:\Users\leadb\AppData\Local\Programs\Python\Python312",
        "C:\Users\leadb\AppData\Local\Programs\Python\Python313"
    )
    
    foreach ($pythonPath in $pythonPaths) {
        if (Test-Path "$pythonPath\python.exe") {
            if ($currentPath -notlike "*$pythonPath*") {
                [Environment]::SetEnvironmentVariable("PATH", "$currentPath;$pythonPath", "Machine")
                $currentPath = [Environment]::GetEnvironmentVariable("PATH", "Machine")
                Write-Host "✅ Added Python: $pythonPath" -ForegroundColor Green
                $modified = $true
            } else {
                Write-Host "✅ Python already in PATH: $pythonPath" -ForegroundColor Green
            }
            break
        }
    }
}

# Add Cargo
if (-not $SkipCargo) {
    $cargoPath = "$HOME\.cargo\bin"
    if (Test-Path $cargoPath) {
        if ($currentPath -notlike "*$cargoPath*") {
            [Environment]::SetEnvironmentVariable("PATH", "$currentPath;$cargoPath", "Machine")
            $currentPath = [Environment]::GetEnvironmentVariable("PATH", "Machine")
            Write-Host "✅ Added Cargo: $cargoPath" -ForegroundColor Green
            $modified = $true
        } else {
            Write-Host "✅ Cargo already in PATH: $cargoPath" -ForegroundColor Green
        }
    }
}

if (-not $modified) {
    Write-Host "ℹ️  No changes needed" -ForegroundColor Gray
}

Write-Host ""
Write-Host "📝 Next steps:" -ForegroundColor Cyan
Write-Host "  1. Close ALL PowerShell/CMD terminals" -ForegroundColor Yellow
Write-Host "  2. Open a NEW terminal (don't need admin mode now)" -ForegroundColor Yellow
Write-Host "  3. Test: python --version" -ForegroundColor Yellow
Write-Host "  4. Test: cargo --version" -ForegroundColor Yellow
Write-Host ""

Read-Host "Press Enter to exit"
