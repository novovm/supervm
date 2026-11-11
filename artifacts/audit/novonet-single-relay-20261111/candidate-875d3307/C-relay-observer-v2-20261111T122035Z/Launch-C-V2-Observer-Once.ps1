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
 if($old.host_id -cne 'C' -or $old.helper_pid -ne 27480 -or $old.source -cne $settings.source) {throw 'Previous observer receipt differs'}
 $oldExitPath=Join-Path $settings.old_evidence_root 'observer-exit.json'
 $oldExit=$null
 if(Test-Path -LiteralPath $oldExitPath){$oldExit=Get-Content -LiteralPath $oldExitPath -Raw|ConvertFrom-Json;if($oldExit.helper_pid -ne $old.helper_pid){throw 'Previous exit PID mismatch'}}
 $candidate=Get-CimInstance Win32_Process -Filter "ProcessId=$($old.helper_pid)"
 $stop=[ordered]@{host='C';time_utc=[DateTime]::UtcNow.ToString('o');expected_pid=$old.helper_pid;expected_created_utc=$old.helper_created_utc;prior_exit_present=($null -ne $oldExit);prior_exit=$oldExit;process_present=($null -ne $candidate);deliberate_termination=$false;node_stop_requested=$false;old_evidence_preserved=$true}
 if($null -ne $candidate){
  $creationMatches=[Math]::Abs(($candidate.CreationDate.ToUniversalTime()-[DateTime]$old.helper_created_utc).TotalMilliseconds) -le 1
  $expectedArgs='-NoProfile -NonInteractive -File "'+$old.script+'" -HostId C -Source '+$settings.source+' -NodeSha256 '+$settings.node_sha256+' -WorkName '+$settings.work_name+' -EvidenceRoot "'+$old.evidence+'"'
  $commandMatches=($candidate.CommandLine -ceq ('"'+$pwsh+'" '+$expectedArgs)) -or ($candidate.CommandLine -ceq ($pwsh+' '+$expectedArgs))
  $pathMatches=$candidate.ExecutablePath -ieq $pwsh
  $stop.actual_created_utc=$candidate.CreationDate.ToUniversalTime().ToString('o');$stop.creation_matches=$creationMatches;$stop.command_matches=$commandMatches;$stop.executable_matches=$pathMatches
  if($creationMatches -and $commandMatches -and $pathMatches){
   $stop.verified_command=$candidate.CommandLine
   $proc=Get-Process -Id $old.helper_pid -ErrorAction Stop
   try {
    $null=$proc.Handle
    if([Math]::Abs(($proc.StartTime.ToUniversalTime()-[DateTime]$old.helper_created_utc).TotalMilliseconds) -gt 1 -or $proc.Path -ine $pwsh){throw 'Observer handle identity changed'}
    if(!$proc.HasExited){$proc.Kill();if(!$proc.WaitForExit(10000)){throw 'Previous observer exit not proven'};$stop.deliberate_termination=$true}
    $stop.process_exit_proven=$true
   } finally {$proc.Dispose()}
  }elseif($null -ne $oldExit -and !$creationMatches){$stop.pid_reused_unrelated_process_not_touched=$true;$stop.process_exit_proven=$true}
  else {Save (Join-Path $root 'old-observer-stop.json') $stop;throw 'Old observer PID/creation/command not proven; no stop or replacement'}
 }else{
  if($null -eq $oldExit){Save (Join-Path $root 'old-observer-stop.json') $stop;throw 'Old observer absent without terminal receipt; preserve evidence'}
  $stop.process_exit_proven=$true;$stop.already_exited=$true
 }
 Save (Join-Path $root 'old-observer-stop.json') $stop
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