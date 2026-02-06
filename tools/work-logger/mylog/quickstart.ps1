#!/usr/bin/env pwsh
# Quick start script for SuperVM Changelog System
# Run this to initialize the database and start using the changelog

Set-StrictMode -Version Latest

$ScriptPath = Split-Path -Parent $MyInvocation.MyCommandPath
Push-Location $ScriptPath

Write-Host "`n============================================" -ForegroundColor Cyan
Write-Host "  SuperVM Changelog System - Quick Start" -ForegroundColor Cyan
Write-Host "============================================`n" -ForegroundColor Cyan

# Check if Python is installed
Write-Host "Checking if Python is installed..." -ForegroundColor Yellow
$pythonVersion = & python --version 2>&1
if ($LASTEXITCODE -ne 0) {
    Write-Host "❌ ERROR: Python is not installed or not in PATH" -ForegroundColor Red
    Write-Host "Please install Python 3.7+ from https://www.python.org" -ForegroundColor Yellow
    Pop-Location
    exit 1
}

Write-Host "✅ Python found: $pythonVersion`n" -ForegroundColor Green

# Initialize database
Write-Host "Initializing SQLite database..." -ForegroundColor Yellow
python init-changelog.py
if ($LASTEXITCODE -ne 0) {
    Write-Host "❌ ERROR: Failed to initialize database" -ForegroundColor Red
    Pop-Location
    exit 1
}

Write-Host "`n✅ Setup complete!`n" -ForegroundColor Green

Write-Host "Quick commands:" -ForegroundColor Cyan
Write-Host "`n  Add a new entry:" -ForegroundColor Yellow
Write-Host "    python ..\..\scripts\changelog.py add \`
      --date 2026-02-06 \`
      --time 14:30 \`
      --version 0.5.0 \`
      --level L0 \`
      --module aoem-core \`
      --property 测试 \`
      --desc ""修改描述"" \`
      --conclusion ""结论"""

Write-Host "`n  Query entries:" -ForegroundColor Yellow
Write-Host "    python ..\..\scripts\changelog.py query --module aoem-core"

Write-Host "`n  Export to Markdown:" -ForegroundColor Yellow
Write-Host "    python ..\..\scripts\changelog.py export --format markdown"

Write-Host "`n  View statistics:" -ForegroundColor Yellow
Write-Host "    python ..\..\scripts\changelog.py stats --by-module"

Write-Host "`nFor more help, see tools/work-logger/mylog/README.md`n" -ForegroundColor Green

Pop-Location
