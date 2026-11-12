#Requires -Version 7.0
#Requires -RunAsAdministrator
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$root=$PSScriptRoot
$repo='D:\WEB3_AI\SUPERVM'
$pwsh='C:\Program Files\PowerShell\7\pwsh.exe'
$recovery=Join-Path $repo 'scripts\windows\Resume-Novonet875C.ps1'
$watch=Join-Path $repo 'artifacts\audit\novonet-single-relay-20261111\Watch-Relay.ps1'
$reader=Join-Path (Split-Path $watch) 'Read-RelayProgress.ps1'
function Save([string]$name,$value){
 $bytes=[Text.UTF8Encoding]::new($false).GetBytes(($value|ConvertTo-Json -Depth 12))
 $file=[IO.File]::Open((Join-Path $root $name),[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::Read)
 try{$file.Write($bytes,0,$bytes.Length);$file.Flush($true)}finally{$file.Dispose()}
}
if(Test-Path -LiteralPath "$root/invoked-once.json"){throw 'Already invoked'}
$pins=Get-Content -LiteralPath "$root/script-pins.json" -Raw|ConvertFrom-Json -AsHashtable
foreach($p in @($recovery,$watch,$reader)){if((Get-FileHash -LiteralPath $p).Hash.ToLowerInvariant() -cne $pins[$p]){throw 'Script pin mismatch'}}
Save 'invoked-once.json' @{host='C';utc=[DateTime]::UtcNow.ToString('o');handoff='19daf8ff';launcher_pid=$PID;expected_crashed_pid=3228;recovery_attempts=1;D_touched=$false;object_sent=$false}
try{
 $old=@(Get-CimInstance Win32_Process -Filter "Name='pwsh.exe'"|Where-Object {$_.CommandLine -like '*Watch-Relay.ps1*'})
 Save 'old-observer-check.json' @{utc=[DateTime]::UtcNow.ToString('o');matching_count=$old.Count;terminated=$false;processes=@($old|Select-Object ProcessId,CreationDate,ExecutablePath)}
 if($old.Count){throw 'Observer already present; do not overlap or terminate'}
 $p=Start-Process -FilePath $pwsh -WindowStyle Hidden -ArgumentList @('-NoProfile','-NonInteractive','-File',('"'+$recovery+'"'),'-ExpectedCrashedPid','3228','-EvidenceRoot',('"'+$root+'\recovery"')) -RedirectStandardOutput "$root/recovery.stdout.log" -RedirectStandardError "$root/recovery.stderr.log" -PassThru
 Save 'recovery-started.json' @{pid=$p.Id;created_utc=$p.StartTime.ToUniversalTime().ToString('o');script_sha256=$pins[$recovery]}
 $p.WaitForExit();$code=$p.ExitCode
 Save 'recovery-exit.json' @{pid=$p.Id;exit_code=$code;utc=[DateTime]::UtcNow.ToString('o')}
 if($code -ne 0){exit $code}
 $result=Get-Content -LiteralPath "$root/recovery/result-public.json" -Raw|ConvertFrom-Json
 if($result.status -cne 'C_875_RECOVERED_READY_FOR_A_QUERY' -or !$result.custody_unchanged -or !$result.configuration_unchanged -or $result.start -cne 'existing'){throw 'Recovery not confirmed'}
 $args=@('-NoProfile','-NonInteractive','-File',('"'+$watch+'"'),'-HostId','C','-Source','875d3307a1fc2ca34b45464ac3bf51a03619adc2','-NodeSha256','9174f29bad4bf1feb9acfbd5d2b5ed36bb47ff2a2cd1519d7742a2e1b944dee7','-WorkName','novonet-observation-875d3307-20261111','-EvidenceRoot',('"'+$root+'\observer"'),'-MaximumWaitSeconds','7200')
 $w=Start-Process -FilePath $pwsh -WindowStyle Hidden -ArgumentList $args -RedirectStandardOutput "$root/observer.stdout.log" -RedirectStandardError "$root/observer.stderr.log" -PassThru
 Save 'observer-started.json' @{host='C';observer_version=3;pid=$w.Id;created_utc=$w.StartTime.ToUniversalTime().ToString('o');node_pid=$result.pid;maximum_wait_seconds=7200;hidden=$true;watch_sha256=$pins[$watch];reader_sha256=$pins[$reader];object_sent=$false;D_touched=$false}
 $w.WaitForExit();$code=$w.ExitCode
 Save 'observer-exit.json' @{pid=$w.Id;utc=[DateTime]::UtcNow.ToString('o');exit_code=$code}
 exit $code
}catch{Save 'launcher-error.json' @{utc=[DateTime]::UtcNow.ToString('o');error=$_.Exception.Message;retry_performed=$false;D_touched=$false;object_sent=$false};throw}
