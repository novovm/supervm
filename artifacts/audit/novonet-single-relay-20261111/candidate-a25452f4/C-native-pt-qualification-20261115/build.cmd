@echo off
call "D:\Program Files\Microsoft Visual Studio\18\Professional\VC\Auxiliary\Build\vcvars64.bat" >nul
if errorlevel 1 exit /b %errorlevel%
cl /nologo /W4 /WX /O2 /Zi /MT /EHsc PtQualification.cpp /Fe:PtQualification.exe /Fo:PtQualification.obj /Fd:PtQualification-compile.pdb /link /DEBUG:FULL /PDB:PtQualification.pdb /INCREMENTAL:NO dbghelp.lib
exit /b %errorlevel%
