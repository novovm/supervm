param([Parameter(Mandatory)][string]$EvidenceRoot,[ValidateSet('Lifecycle','Single','Concurrent')][string]$Mode='Lifecycle')
$ErrorActionPreference='Stop'
if(Test-Path -LiteralPath $EvidenceRoot){throw 'EvidenceRoot must be new'}
$root=[IO.Path]::GetFullPath($EvidenceRoot)
New-Item -ItemType Directory -Path $root | Out-Null
$source=New-Item -ItemType Directory -Path (Join-Path $root 'source')
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'qualify.c'),(Join-Path $PSScriptRoot 'capture.asm'),$PSCommandPath -Destination $source.FullName
$vswhere='C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe'
$vs=(& $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath).Trim()
if(!$vs){throw 'MSVC x64 tools missing'}
$dev=Join-Path $vs 'Common7\Tools\VsDevCmd.bat'
$build=@"
@echo off
call "$dev" -arch=x64 -host_arch=x64
if errorlevel 1 exit /b %errorlevel%
cd /d "$root"
ml64 /nologo /c /Zi /Fo"capture.obj" "$source\capture.asm"
if errorlevel 1 exit /b %errorlevel%
cl /nologo /std:c11 /O2 /Zi /W4 /WX /D_WIN32_WINNT=0x0602 /Fe:"qualify.exe" /Fd:"qualify-compile.pdb" "$source\qualify.c" capture.obj /link /DEBUG:FULL /PDB:qualify.pdb /INCREMENTAL:NO
exit /b %errorlevel%
"@
Set-Content -LiteralPath (Join-Path $root 'build.cmd') -Value $build -Encoding ascii
& cmd.exe /d /c (Join-Path $root 'build.cmd') *> (Join-Path $root 'build.log')
$buildExit=$LASTEXITCODE
if($buildExit -ne 0){throw "Build failed: $buildExit; see build.log"}
$start=[DateTime]::UtcNow
$arguments='"'+$root+'"'
if($Mode -eq 'Concurrent'){$arguments+=' --concurrent'}
if($Mode -eq 'Single'){$arguments+=' --single'}
$p=Start-Process -FilePath (Join-Path $root 'qualify.exe') -ArgumentList $arguments -WorkingDirectory $root -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $root 'stdout.txt') -RedirectStandardError (Join-Path $root 'stderr.txt')
$timedOut=-not $p.WaitForExit(60000)
if($timedOut){$p.Kill();$p.WaitForExit()}
$p.Refresh()
$end=[DateTime]::UtcNow
$manifest=[ordered]@{
 schema=1;start_utc=$start.ToString('o');end_utc=$end.ToString('o');pid=$p.Id
 build_exit=$buildExit;exit_code=$p.ExitCode;timed_out=$timedOut;mode=$Mode
 scope='New standalone caller/callee and private heap only; no NOVOVM process or original data'
 files=@(Get-Item -LiteralPath (Join-Path $root 'qualify.exe'),(Join-Path $root 'qualify.pdb'),$PSCommandPath,(Join-Path $PSScriptRoot 'qualify.c'),(Join-Path $PSScriptRoot 'capture.asm'),'C:\Windows\System32\ntdll.dll' | ForEach-Object { @{name=$_.Name;bytes=$_.Length;sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLower()} })
}
$manifest | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $root 'execution.json') -Encoding utf8
if($timedOut -or $p.ExitCode -ne 0){throw 'Qualification failed; evidence retained, no retry'}
Get-Content -LiteralPath (Join-Path $root 'stdout.txt')
