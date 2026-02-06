@echo off
REM Run this as Administrator!
cls

echo ========================================
echo    Add Python to Global PATH
echo ========================================
echo.

echo Current PATH length: %PATH%
echo.

echo Adding: C:\Users\leadb\AppData\Local\Programs\Python\Python311
setx PATH "%PATH%;C:\Users\leadb\AppData\Local\Programs\Python\Python311"

if %ERRORLEVEL% EQU 0 (
    echo.
    echo [OK] Python added to global PATH
    echo.
    echo IMPORTANT:
    echo   1. Close ALL terminals and VS Code
    echo   2. Open a NEW terminal
    echo   3. Run: python --version
    echo.
) else (
    echo.
    echo [ERROR] Permission denied
    echo   This must be run as Administrator!
    echo.
)

pause
