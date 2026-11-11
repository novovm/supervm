#Requires -Version 7.0
#Requires -RunAsAdministrator
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$root=$PSScriptRoot
$settings=Get-Content -LiteralPath (Join-Path $root 'settings.json') -Raw|ConvertFrom-Json
$pwsh='C:\Program Files\PowerShell\7\pwsh.exe'
function Save([string]$path,$value){
 $b=[Text.UTF8Encoding]::new($false).GetBytes(($value|ConvertTo-Json -Depth 15))
 $f=[IO.File]::Open($path,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
 try{$f.Write($b,0,$b.Length);$f.Flush($true)}finally{$f.Dispose()}
}
if ($settings.host -cne 'C' -or (Test-Path -LiteralPath $settings.evidence)) {throw 'Invalid host or consumed evidence directory'}
foreach($pair in @(@($settings.watch_script,$settings.watch_sha256),@($settings.reader_script,$settings.reader_sha256))) {
 if ((Get-FileHash -LiteralPath $pair[0] -Algorithm SHA256).Hash.ToLowerInvariant() -cne $pair[1]) {throw 'Original observer/adjacent reader hash changed'}
}
Save (Join-Path $root 'invoked-once.json') ([ordered]@{host='C';started_utc=[DateTime]::UtcNow.ToString('o');observer_version=2;source=$settings.source;watch_sha256=$settings.watch_sha256;reader_sha256=$settings.reader_sha256;observer_invocations_requested=1;node_mutation_requested=$false;object_requested=$false;D_touched=$false})
try {
 $old=Get-Content -LiteralPath (Join-Path $settings.old_evidence_root 'helper-started.json') -Raw|ConvertFrom-Json
 $oldExit=Get-Content -LiteralPath (Join-Path $settings.old_evidence_root 'observer-exit.json') -Raw|ConvertFrom-Json
 $oldReady=Get-Content -LiteralPath (Join-Path $settings.old_evidence_root 'observer\ready.json') -Raw|ConvertFrom-Json
 $oldLive=Get-Content -LiteralPath (Join-Path $settings.old_evidence_root 'observer\liveness.json') -Raw|ConvertFrom-Json
 $oldResult=Get-Content -LiteralPath (Join-Path $settings.old_evidence_root 'observer\result.json') -Raw|ConvertFrom-Json
 if($old.host_id -cne 'C' -or $old.observer_version -ne 2 -or $old.helper_pid -ne 6592 -or $old.source -cne $settings.source -or $oldReady.observer_pid -ne $old.helper_pid -or $oldReady.observer_version -ne 2 -or $oldExit.helper_pid -ne $old.helper_pid -or $oldExit.exit_code -ne 0 -or $oldLive.observer_pid -ne $old.helper_pid -or $oldLive.active -ne $false -or $oldResult.wait_expired -ne $true){throw 'Prior exact v2 observer terminal identity not proven'}
 $oldCreated=[DateTime]$old.helper_created_utc
 if([DateTime]$oldExit.finished_utc -le $oldCreated){throw 'Prior observer exit timeline differs'}
 $candidate=Get-CimInstance Win32_Process -Filter "ProcessId=$($old.helper_pid)"
 $check=[ordered]@{host='C';time_utc=[DateTime]::UtcNow.ToString('o');prior_pid=$old.helper_pid;prior_created_utc=$old.helper_created_utc;prior_exit=$oldExit;prior_final_liveness=$oldLive;prior_result=$oldResult;process_present=($null -ne $candidate);deliberate_termination=$false;node_stop_requested=$false;old_evidence_preserved=$true}
 if($null -ne $candidate){
  $sameCreated=[Math]::Abs(($candidate.CreationDate.ToUniversalTime()-$oldCreated).TotalMilliseconds) -le 1
  $check.actual_created_utc=$candidate.CreationDate.ToUniversalTime().ToString('o');$check.creation_matches=$sameCreated
  if($sameCreated){
   $process=Get-Process -Id $old.helper_pid -ErrorAction Stop
   try{if(!$process.HasExited){Save (Join-Path $root 'old-observer-state.json') $check;throw 'Prior observer unexpectedly active; do not stop or overlap'}}finally{$process.Dispose()}
   $check.kernel_exit_confirmed=$true
  }else{$check.pid_reused_unrelated_process_not_touched=$true}
 }else{$check.prior_pid_absent=$true}
 $check.prior_observer_exited=$true
 Save (Join-Path $root 'old-observer-state.json') $check
 $arguments=@('-NoProfile','-NonInteractive','-File',('"'+$settings.watch_script+'"'),'-HostId','C','-Source',$settings.source,'-NodeSha256',$settings.node_sha256,'-WorkName',$settings.work_name,'-EvidenceRoot',('"'+$settings.evidence+'"'))
 $p=Start-Process -FilePath $pwsh -WindowStyle Hidden -ArgumentList $arguments -RedirectStandardOutput (Join-Path $root 'observer.stdout.log') -RedirectStandardError (Join-Path $root 'observer.stderr.log') -PassThru
 Save (Join-Path $root 'helper-started.json') ([ordered]@{host='C';observer_version=2;helper_pid=$p.Id;helper_created_utc=$p.StartTime.ToUniversalTime().ToString('o');source=$settings.source;script=$settings.watch_script;watch_sha256=$settings.watch_sha256;reader_script=$settings.reader_script;reader_sha256=$settings.reader_sha256;host_id='C';evidence=$settings.evidence;hidden=$true;maximum_wait_seconds=1800;stdout='observer.stdout.log';stderr='observer.stderr.log';node_mutated=$false;object_sent=$false})
 $p.WaitForExit()
 Save (Join-Path $root 'observer-exit.json') ([ordered]@{host='C';observer_version=2;helper_pid=$p.Id;finished_utc=[DateTime]::UtcNow.ToString('o');exit_code=$p.ExitCode;node_mutated=$false;object_sent=$false})
 exit $p.ExitCode
}catch{
 Save (Join-Path $root 'launcher-error.json') ([ordered]@{host='C';time_utc=[DateTime]::UtcNow.ToString('o');error=$_.ToString();retry_executed=$false;node_stop_requested=$false})
 throw
}