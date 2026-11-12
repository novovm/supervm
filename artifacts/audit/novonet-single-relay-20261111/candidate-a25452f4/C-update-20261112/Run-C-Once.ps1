#Requires -Version 7.0
#Requires -RunAsAdministrator
$ErrorActionPreference='Stop'
$PSNativeCommandUseErrorActionPreference=$false
Set-StrictMode -Version Latest
$root=$PSScriptRoot
$pwsh='C:\Program Files\PowerShell\7\pwsh.exe'
$tools=Join-Path $root 'tools'
$updater=Join-Path $tools 'Update-Windows.ps1'
$package=Join-Path $root 'package\novonet-observation-a25452f4-windows-x86_64'
$work='C:\ProgramData\NOVOVM\selfrun\91129655606029\C\novonet-observation-875d3307-20261111'
$oldExe='C:\ProgramData\NOVOVM\releases\novonet-observation-875d3307-windows-x86_64\bin\novovm-node.exe'
$taskName='NOVOVM-C-875d3307-Existing-20261111'
function Save([string]$name,$data){
 $bytes=[Text.UTF8Encoding]::new($false).GetBytes(($data|ConvertTo-Json -Depth 18))
 $f=[IO.File]::Open((Join-Path $root $name),[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::Read)
 try{$f.Write($bytes,0,$bytes.Length);$f.Flush($true)}finally{$f.Dispose()}
}
if(Test-Path -LiteralPath "$root/invoked-once.json"){throw 'Already invoked; no retry'}
$inputs=Get-Content "$root/inputs-verified-public.json" -Raw|ConvertFrom-Json
foreach($a in $inputs.tools_verified){$p=Join-Path $tools $a.name;if((Get-FileHash $p).Hash.ToLowerInvariant() -cne $a.sha256){throw 'Tool pin differs'}}
Save 'invoked-once.json' @{host='C';utc=[DateTime]::UtcNow.ToString('o');launcher_pid=$PID;source=$inputs.source;updater_calls_maximum=1;observer_started=$false;D_operated=$false}
try{
 . "$tools/NovonetExistingProcess.ps1"
 $tokens=$null;$errors=$null
 $ast=[Management.Automation.Language.Parser]::ParseFile($updater,[ref]$tokens,[ref]$errors)
 if($errors.Count){throw 'Updater parser errors'}
 foreach($name in @('Assert-CrashedOwnerEvidence','ConvertFrom-NovonetCrashEvent','Assert-StoppedOwnerEvidence')){
  $defs=@($ast.FindAll({param($n)$n -is [Management.Automation.Language.FunctionDefinitionAst]},$false)|Where-Object Name -CEQ $name)
  if($defs.Count -ne 1){throw 'Original preflight gate absent'}
  . ([scriptblock]::Create($defs[0].Extent.Text))
 }
 $task=Get-ScheduledTask -TaskName $taskName
 $info=Get-ScheduledTaskInfo -TaskName $taskName
 $execution=Get-NovonetNodeExecutionState
 $nodes=@($execution.active)
 $ports=@(Get-NetUDPEndpoint|Where-Object LocalPort -in 41052,42052)
 $sid=if($task.Principal.UserId.StartsWith('S-1-')){$task.Principal.UserId}else{([Security.Principal.NTAccount]::new($task.Principal.UserId)).Translate([Security.Principal.SecurityIdentifier]).Value}
 $proof=[pscustomobject]@{task_state=$task.State.ToString();task_sid=$sid;restart_count=[int]$task.Settings.RestartCount;trigger_count=@($task.Triggers|Where-Object {$null -ne $_}).Count;task_exit_code=[long]$info.LastTaskResult;last_run_utc=$info.LastRunTime.ToUniversalTime();executing_nodes=$nodes.Count;bound_ports=$ports.Count;progress_utc=(Get-Item "$work/logs/progress.json").LastWriteTimeUtc}
 Save 'old-owner-public.json' @{utc=[DateTime]::UtcNow.ToString('o');task=$taskName;lifecycle=$proof;execution=$execution;ports=@($ports|Select-Object LocalAddress,LocalPort,OwningProcess);old_node_sha256=(Get-FileHash $oldExe).Hash.ToLowerInvariant();progress_sha256=(Get-FileHash "$work/logs/progress.json").Hash.ToLowerInvariant()}
 if($sid -cne 'S-1-5-19' -or $proof.restart_count -ne 0 -or $proof.trigger_count -ne 0){throw 'Old task authority differs'}
 if((Get-FileHash $oldExe).Hash.ToLowerInvariant() -cne '9174f29bad4bf1feb9acfbd5d2b5ed36bb47ff2a2cd1519d7742a2e1b944dee7'){throw 'Old875 hash differs'}
 $mode=$null
 if($nodes.Count -eq 1){
  if($task.State.ToString() -cne 'Running' -or $nodes[0].ExecutablePath -ine $oldExe -or (Invoke-CimMethod $nodes[0] -MethodName GetOwnerSid).Sid -cne 'S-1-5-19'){throw 'Live875 owner differs'}
  if($ports.Count -ne 2 -or @($ports|Where-Object OwningProcess -NE $nodes[0].ProcessId).Count){throw 'Live875 ports differ'}
  $mode='running'
 }elseif($nodes.Count -eq 0 -and $ports.Count -eq 0){
  switch([long]$info.LastTaskResult){
   0 {Assert-StoppedOwnerEvidence $proof $false;$mode='ResumeStopped'}
   267014 {Assert-StoppedOwnerEvidence $proof $true;$mode='RecoverInterrupted'}
   {$_ -in @(3221225477,3221226356)} {
    $events=@(Get-WinEvent -FilterHashtable @{LogName='Application';Id=1000;StartTime=$info.LastRunTime}|ForEach-Object{ConvertFrom-NovonetCrashEvent ([xml]$_.ToXml()) $_.TimeCreated.ToUniversalTime() $_.RecordId $oldExe}|Sort-Object event_utc -Descending)
    if(!$events.Count){throw 'Original crash event absent'}
    $crash=$events[0]
    $crash|Add-Member -NotePropertyMembers @{task_state=$proof.task_state;task_sid=$sid;restart_count=$proof.restart_count;trigger_count=$proof.trigger_count;task_exit_code=$proof.task_exit_code;last_run_utc=$proof.last_run_utc}
    $exit=Get-Content "$work/logs/exit.json" -Raw|ConvertFrom-Json
    Save 'old-crash-public.json' @{crash=$crash;exit=$exit}
    Assert-CrashedOwnerEvidence $crash $oldExe $exit
    $mode='RecoverCrashed'
   }
   default {throw 'Unsupported old lifecycle; no updater call'}
  }
 }else{throw 'Unproven old process/port state; no updater call'}
 Save 'mode-selected-public.json' @{utc=[DateTime]::UtcNow.ToString('o');mode=$mode;updater_sha256=(Get-FileHash $updater).Hash.ToLowerInvariant();package=$package;old_program_started=$false;initialize_requested=$false}
 $argv=@('-NoProfile','-ExecutionPolicy','RemoteSigned','-File',('"'+$updater+'"'),'-HostId','C','-PackageRoot',('"'+$package+'"'),'-EvidenceRoot',('"'+$root+'\C-update"'))
 if($mode -cne 'running'){$argv+=('-'+$mode)}
 $p=Start-Process -FilePath $pwsh -WindowStyle Hidden -ArgumentList $argv -RedirectStandardOutput "$root/updater.stdout.log" -RedirectStandardError "$root/updater.stderr.log" -PassThru
 Save 'updater-started.json' @{pid=$p.Id;created_utc=$p.StartTime.ToUniversalTime().ToString('o');mode=$mode}
 $p.WaitForExit();$code=$p.ExitCode
 Save 'updater-exit.json' @{utc=[DateTime]::UtcNow.ToString('o');pid=$p.Id;exit_code=$code;attempts=1}
 if($code -ne 0){exit $code}
 $r=Get-Content "$root/C-update/result.json" -Raw|ConvertFrom-Json
 if($r.source -cne 'a25452f4c49be37cd984e650f2079642e6c273b3' -or !$r.update_completed -or !$r.existing_recovery_opened -or $r.start -cne 'existing' -or !$r.custody_unchanged -or !$r.progress_state_unchanged -or $r.initialize_executed){throw 'Update success invariants not confirmed'}
 $after=Get-Content "$root/C-update/after.json" -Raw|ConvertFrom-Json
 $after|Select-Object host_id,time,source,pid,exe,node_sha256,sid,database,start,initialize_executed,keys_generated,custody_unchanged,aoem_module,aoem_sha256,new_task,listeners,new_command_file_present|ConvertTo-Json -Depth 10|Set-Content "$root/after-public.json" -Encoding utf8
 Save 'result-public.json' @{status='C_A254_EXISTING_READY_FOR_A_QUERY';utc=[DateTime]::UtcNow.ToString('o');exit_code=$code;result=$r;external_authenticated_query_required=$true;observer_started=$false;D_operated=$false}
}catch{Save 'launcher-error-public.json' @{utc=[DateTime]::UtcNow.ToString('o');error=$_.Exception.Message;retry_performed=$false;observer_started=$false;D_operated=$false};throw}
