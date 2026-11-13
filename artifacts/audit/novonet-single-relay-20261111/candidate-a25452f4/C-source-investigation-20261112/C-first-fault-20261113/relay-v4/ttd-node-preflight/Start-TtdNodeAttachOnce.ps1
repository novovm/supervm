#Requires -Version 7.0
#Requires -RunAsAdministrator
param()
$ErrorActionPreference='Stop';Set-StrictMode -Version Latest
$base='D:\WEB3_AI\SUPERVM\target\c-ttd-node-20261113'
$work=Join-Path $base 'run-private-02';$out=Join-Path $base 'launch-public-02'
$taskName='NOVOVM-C-Ttd-AttachNode-20261113-02';$pwsh='C:\Program Files\PowerShell\7\pwsh.exe'
$toolBase='D:\WEB3_AI\SUPERVM\target\c-first-fault-20261113\ttd-tools'
$watch='D:\WEB3_AI\SUPERVM\artifacts\audit\novonet-single-relay-20261111\Watch-Relay.ps1'
. (Join-Path $base 'KernelProcessIdentity.ps1')
$recorder=$null;$nodeIdentity=$null;$success=$false;$observer=$null;$observerIdentity=$null;$observerSaved=$false;$taskStarted=$false;$protected=@();$before=@{}
function Save([string]$name,$value){$p=Join-Path $out $name;if((Test-Path -LiteralPath $p) -or (Test-Path -LiteralPath ($p+'.tmp'))){throw 'Evidence exists'};$value|ConvertTo-Json -Depth 10|Set-Content -LiteralPath ($p+'.tmp');Move-Item -LiteralPath ($p+'.tmp') -Destination $p}
function Finish-Observer([string]$reason){
 if(!$observer -or $observerSaved){return}
 $forced=$false
 if(!$observer.HasExited){
  $c=Get-CimInstance Win32_Process -Filter "ProcessId=$($observer.Id)"
  if(!$c -or $c.ExecutablePath -ine $pwsh -or $c.CreationDate.ToUniversalTime().Ticks -ne ([DateTime]$observerIdentity.created_utc).ToUniversalTime().Ticks -or !$c.CommandLine.Contains((Join-Path $work 'observer')) -or !$c.CommandLine.Contains('Watch-Relay.ps1')){throw 'Observer identity mismatch; preserve'}
  Stop-Process -Id $observer.Id -Force;if(!$observer.WaitForExit(15000)){throw 'Observer exit not confirmed'};$forced=$true
 }
 $observer.Refresh();Save 'observer-exit-public.json' @{utc=[DateTime]::UtcNow.ToString('o');pid=$observer.Id;exit_code=$observer.ExitCode;forced=$forced;reason=$reason;relay_ready=$false};$script:observerSaved=$true
}
try{
 if((Test-Path -LiteralPath $work) -or (Test-Path -LiteralPath $out)){throw 'Run exists; no retry/overwrite'}
 New-Item -ItemType Directory -Path $out|Out-Null
 $proof=Get-Content -LiteralPath (Join-Path $base 'attach-control-03/replay-public.json') -Raw|ConvertFrom-Json
 if(!$proof.reverse_write_control_verified){throw 'LocalService attached reverse writer not verified'}
 $plan=Get-Content -LiteralPath (Join-Path $base 'plan-attach.json') -Raw|ConvertFrom-Json
 foreach($pin in $plan.inputs){if((Get-FileHash -LiteralPath $pin.path).Hash -ine $pin.sha256){throw 'Pinned input changed'}}
 if(Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue){throw 'Diagnostic task already exists'}
 $processes=@(Get-CimInstance Win32_Process -Filter "Name='novovm-node.exe' OR Name='cdb.exe' OR Name='TTD.exe'")
 if($processes.Count -or @(Get-NetUDPEndpoint|Where-Object LocalPort -in 41052,42052).Count){throw 'Node/debugger/recorder/C ports present'}
 if(@(Get-CimInstance Win32_Process -Filter "Name='pwsh.exe'"|Where-Object {$_.CommandLine -like '*Watch-Relay.ps1*'}).Count){throw 'Observer already present'}
 $tasks=@(Get-ScheduledTask|Where-Object {$_.TaskName -like 'NOVOVM*C*'})
 if(@($tasks|Where-Object State -eq 'Running').Count){throw 'Existing C task running'}
 Save 'prior-tasks-public.json' @($tasks|Select-Object TaskName,@{n='state';e={$_.State.ToString()}})
 foreach($name in @('novovm-node.exe','ttd-pointer-control-aligned.exe')){if(Test-Path -LiteralPath ('HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options\'+$name)){throw 'Existing image diagnostics; no changes made'}}
 if((Get-PSDrive D).Free -lt 16GB){throw 'Insufficient diagnostic disk reserve'}
 if((Get-FileHash -LiteralPath $plan.node).Hash -ine $plan.node_sha256){throw 'Original node differs'}
 $cfg=Get-Content -LiteralPath $plan.original_config -Raw|ConvertFrom-Json
 if($cfg.database -cne $plan.database -or !(Test-Path -LiteralPath $cfg.database -PathType Container) -or (Get-FileHash -LiteralPath $cfg.library).Hash -ine $plan.runtime_sha256){throw 'Original DB/runtime mismatch'}
 $net=Get-Content -LiteralPath $cfg.novonet_config_file -Raw|ConvertFrom-Json
 $protected=@($cfg.signing_key_file,$cfg.transport_key_file,$net.carrier_identity_file,$cfg.novonet_config_file,$plan.original_config)
 foreach($path in $protected){$before[$path]=(Get-FileHash -LiteralPath $path).Hash}
 New-Item -ItemType Directory -Path $work|Out-Null
 $sid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
 & icacls.exe $work /inheritance:r /grant:r "*$($sid):(OI)(CI)F" '*S-1-5-18:(OI)(CI)F' '*S-1-5-32-544:(OI)(CI)F' '*S-1-5-19:(OI)(CI)M'|Out-Null
 if($LASTEXITCODE -ne 0){throw 'Private evidence ACL failed'}
 Copy-Item -LiteralPath $plan.original_config -Destination (Join-Path $work 'run.json')
 Copy-Item -LiteralPath (Join-Path $base 'plan-attach.json') -Destination (Join-Path $work 'plan.json')
 Copy-Item -LiteralPath (Join-Path $base 'NodeOwnerAttach.ps1') -Destination $work
 Copy-Item -LiteralPath (Join-Path $toolBase 'ttd-pointer-control-aligned.exe') -Destination $work
 New-Item -ItemType Directory -Path (Join-Path $work 'tools')|Out-Null
 foreach($p in $plan.recorder_files){
  $sig=Get-AuthenticodeSignature -LiteralPath $p.path
  if($sig.Status -ne 'Valid' -or $sig.SignerCertificate.Subject -notmatch 'Microsoft Corporation' -or (Get-FileHash -LiteralPath $p.path).Hash -ine $p.sha256){throw 'Recorder input signature/hash differs'}
  Copy-Item -LiteralPath $p.path -Destination (Join-Path $work 'tools')
  if((Get-FileHash -LiteralPath (Join-Path $work ('tools\'+[IO.Path]::GetFileName($p.path)))).Hash -ine $p.sha256){throw 'Copied recorder differs'}
 }
 if(Test-Path -LiteralPath $plan.progress){
  if((Get-Item -LiteralPath $plan.progress).Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Progress reparse point'}
  Copy-Item -LiteralPath $plan.progress -Destination (Join-Path $work 'prior-progress-private.json')
  if((Get-FileHash -LiteralPath $plan.progress).Hash -ine (Get-FileHash -LiteralPath (Join-Path $work 'prior-progress-private.json')).Hash){throw 'Progress archive differs'}
 }
 $args='-NoProfile -NonInteractive -File "'+(Join-Path $work 'NodeOwnerAttach.ps1')+'" -WorkRoot "'+$work+'"'
 $action=New-ScheduledTaskAction -Execute $pwsh -Argument $args
 $principal=New-ScheduledTaskPrincipal -UserId 'NT AUTHORITY\LOCAL SERVICE' -LogonType ServiceAccount
 $settings=New-ScheduledTaskSettingsSet -ExecutionTimeLimit (New-TimeSpan -Minutes 130) -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
 Register-ScheduledTask -TaskName $taskName -Action $action -Principal $principal -Settings $settings|Out-Null
 Save 'preflight-public.json' @{utc=[DateTime]::UtcNow.ToString('o');node_sha256=$plan.node_sha256;runtime_sha256=$plan.runtime_sha256;start='existing';identity_preserved=$true;node_absent_before=$true;pageheap_enabled=$false;ring_tail_only=$true;max_trace_mb=2048;node_run_seconds=7200;owner_hard_seconds=7320;task_limit_minutes=130;A_must_not_send=$true;relay_ready=$false}
 Start-ScheduledTask -TaskName $taskName;$taskStarted=$true
 $created=Join-Path $work 'node-created-public.json';$clock=[Diagnostics.Stopwatch]::StartNew()
 while(!(Test-Path -LiteralPath $created)){
  if($clock.Elapsed.TotalSeconds -gt 90 -or (Test-Path -LiteralPath (Join-Path $work 'owner-error-public.json'))){throw 'Suspended node unavailable'}
  Start-Sleep -Milliseconds 200
 }
 $nodeIdentity=Get-Content -LiteralPath $created -Raw|ConvertFrom-Json
 $facts=[KernelProcessIdentity]::Read($nodeIdentity.pid)
 if($facts.path -ine $plan.node -or $facts.owner_sid -cne 'S-1-5-19' -or ([DateTime]$facts.created_utc).ToUniversalTime().Ticks -ne ([DateTime]$nodeIdentity.created_utc).ToUniversalTime().Ticks){throw 'Suspended node kernel identity differs'}
 Save 'node-kernel-identity-public.json' $facts
 $ttd=Join-Path $work 'tools\TTD.exe';$stdout=Join-Path $work 'recorder-private.stdout';$stderr=Join-Path $work 'recorder-private.stderr'
 $recorder=Start-Process -FilePath $ttd -ArgumentList @('-accepteula','-noUI','-ring','-maxFile','2048','-out',('"'+(Join-Path $work 'node-private.run')+'"'),'-attach',"$($nodeIdentity.pid)") -WindowStyle Hidden -PassThru -RedirectStandardOutput $stdout -RedirectStandardError $stderr
 Save 'recorder-start-public.json' @{pid=$recorder.Id;created_utc=$recorder.StartTime.ToUniversalTime().ToString('o');attached_pid=$nodeIdentity.pid;ring_tail_only=$true;relay_ready=$false}
 $clock.Restart()
 while($true){
  $raw=Get-Content -LiteralPath $stdout -Raw -ErrorAction SilentlyContinue
  if($raw -match "Recording has started of process \(PID:$($nodeIdentity.pid)\)"){break}
  if($recorder.HasExited -or $clock.Elapsed.TotalSeconds -gt 75){throw 'Recorder not ready within bound; node main not resumed'}
  Start-Sleep -Milliseconds 200
 }
 @{recording_started=$true;pid=$nodeIdentity.pid;created_utc=$nodeIdentity.created_utc}|ConvertTo-Json|Set-Content -LiteralPath (Join-Path $work 'resume-approved.tmp')
 Move-Item -LiteralPath (Join-Path $work 'resume-approved.tmp') -Destination (Join-Path $work 'resume-approved.json')
 $clock.Restart()
 while(!(Test-Path -LiteralPath (Join-Path $work 'node-resumed-public.json'))){
  if($clock.Elapsed.TotalSeconds -gt 15 -or (Test-Path -LiteralPath (Join-Path $work 'owner-error-public.json'))){throw 'Node resume not confirmed'}
  Start-Sleep -Milliseconds 100
 }
 Save 'recording-start-public.json' @{utc=[DateTime]::UtcNow.ToString('o');node=$facts;node_run_seconds=7200;max_trace_mb=2048;ring_tail_only=$true;root_cause_fixed=$false;relay_ready=$false}
 $freshDeadline=[DateTime]::UtcNow.AddSeconds(120)
 while([DateTime]::UtcNow -lt $freshDeadline){
  if(Test-Path -LiteralPath (Join-Path $work 'node-exit-public.json')){break}
  if((Test-Path -LiteralPath $plan.progress) -and (Get-Item -LiteralPath $plan.progress).LastWriteTimeUtc -gt ([DateTime]$facts.created_utc).ToUniversalTime()){break}
  Start-Sleep -Seconds 1
 }
 if(!(Test-Path -LiteralPath (Join-Path $work 'node-exit-public.json'))){
  $watchArgs=@('-NoProfile','-NonInteractive','-File',('"'+$watch+'"'),'-HostId','C','-Source','a25452f4c49be37cd984e650f2079642e6c273b3','-NodeSha256',$plan.node_sha256,'-WorkName','novonet-observation-a25452f4-20261112','-EvidenceRoot',('"'+(Join-Path $work 'observer')+'"'),'-MaximumWaitSeconds','7200')
  $observer=Start-Process -FilePath $pwsh -ArgumentList $watchArgs -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $work 'observer-private.stdout') -RedirectStandardError (Join-Path $work 'observer-private.stderr')
  $oc=Get-CimInstance Win32_Process -Filter "ProcessId=$($observer.Id)"
  if(!$oc){throw 'Observer identity missing'}
  $observerIdentity=@{pid=$observer.Id;created_utc=$oc.CreationDate.ToUniversalTime().ToString('o')};Save 'observer-start-public.json' $observerIdentity
 }
 $deadline=[DateTime]::UtcNow.AddSeconds(7340)
 while(!(Test-Path -LiteralPath (Join-Path $work 'owner-terminal-public.json'))){
  if([DateTime]::UtcNow -gt $deadline){throw 'Owner completion bound reached'}
  if($recorder.HasExited -and !(Test-Path -LiteralPath (Join-Path $work 'node-exit-public.json'))){Start-Sleep -Seconds 2;if(!(Test-Path -LiteralPath (Join-Path $work 'node-exit-public.json'))){throw 'Recorder exited before node exit proof'}}
  if($observer -and $observer.HasExited -and !$observerSaved){Finish-Observer 'natural_observer_exit'}
  Start-Sleep -Seconds 3
 }
 if(!$recorder.WaitForExit(60000)){throw 'Recorder flush exceeded bound'}
 $recorder.Refresh();Save 'recorder-exit-public.json' @{utc=[DateTime]::UtcNow.ToString('o');exit_code=$recorder.ExitCode;not_node_exit_code=$true}
 $success=$true
}catch{
 if(Test-Path -LiteralPath $out){Save 'launch-error-public.json' @{utc=[DateTime]::UtcNow.ToString('o');error=$_.Exception.Message;root_cause_fixed=$false;relay_ready=$false;no_retry=$true}}
}finally{
 if($taskStarted){
  if(!(Test-Path -LiteralPath (Join-Path $work 'owner-terminal-public.json'))){
   if($nodeIdentity -and $recorder -and !$recorder.HasExited){
    try{
     $facts=[KernelProcessIdentity]::Read($nodeIdentity.pid)
     if($facts.path -ine $plan.node -or $facts.owner_sid -cne 'S-1-5-19' -or ([DateTime]$facts.created_utc).ToUniversalTime().Ticks -ne ([DateTime]$nodeIdentity.created_utc).ToUniversalTime().Ticks){throw 'Stop target identity differs'}
     $stop=Start-Process -FilePath $ttd -ArgumentList @('-noUI','-stop',"$($nodeIdentity.pid)") -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $work 'stop-private.stdout') -RedirectStandardError (Join-Path $work 'stop-private.stderr')
     if(!$stop.WaitForExit(15000)){$stop.Kill();$stop.WaitForExit()}
    }catch{Save 'stop-recorder-error-public.json' @{error=$_.Exception.Message}}
   }
   @{utc=[DateTime]::UtcNow.ToString('o');reason='coordinator_end'}|ConvertTo-Json|Set-Content -LiteralPath (Join-Path $work 'stop-request.json')
   $limit=[DateTime]::UtcNow.AddSeconds(20)
   while(!(Test-Path -LiteralPath (Join-Path $work 'owner-terminal-public.json')) -and [DateTime]::UtcNow -lt $limit){Start-Sleep -Seconds 1}
  }
  if(!(Test-Path -LiteralPath (Join-Path $work 'owner-terminal-public.json')) -and $nodeIdentity){
   try{
    $held=Get-Process -Id $nodeIdentity.pid -ErrorAction Stop;[void]$held.Handle
    $f=[KernelProcessIdentity]::Read($nodeIdentity.pid)
    if($f.path -ine $plan.node -or $f.owner_sid -cne 'S-1-5-19' -or ([DateTime]$f.created_utc).ToUniversalTime().Ticks -ne ([DateTime]$nodeIdentity.created_utc).ToUniversalTime().Ticks -or $held.StartTime.ToUniversalTime().Ticks -ne ([DateTime]$f.created_utc).ToUniversalTime().Ticks){throw 'Fallback cleanup identity differs'}
    $held.Kill();if(!$held.WaitForExit(15000)){throw 'Fallback exit unconfirmed'}
    Save 'fallback-node-exit-public.json' @{pid=$held.Id;exit_code=$held.ExitCode;forced=$true;reason='owner_terminal_missing'}
   }catch{Save 'fallback-cleanup-error-public.json' @{error=$_.Exception.Message}}
   $success=$false
  }
  if($recorder -and !$recorder.HasExited){if(!$recorder.WaitForExit(15000)){$recorder.Kill();$recorder.WaitForExit();Save 'recorder-forced-exit-public.json' @{pid=$recorder.Id;exit_code=$recorder.ExitCode;trace_may_be_incomplete=$true};$success=$false}}
  try{Finish-Observer 'node_recording_finished'}catch{Save 'observer-cleanup-error-public.json' @{error=$_.Exception.Message};$success=$false}
  foreach($name in @('owner-terminal-public.json','owner-error-public.json','owner-cleanup-error-public.json','node-exit-public.json','node-resumed-public.json','liveness-public.json')){if(Test-Path -LiteralPath (Join-Path $work $name)){Copy-Item -LiteralPath (Join-Path $work $name) -Destination (Join-Path $out $name)}}
  if(Test-Path -LiteralPath $plan.progress){Copy-Item -LiteralPath $plan.progress -Destination (Join-Path $work 'final-progress-private.json')}
  $unchanged=$true;foreach($path in $protected){if((Get-FileHash -LiteralPath $path).Hash -cne $before[$path]){$unchanged=$false}}
  $t=Get-ScheduledTask -TaskName $taskName
  Save 'end-public.json' @{utc=[DateTime]::UtcNow.ToString('o');identity_network_config_unchanged=$unchanged;nodes=@(Get-CimInstance Win32_Process -Filter "Name='novovm-node.exe'"|Select-Object ProcessId,CreationDate);task_state=$t.State.ToString();task_exit=(Get-ScheduledTaskInfo -TaskName $taskName).LastTaskResult;relay_ready=$false;root_cause_fixed=$false}
  if($t.State -ne 'Running'){Unregister-ScheduledTask -TaskName $taskName -Confirm:$false}
 }
}
if($success){exit 0}else{exit 1}
