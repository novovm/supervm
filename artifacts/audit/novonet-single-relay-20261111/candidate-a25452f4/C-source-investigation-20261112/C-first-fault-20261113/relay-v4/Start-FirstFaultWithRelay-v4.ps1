#Requires -Version 7.0
#Requires -RunAsAdministrator
param()
$ErrorActionPreference='Stop';Set-StrictMode -Version Latest
$base='D:\WEB3_AI\SUPERVM\target\c-first-fault-20261113'
$work=Join-Path $base 'run-private-03'
$out=Join-Path $base 'launch-public-03'
$tool=Join-Path $base 'sdk\extracted\Windows Kits\10\Debuggers\x64'
$pwsh='C:\Program Files\PowerShell\7\pwsh.exe'
$taskName='NOVOVM-C-FirstFault-Relay-20261113-03'
$oldWork='C:\ProgramData\NOVOVM\selfrun\91129655606029\C\a254-aoem966-backtrace-20261113'
$originalWork='C:\ProgramData\NOVOVM\selfrun\91129655606029\C\novonet-observation-a25452f4-20261112'
$progress=Join-Path $originalWork 'logs\progress.json'
$watch='D:\WEB3_AI\SUPERVM\artifacts\audit\novonet-single-relay-20261111\Watch-Relay.ps1'
$reader=Join-Path (Split-Path $watch) 'Read-RelayProgress.ps1'
$ifeo='HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options'
$ownedFlags=[Collections.Generic.List[string]]::new()
$observer=$null;$observerCreation=$null;$observerExitSaved=$false;$terminal=$false
function Save($name,$value){
 $path=Join-Path $out $name
 if(Test-Path -LiteralPath $path){throw 'Public evidence exists'}
 $value|ConvertTo-Json -Depth 10|Set-Content -LiteralPath $path
}
function ClearOwnFlag([string]$image){
 if(!$ownedFlags.Contains($image)){return}
 & (Join-Path $tool 'gflags.exe') /p /disable $image >> (Join-Path $out 'gflags-public.log') 2>&1
 if($LASTEXITCODE -ne 0){throw "GFlags cleanup failed: $image"}
 $key=Join-Path $ifeo $image
 if(Test-Path -LiteralPath $key){
  $item=Get-Item -LiteralPath $key
  if($item.ValueCount -ne 0 -or $item.SubKeyCount -ne 0){throw 'Unexpected IFEO data; not removed'}
  Remove-Item -LiteralPath $key
 }
 [void]$ownedFlags.Remove($image)
}
function FinishOwnObserver([string]$reason){
 if($null -eq $observer -or $observerExitSaved){return}
 if(!$observer.HasExited){
  $current=Get-CimInstance Win32_Process -Filter "ProcessId=$($observer.Id)"
  if(!$current -or $current.Name -cne 'pwsh.exe' -or $current.ExecutablePath -ine $pwsh -or $current.CreationDate.ToUniversalTime().ToString('o') -cne $observerCreation -or $current.CommandLine -notlike '*Watch-Relay.ps1*' -or !$current.CommandLine.Contains((Join-Path $work 'observer'))){throw 'Observer identity changed; not terminated'}
  Stop-Process -Id $observer.Id -Force
  $observer.WaitForExit()
  $wasTerminated=$true
 }else{$wasTerminated=$false}
 Save 'observer-exit-public.json' @{utc=[DateTime]::UtcNow.ToString('o');pid=$observer.Id;exit_code=$observer.ExitCode;terminated_by_coordinator=$wasTerminated;reason=$reason;diagnostic_only=$true;relay_ready=$false}
 $script:observerExitSaved=$true
}
if(Test-Path -LiteralPath $out){throw 'No retry: public root exists'}
New-Item -ItemType Directory -Path $out|Out-Null
try{
 if(Test-Path -LiteralPath $work){throw 'No retry: private root exists'}
 if(Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue){throw 'Task already exists'}
 if(@(Get-CimInstance Win32_Process -Filter "Name='novovm-node.exe' OR Name='cdb.exe'").Count -or @(Get-NetUDPEndpoint|Where-Object LocalPort -in 41052,42052).Count){throw 'Node/debugger/C listeners already present'}
 if(@(Get-CimInstance Win32_Process -Filter "Name='pwsh.exe'" | Where-Object {$_.CommandLine -like '*Watch-Relay.ps1*'}).Count){throw 'Existing observer; preserve and stop preflight'}
 $prior=Get-Content (Join-Path $base 'launch-public-02\result-public.json') -Raw|ConvertFrom-Json
 $priorEnd=Get-Content (Join-Path $base 'launch-public-02\end-public.json') -Raw|ConvertFrom-Json
 $priorTask=Get-ScheduledTask -TaskName 'NOVOVM-C-FirstFault-20261113-02'
 if($prior.status -cne 'TARGET_EXIT' -or $priorTask.State -ne 'Ready' -or !$priorEnd.identity_and_network_unchanged -or @($priorEnd.nodes).Count){throw 'Previous diagnostic exit not confirmed'}
 foreach($image in 'novovm-node.exe','c-pageheap-control.exe'){if(Test-Path -LiteralPath (Join-Path $ifeo $image)){throw 'Existing IFEO settings; preserve and stop'}}
 foreach($binary in 'cdb.exe','gflags.exe','dbgeng.dll','dbghelp.dll'){
  $sig=Get-AuthenticodeSignature -LiteralPath (Join-Path $tool $binary)
  if($sig.Status -ne 'Valid' -or $sig.SignerCertificate.Subject -notmatch 'Microsoft Corporation'){throw "Debugger signature mismatch: $binary"}
 }
 foreach($pin in @(@($watch,'d0dce2979cb51c39edf545637d276a80d56b921bfd194130842b3a475df966d9'),@($reader,'ccc8f9592a9fb12d68531dc2ace804efb544b9921e4f43c51147f2de57cbcd44'),@((Join-Path $base 'c-pageheap-control.exe'),'3cde64eb56db67a2a7465a019c970b9f90f2f2ee5c3e068c40811237282c81d4'),@((Join-Path $tool 'c-pageheap-control.exe'),'3cde64eb56db67a2a7465a019c970b9f90f2f2ee5c3e068c40811237282c81d4'))){if((Get-FileHash -LiteralPath $pin[0]).Hash.ToLowerInvariant() -cne $pin[1]){throw 'Pinned observer/reader/control differs'}}
 $cfg=Get-Content (Join-Path $oldWork 'run.json') -Raw|ConvertFrom-Json
 if($cfg.database -cne 'C:\ProgramData\NOVOVM\selfrun\91129655606029\C\chain-state' -or !(Test-Path -LiteralPath $cfg.database -PathType Container)){throw 'Original DB mismatch'}
 $net=Get-Content -LiteralPath $cfg.novonet_config_file -Raw|ConvertFrom-Json
 $protected=@($cfg.signing_key_file,$cfg.transport_key_file,$net.carrier_identity_file,$cfg.novonet_config_file)
 $before=@{};foreach($path in $protected){$before[$path]=(Get-FileHash -LiteralPath $path).Hash}
 New-Item -ItemType Directory -Path $work|Out-Null
 $sid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
 & icacls.exe $work /inheritance:r /grant:r "*$($sid):(OI)(CI)F" '*S-1-5-18:(OI)(CI)F' '*S-1-5-32-544:(OI)(CI)F' '*S-1-5-19:(OI)(CI)M'|Out-Null
 if($LASTEXITCODE -ne 0){throw 'Private ACL failed'}
 Copy-Item -LiteralPath (Join-Path $oldWork 'run.json') -Destination (Join-Path $work 'run.json')
 Copy-Item -LiteralPath (Join-Path $base 'FirstFaultWithRelayOwner-v4.ps1') -Destination (Join-Path $work 'FirstFaultWithRelayOwner-v4.ps1')
 # Preserve exact old progress before the original observer path is reused.
 if(Test-Path -LiteralPath $progress){
  if((Get-Item -LiteralPath $progress).Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Unexpected progress reparse point'}
  $archive=Join-Path $work 'pre-existing-progress-private.json'
  Copy-Item -LiteralPath $progress -Destination $archive
  $hash=(Get-FileHash -LiteralPath $progress).Hash
  if((Get-FileHash -LiteralPath $archive).Hash -cne $hash){throw 'Progress archive hash mismatch'}
  Save 'prior-progress-archive-public.json' @{utc=[DateTime]::UtcNow.ToString('o');bytes=(Get-Item -LiteralPath $archive).Length;sha256=$hash.ToLowerInvariant();exact_local_archive=$true;raw_uploaded=$false}
 }else{Save 'prior-progress-archive-public.json' @{utc=[DateTime]::UtcNow.ToString('o');original_file_absent=$true;raw_uploaded=$false}}
 foreach($image in 'c-pageheap-control.exe','novovm-node.exe'){
  $ownedFlags.Add($image)
  & (Join-Path $tool 'gflags.exe') /p /enable $image /full >> (Join-Path $out 'gflags-public.log') 2>&1
  if($LASTEXITCODE -ne 0){throw 'Enable full PageHeap failed'}
 }
 Save 'preflight-public.json' @{utc=[DateTime]::UtcNow.ToString('o');start='existing';previous_owner_exited=$true;protected_identity_verified=$true;original_observer_unmodified=$true;observer_wait_seconds=7200;node_run_ms=7320000;debugger_hard_seconds=7440;task_max_minutes=128;memory_bound_bytes=6GB;minimum_disk_free_bytes=10GB;relay_ready=$false;A_B_D_operated=$false}
 $args='-NoProfile -NonInteractive -File "'+(Join-Path $work 'FirstFaultWithRelayOwner-v4.ps1')+'" -WorkRoot "'+$work+'" -ToolRoot "'+$tool+'"'
 $action=New-ScheduledTaskAction -Execute $pwsh -Argument $args
 $principal=New-ScheduledTaskPrincipal -UserId 'NT AUTHORITY\LOCAL SERVICE' -LogonType ServiceAccount
 $settings=New-ScheduledTaskSettingsSet -ExecutionTimeLimit (New-TimeSpan -Minutes 128) -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
 Register-ScheduledTask -TaskName $taskName -Action $action -Principal $principal -Settings $settings|Out-Null
 Start-ScheduledTask -TaskName $taskName
 $launchUtc=[DateTime]::UtcNow;$ready=$false
 while(([DateTime]::UtcNow-$launchUtc).TotalSeconds -lt 180){
  if(Test-Path (Join-Path $work 'error-public.json')){throw 'Diagnostic owner failed; preserve original error'}
  $log=Join-Path $work 'node-private.stdout'
  if(Test-Path -LiteralPath $log){
   $text=Get-Content -LiteralPath $log -Raw
   if($text -match '(?m)^C_DIAG_READY\r?$' -and $text -match '(?m)^Page heap: pid 0x[0-9a-fA-F]+: page heap enabled with flags 0x3\.'){
    if((Test-Path -LiteralPath $progress) -and (Get-Item -LiteralPath $progress).LastWriteTimeUtc -gt $launchUtc){$ready=$true;break}
   }
  }
  if(Test-Path (Join-Path $work 'result-public.json')){break}
  Start-Sleep -Seconds 1
 }
 foreach($image in @($ownedFlags.ToArray())){ClearOwnFlag $image}
 foreach($name in 'controls-public.json','control-dump-public.json'){Copy-Item -LiteralPath (Join-Path $work $name) -Destination (Join-Path $out $name)}
 if(!$ready){throw 'Node instrumentation/fresh original progress not verified'}
 $debug=Get-Content (Join-Path $work 'node-debugger.json') -Raw|ConvertFrom-Json
 $nodes=@(Get-CimInstance Win32_Process -Filter "Name='novovm-node.exe'" | Where-Object ParentProcessId -eq $debug.pid)
 if($nodes.Count -ne 1 -or (Invoke-CimMethod $nodes[0] -MethodName GetOwnerSid).Sid -cne 'S-1-5-19'){throw 'Diagnostic child identity mismatch'}
 $watchArgs=@('-NoProfile','-NonInteractive','-File',('"'+$watch+'"'),'-HostId','C','-Source','a25452f4c49be37cd984e650f2079642e6c273b3','-NodeSha256','1758e4a79dd34d1688d8f7bf4b60f46ed9ced518186f42fdb16baf04c0d0c46d','-WorkName','novonet-observation-a25452f4-20261112','-EvidenceRoot',('"'+(Join-Path $work 'observer')+'"'),'-MaximumWaitSeconds','7200')
 $observer=Start-Process -FilePath $pwsh -ArgumentList $watchArgs -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $work 'observer-private.stdout') -RedirectStandardError (Join-Path $work 'observer-private.stderr')
 $observerRecord=Get-CimInstance Win32_Process -Filter "ProcessId=$($observer.Id)"
 if(!$observerRecord -or $observerRecord.ExecutablePath -ine $pwsh){throw 'New observer process identity missing'}
 # Use the same WMI creation-time precision for both identity checks.
 $observerCreation=$observerRecord.CreationDate.ToUniversalTime().ToString('o')
 $wait=[DateTime]::UtcNow.AddSeconds(45);$observerReady=Join-Path $work 'observer\ready.json'
 while(!(Test-Path -LiteralPath $observerReady) -and [DateTime]::UtcNow -lt $wait){if($observer.HasExited){throw 'Original observer exited before readiness'};Start-Sleep -Milliseconds 500}
 if(!(Test-Path -LiteralPath $observerReady)){throw 'Original observer readiness missing'}
 $rawReady=Get-Content -LiteralPath $observerReady -Raw|ConvertFrom-Json
 if($rawReady.pid -ne $nodes[0].ProcessId -or $rawReady.observer_pid -ne $observer.Id){throw 'Observer/node PID mismatch'}
 Save 'diagnostic-ready-public.json' @{utc=[DateTime]::UtcNow.ToString('o');status='C_FAULT_DIAGNOSTIC_RUNNING';debugger=$debug;node_pid=$nodes[0].ProcessId;node_creation_utc=$nodes[0].CreationDate.ToUniversalTime().ToString('o');observer_pid=$observer.Id;observer_creation_utc=$observerCreation;original_observer_verified=$true;full_dump_control_verified=$true;original_progress_path_used=$true;observer_window_seconds=7200;temporary_ifeo_removed=$true;relay_ready=$false;A_must_not_send=$true}
 $deadline=$launchUtc.AddMinutes(127)
 while([DateTime]::UtcNow -lt $deadline){
  if(Test-Path (Join-Path $work 'result-public.json')){Copy-Item (Join-Path $work 'result-public.json') (Join-Path $out 'result-public.json');$terminal=$true;break}
  if(Test-Path (Join-Path $work 'error-public.json')){Copy-Item (Join-Path $work 'error-public.json') (Join-Path $out 'owner-error-public.json');$terminal=$true;break}
  if($observer.HasExited -and !$observerExitSaved){
   FinishOwnObserver 'observer_natural_exit'
   $ro=Join-Path $work 'observer'
   if(Test-Path (Join-Path $ro 'error.json')){throw 'Original observer failed; preserve then stop owned diagnostic'}
  }
  Start-Sleep -Seconds 5
 }
 if(!$terminal){throw 'Coordinator deadline reached without owner result'}
 FinishOwnObserver 'node_diagnostic_ended'
 if(Test-Path -LiteralPath $progress){Copy-Item -LiteralPath $progress -Destination (Join-Path $work 'final-progress-private.json')}
 foreach($name in 'node-dump-public.json','fault-observed-public.json'){if(Test-Path (Join-Path $work $name)){Copy-Item (Join-Path $work $name) (Join-Path $out $name)}}
 $unchanged=$true;foreach($path in $protected){if((Get-FileHash -LiteralPath $path).Hash -cne $before[$path]){$unchanged=$false}}
 Save 'end-public.json' @{utc=[DateTime]::UtcNow.ToString('o');identity_and_network_unchanged=$unchanged;nodes=@(Get-CimInstance Win32_Process -Filter "Name='novovm-node.exe'" | Select-Object ProcessId,ParentProcessId,CreationDate);task_state=(Get-ScheduledTask -TaskName $taskName).State.ToString();root_cause_fixed=$false;relay_ready=$false}
}catch{
 Save 'launch-error-public.json' @{utc=[DateTime]::UtcNow.ToString('o');error=$_.Exception.Message;relay_ready=$false;no_retry=$true}
 try{FinishOwnObserver 'diagnostic_error'}catch{Save 'observer-cleanup-error-public.json' @{utc=[DateTime]::UtcNow.ToString('o');error=$_.Exception.Message;relay_ready=$false}}
 if(Test-Path (Join-Path $work 'node-debugger.json')){
  $d=Get-Content (Join-Path $work 'node-debugger.json') -Raw|ConvertFrom-Json
  $p=Get-Process -Id $d.pid -ErrorAction SilentlyContinue
  if($p -and $p.Path -ceq $d.path -and $p.StartTime.ToUniversalTime().ToString('o') -ceq $d.created_utc){Stop-Process -Id $p.Id -Force}
 }
 exit 1
}finally{
 foreach($image in @($ownedFlags.ToArray())){ClearOwnFlag $image}
 Save 'registry-cleanup-public.json' @{utc=[DateTime]::UtcNow.ToString('o');node_ifeo_present=(Test-Path -LiteralPath (Join-Path $ifeo 'novovm-node.exe'));control_ifeo_present=(Test-Path -LiteralPath (Join-Path $ifeo 'c-pageheap-control.exe'))}
}
