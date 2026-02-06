@echo off
REM Quick start script for SuperVM Changelog
REM Run this to initialize the database and start using the changelog system

setlocal enabledelayedexpansion

cd /d "%~dp0"

echo.
echo ============================================
echo   SuperVM Changelog System - Quick Start
echo ============================================
echo.

echo Checking if Python is installed...
python --version >nul 2>&1
if errorlevel 1 (
    echo ERROR: Python is not installed or not in PATH
    echo Please install Python 3.7+ from https://www.python.org
    exit /b 1
)

echo Python found: $(python --version)
echo.

echo Initializing SQLite database...
python init-changelog.py
if errorlevel 1 (
    echo ERROR: Failed to initialize database
    exit /b 1
)

echo.
echo ✅ Setup complete!
echo.
echo Quick commands:
echo.
echo   Add a new entry:
echo     python ..\..\scripts\changelog.py add ^
echo       --date 2026-02-06 ^
echo       --time 14:30 ^
echo       --version 0.5.0 ^
echo       --level L0 ^
echo       --module aoem-core ^
echo       --property 测试 ^
echo       --desc "修改描述" ^
echo       --conclusion "结论"
echo.
echo   Query entries:
echo     python ..\..\scripts\changelog.py query --module aoem-core
echo.
echo   Export to Markdown:
echo     python ..\..\scripts\changelog.py export --format markdown
echo.
echo   View statistics:
echo     python ..\..\scripts\changelog.py stats --by-module
echo.
echo For more help, see tools/work-logger/mylog/README.md
echo.
