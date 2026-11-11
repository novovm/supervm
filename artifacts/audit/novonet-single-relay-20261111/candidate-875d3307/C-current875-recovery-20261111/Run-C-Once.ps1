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
 $f=[IO.File]::Open((Join-Path $root $name),[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::Read)
 try{$f.Write($bytes,0,$bytes.Length);$f.Flush($true)}finally{$f.Dispose()}
}
if(Test-Path -LiteralPath (Join-Path $root 'invoked-once.json')){throw 'Already invoked'}
foreach($entry in @(@($recovery,'a33bb0985175675336452dab2020f4c5739ebd31164151fa74a708482eae8b2b'),@($watch,'16783de0e2b59b862db5609cb6ccf4960cd289bdd608e654c93ee3bb88865219'),@($reader,'ccc8f9592a9fb12d68531dc2ace804efb544b9921e4f43c51147f2de57cbcd44'))){if((Get-FileHash -LiteralPath $entry[0]).Hash.ToLowerInvariant() -cne $entry[1]){throw 'Fixed script hash changed'}}
Save 'invoked-once.json' ([ordered]@{host='C';utc=[DateTime]::UtcNow.ToString('o');handoff='9a0aec5e';launcher_pid=$PID;recovery_invocations=1;recovery_sha256=(Get-FileHash $recovery).Hash.ToLowerInvariant();watch_sha256=(Get-FileHash $watch).Hash.ToLowerInvariant();reader_sha256=(Get-FileHash $reader).Hash.ToLowerInvariant();D_touched=$false;object_sent=$false})
try{
 $out=Join-Path $root 'recovery'
 $p=Start-Process -FilePath $pwsh -WindowStyle Hidden -ArgumentList @('-NoProfile','-NonInteractive','-File',('"'+$recovery+'"'),'-EvidenceRoot',('"'+$out+'"')) -RedirectStandardOutput (Join-Path $root 'recovery.stdout.log') -RedirectStandardError (Join-Path $root 'recovery.stderr.log') -PassThru
 Save 'recovery-started.json' ([ordered]@{pid=$p.Id;created_utc=$p.StartTime.ToUniversalTime().ToString('o');hidden=$true})
 $p.WaitForExit();$code=$p.ExitCode
 Save 'recovery-exit.json' ([ordered]@{pid=$p.Id;exit_code=$code;finished_utc=[DateTime]::UtcNow.ToString('o')})
 if($code -ne 0){exit $code}
 $result=Get-Content -LiteralPath (Join-Path $out 'result-public.json') -Raw|ConvertFrom-Json
 if($result.status -cne 'C_875_RECOVERED_READY_FOR_A_QUERY' -or $result.start -cne 'existing' -or $result.custody_unchanged -ne $true -or $result.configuration_unchanged -ne $true){throw 'Recovery success not proven'}
 $old=Get-Content -LiteralPath (Join-Path $repo 'target\c-relay-after-B-20261111T144822Z\helper-started.json') -Raw|ConvertFrom-Json
 $oldExit=Get-Content -LiteralPath (Join-Path $repo 'target\c-relay-after-B-20261111T144822Z\observer-exit.json') -Raw|ConvertFrom-Json
 if($old.helper_pid -ne 1700 -or $oldExit.helper_pid -ne 1700 -or $oldExit.exit_code -ne 1){throw 'Old exact observer exit differs'}
 $present=Get-CimInstance Win32_Process -Filter 'ProcessId=1700'
 $state=[ordered]@{old_pid=1700;old_created_utc=$old.helper_created_utc;actual_exit=$oldExit;process_present=($null -ne $present);process_stopped_by_launcher=$false}
 if($null -ne $present){
  $same=[Math]::Abs(($present.CreationDate.ToUniversalTime()-[DateTime]$old.helper_created_utc).TotalMilliseconds) -le 1
  $state.creation_matches=$same
  if($same){Save 'old-observer-check.json' $state;throw 'Old exact observer still enumerated; do not overlap'}
  $state.pid_reused_unrelated_not_touched=$true
 }
 Save 'old-observer-check.json' $state
 $e=Join-Path $root 'observer'
 $args=@('-NoProfile','-NonInteractive','-File',('"'+$watch+'"'),'-HostId','C','-Source','875d3307a1fc2ca34b45464ac3bf51a03619adc2','-NodeSha256','9174f29bad4bf1feb9acfbd5d2b5ed36bb47ff2a2cd1519d7742a2e1b944dee7','-WorkName','novonet-observation-875d3307-20261111','-EvidenceRoot',('"'+$e+'"'))
 $w=Start-Process -FilePath $pwsh -WindowStyle Hidden -ArgumentList $args -RedirectStandardOutput (Join-Path $root 'observer.stdout.log') -RedirectStandardError (Join-Path $root 'observer.stderr.log') -PassThru
 Save 'observer-started.json' ([ordered]@{host='C';observer_version=2;pid=$w.Id;created_utc=$w.StartTime.ToUniversalTime().ToString('o');node_pid=$result.pid;hidden=$true;maximum_wait_seconds=1800;source=$result.source;script=$watch;watch_sha256=(Get-FileHash $watch).Hash.ToLowerInvariant();reader_sha256=(Get-FileHash $reader).Hash.ToLowerInvariant();node_mutated_by_observer=$false;object_sent=$false})
 $w.WaitForExit();$wc=$w.ExitCode
 Save 'observer-exit.json' ([ordered]@{host='C';pid=$w.Id;exit_code=$wc;finished_utc=[DateTime]::UtcNow.ToString('o')})
 exit $wc
}catch{Save 'launcher-error.json' ([ordered]@{utc=[DateTime]::UtcNow.ToString('o');error=$_.Exception.Message;retry_performed=$false;D_touched=$false;object_sent=$false});throw}
