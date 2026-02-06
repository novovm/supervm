@echo off
REM Add Python to PATH permanently
REM Run as Administrator!

echo.
echo ========================================
echo  Permanent PATH Setup for Python
echo ========================================
echo.

setx PATH "%PATH%;C:\Users\leadb\AppData\Local\Programs\Python\Python311"

if %ERRORLEVEL% EQU 0 (
    echo.
    echo [SUCCESS] Python added to global PATH
    echo.
    echo Next steps:
    echo   1. Close all terminals
    echo   2. Open a NEW terminal
    echo   3. Test: python --version
    echo.
) else (
    echo.
    echo [ERROR] Failed to update PATH
    echo         This script needs to be run with administrative privileges
    echo.
)

pause
