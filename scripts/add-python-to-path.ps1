# Run this as Administrator in PowerShell!

$pythonPath = "C:\Users\leadb\AppData\Local\Programs\Python\Python311"
$currentPath = [Environment]::GetEnvironmentVariable("PATH", "Machine")

Write-Host "Current PATH: $currentPath" -ForegroundColor Gray
Write-Host ""

if ($currentPath -notlike "*$pythonPath*") {
    Write-Host "Adding Python to global PATH..." -ForegroundColor Cyan
    [Environment]::SetEnvironmentVariable("PATH", "$currentPath;$pythonPath", "Machine")
    Write-Host "✅ Success! Python added to global PATH" -ForegroundColor Green
} else {
    Write-Host "✅ Python already in PATH" -ForegroundColor Green
}

Write-Host ""
Write-Host "⚠️  IMPORTANT: Close ALL terminals and open a NEW one to test!" -ForegroundColor Yellow
Write-Host "   Then run: python --version" -ForegroundColor Yellow
