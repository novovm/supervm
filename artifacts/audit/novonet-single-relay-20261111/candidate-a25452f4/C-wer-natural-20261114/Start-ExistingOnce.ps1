#Requires -Version 7.0
#Requires -RunAsAdministrator
param([Parameter(Mandatory)][string]$EvidenceRoot)
$ErrorActionPreference='Stop'
if(Test-Path -LiteralPath $EvidenceRoot){throw 'Use new EvidenceRoot; never overwrite'}
[IO.Directory]::CreateDirectory($EvidenceRoot)|Out-Null
$utf8=[Text.UTF8Encoding]::new($false)
function Save($name,$v){[IO.File]::WriteAllText((Join-Path $EvidenceRoot $name),($v|ConvertTo-Json -Depth 16)+"`n",$utf8)}
function Hash($p){(Get-FileHash -LiteralPath $p -Algorithm SHA256).Hash.ToLowerInvariant()}
function PrivateAcl($p){
 $a=Get-Acl -LiteralPath $p
 foreach($r in $a.Access){$s=$r.IdentityReference.Translate([Security.Principal.SecurityIdentifier]).Value;if($r.AccessControlType -eq 'Allow' -and $s -notin @('S-1-5-18','S-1-5-19','S-1-5-32-544','S-1-3-0')){throw 'Unexpected service Profile ACL'}}
 return $a.Sddl
}
$old='C:\ProgramData\NOVOVM\selfrun\91129655606029\C\a254-aoem966-backtrace-20261113'
$node='C:\ProgramData\NOVOVM\releases\novonet-observation-a25452f4-windows-x86_64\bin\novovm-node.exe'
$core='C:\ProgramData\NOVOVM\releases\aoem-9662470d-c-20261113\aoem\windows\core\bin\aoem_ffi.dll'
$nodeHash='1758e4a79dd34d1688d8f7bf4b60f46ed9ced518186f42fdb16baf04c0d0c46d'
$coreHash='5743f5875131330f8c8fec58cdc2a85aa39dc6e9689358609a5f0a09a01c0d52'
$stamp=[DateTime]::UtcNow.ToString('yyyyMMddTHHmmssZ')
$work='C:\Windows\ServiceProfiles\LocalService\AppData\Local\NOVOVM-WER-Natural-C-'+$stamp
$taskName='NOVOVM-C-WER-Natural-'+$stamp
$started=$false
Save 'started.json' @{utc=[DateTime]::UtcNow.ToString('o');launcher_pid=$PID;existing_only=$true;maximum_node_run_seconds=21600;restart_count=0}
try {
 if(@(Get-CimInstance Win32_Process -Filter "Name='novovm-node.exe'").Count){throw 'Node already present; no start'}
 if(@(Get-NetUDPEndpoint|Where-Object LocalPort -in 41052,42052).Count){throw 'C listener already present'}
 $oldObservers=@(Get-CimInstance Win32_Process -Filter "Name='pwsh.exe' OR Name='powershell.exe'"|Where-Object {$_.ProcessId -ne $PID -and $_.CommandLine -match 'Watch-Relay.ps1'})
 if($oldObservers.Count){throw 'Old relay observer still present; no start'}
 if((Hash $node) -cne $nodeHash -or (Hash $core) -cne $coreHash){throw 'Fixed image/runtime hash differs'}
 $cfgPath=Join-Path $old 'run.json';$cfg=Get-Content $cfgPath -Raw|ConvertFrom-Json
 if($cfg.database -cne 'C:\ProgramData\NOVOVM\selfrun\91129655606029\C\chain-state' -or $cfg.library -cne $core -or !(Test-Path -LiteralPath $cfg.database -PathType Container)){throw 'Original existing database/config mismatch'}
 $net=Get-Content -LiteralPath $cfg.novonet_config_file -Raw|ConvertFrom-Json
 if((Hash $cfg.novonet_config_file) -cne 'a41b09211b2e45e4fbf5ab5e7b6a9d3b9e7f35180a34cfffcabe7d5d39369230'){throw 'Original network config differs'}
 if((Hash $cfg.signing_key_file) -cne '8d9b8bb2150496e79e50fc7d3127bffce06f25dcf7ee7f956fb7b54cfc382b9e' -or (Hash $cfg.transport_key_file) -cne '5894e458fcde653e3b0e97a0390d849edc2efa501295964ac77a0c50ab9dd956' -or (Hash $net.carrier_identity_file) -cne 'fbbea2280acc1bc564dfa6f81727f09cbc583a5e79030534e1a9a86a349e6713'){throw 'Original protected identity differs'}
 if($net.probe_control_file -and (Test-Path -LiteralPath $net.probe_control_file)){throw 'Existing probe command file: preserve and inspect before any start'}
 $werKey='HKLM:\SOFTWARE\Microsoft\Windows\Windows Error Reporting\LocalDumps\novovm-node.exe'
 $wer=Get-ItemProperty -LiteralPath $werKey
 if($wer.DumpType -ne 2 -or $wer.DumpCount -ne 2 -or $wer.DumpFolder -cne 'C:\Windows\ServiceProfiles\LocalService\AppData\Local\CrashDumps\NOVOVM-Full-C-20261114T074716Z'){throw 'Qualified WER configuration changed'}
 $dumpFolder=$wer.DumpFolder;PrivateAcl $dumpFolder|Out-Null
 if(@(Get-ChildItem -LiteralPath $dumpFolder -Filter '*.dmp' -File).Count){throw 'Dump folder not empty; archive first, do not risk overwrite'}
 foreach($path in @('HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\AeDebug','HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows NT\CurrentVersion\AeDebug')){
  if((Get-ItemProperty -LiteralPath $path).Auto -eq '1'){throw 'Automatic debugger enabled'}
 }
 if(Test-Path 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options\novovm-node.exe'){throw 'IFEO present; do not run with injected diagnostics'}
 $disk=[IO.DriveInfo]::new('C:\');if($disk.AvailableFreeSpace -lt 18GB){throw 'Less than two 8 GiB dump budgets plus 2 GiB reserve'}
 if(Test-Path $work){throw 'Work already exists'}
 [IO.Directory]::CreateDirectory($work)|Out-Null;[IO.Directory]::CreateDirectory((Join-Path $work 'logs'))|Out-Null
 PrivateAcl $work|Out-Null
 Copy-Item -LiteralPath $cfgPath -Destination (Join-Path $work 'run.json')
 if((Hash $cfgPath) -cne (Hash (Join-Path $work 'run.json'))){throw 'Configuration copy differs'}
 $ownerPath=Join-Path $old 'ExistingOwner.ps1';$owner=Get-Content -LiteralPath $ownerPath -Raw
 if($owner -match 'TTD|cdb\.exe|PageHeap'){throw 'Unexpected original owner action'}
 if(!$owner.Contains($old) -or !$owner.Contains("`$env:NOVOVM_NATIVE_EXECUTION_START='existing'") -or !$owner.Contains("`$env:NOVOVM_NATIVE_EXECUTION_RUN_MS='86400000'")){throw 'Original owner template differs'}
 $owner=$owner.Replace($old,$work).Replace("`$env:NOVOVM_NATIVE_EXECUTION_RUN_MS='86400000'","`$env:NOVOVM_NATIVE_EXECUTION_RUN_MS='21600000'")
 # Existing owner includes initialize_executed=false only; no initializer call is permitted.
 [IO.File]::WriteAllText((Join-Path $work 'ExistingOwner.ps1'),$owner,$utf8)
 $dbManifest=@(Get-ChildItem -LiteralPath $cfg.database -File -Recurse|ForEach-Object {@{relative=[IO.Path]::GetRelativePath($cfg.database,$_.FullName);bytes=$_.Length;last_write_utc=$_.LastWriteTimeUtc.ToString('o');sha256=Hash $_.FullName}})
 [IO.File]::WriteAllText((Join-Path $work 'database-before-private.json'),($dbManifest|ConvertTo-Json -Depth 6),$utf8)
 Save 'preflight-public.json' @{utc=[DateTime]::UtcNow.ToString('o');node_sha256=$nodeHash;aoem_sha256=$coreHash;config_sha256=Hash $cfgPath;identity_matches_original=$true;database_existing=$true;database_file_count=$dbManifest.Count;database_inventory_sha256=Hash (Join-Path $work 'database-before-private.json');wer_dump_type=2;dump_count=2;disk_free_bytes=$disk.AvailableFreeSpace;dump_folder=$dumpFolder;original_owner_sha256=Hash $ownerPath;new_owner_sha256=Hash (Join-Path $work 'ExistingOwner.ps1');maximum_run_seconds=21600;original_logs_preserved=$true;no_probe_command=$true}
 $args='-NoProfile -NonInteractive -File "'+(Join-Path $work 'ExistingOwner.ps1')+'"'
 $action=New-ScheduledTaskAction -Execute 'C:\Program Files\PowerShell\7\pwsh.exe' -Argument $args -WorkingDirectory $work
 $principal=New-ScheduledTaskPrincipal -UserId 'NT AUTHORITY\LOCAL SERVICE' -LogonType ServiceAccount
 # Normal owner runtime is six hours. Extra allowance permits WER to finish before task cleanup.
 $settings=New-ScheduledTaskSettingsSet -Hidden -ExecutionTimeLimit (New-TimeSpan -Hours 7) -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
 Register-ScheduledTask -TaskName $taskName -Action $action -Principal $principal -Settings $settings|Out-Null
 Start-ScheduledTask -TaskName $taskName;$started=$true
 Save 'start-requested.json' @{utc=[DateTime]::UtcNow.ToString('o');task=$taskName;work=$work;start='existing';restart_count=0;double_relay_test_started=$false}
 $watch=[Diagnostics.Stopwatch]::StartNew();$moduleChecked=$false;$nodePid=$null;$budgetStop=$false
 while($watch.Elapsed.TotalSeconds -lt 22200){
  $nodes=@(Get-CimInstance Win32_Process -Filter "Name='novovm-node.exe'")
  if($nodes.Count -gt 1){throw 'Unexpected extra node; preserve, do not terminate'}
  $proc=if($nodes.Count){$nodes[0]}else{$null}
  if($proc){
   if($proc.ExecutablePath -cne $node){throw 'Node executable changed'}
   if($null -eq $nodePid){$nodePid=[int]$proc.ProcessId}elseif($nodePid -ne $proc.ProcessId){throw 'Unexpected PID replacement; no retry'}
   $liveProcess=Get-Process -Id $nodePid
   $privateBytes=$liveProcess.PrivateMemorySize64
   $freeBytes=([IO.DriveInfo]::new('C:\')).AvailableFreeSpace
   if($privateBytes -gt 6GB -or $freeBytes -lt (2*($privateBytes+1GB)+2GB)){
    Save 'resource-stop.json' @{utc=[DateTime]::UtcNow.ToString('o');pid=$nodePid;private_bytes=$privateBytes;free_bytes=$freeBytes;reason='Bounded memory/disk safety limit; NOT natural crash';no_retry=$true}
    if($liveProcess.Path -cne $node -or $liveProcess.StartTime.ToUniversalTime() -ne $proc.CreationDate.ToUniversalTime()){throw 'Process identity changed; do not terminate'}
    Stop-Process -InputObject $liveProcess -ErrorAction Stop
    $budgetStop=$true
   }
   if(!$moduleChecked){
    $module=@(Get-Process -Id $nodePid -Module|Where-Object ModuleName -eq 'aoem_ffi.dll')
    if($module.Count){if((Hash $module[0].FileName) -cne $coreHash){throw 'Loaded AOEM differs'};$moduleChecked=$true
     Save 'running-public.json' @{utc=[DateTime]::UtcNow.ToString('o');pid=$nodePid;created_utc=$proc.CreationDate.ToUniversalTime().ToString('o');node_sha256=$nodeHash;loaded_aoem_sha256=$coreHash;task=$taskName;start='existing';owner_sid='S-1-5-19';planned_latest_normal_exit_utc=$proc.CreationDate.ToUniversalTime().AddHours(6).ToString('o')}
    }
   }
  }
  $logs=Join-Path $work 'logs';$progress=$null;$progressError=$null
  if(Test-Path (Join-Path $logs 'progress.json')){try{$p=Get-Content (Join-Path $logs 'progress.json') -Raw|ConvertFrom-Json;$progress=@{chain_id=$p.chain_id;current_height=$p.current_height;head=$p.head;recovery_in_progress=$p.recovery_in_progress;execution_failures=$p.execution_failures}}catch{$progressError='progress read incomplete'}}
  Save 'liveness.json' @{utc=[DateTime]::UtcNow.ToString('o');node_pid=$nodePid;alive=($null -ne $proc);elapsed_seconds=$watch.Elapsed.TotalSeconds;loaded_aoem_verified=$moduleChecked;progress=$progress;progress_read_error=$progressError;stderr_bytes=if(Test-Path (Join-Path $logs 'node.stderr')){(Get-Item (Join-Path $logs 'node.stderr')).Length}else{0};dump_files=@(Get-ChildItem -LiteralPath $dumpFolder -Filter '*.dmp' -File|Select-Object Name,Length);free_disk_bytes=([IO.DriveInfo]::new('C:\')).AvailableFreeSpace}
  if(Test-Path (Join-Path $logs 'exit.json')){break}
  if(Test-Path (Join-Path $logs 'owner-error.json')){throw 'Owner failed; private original log retained'}
  if(!$proc -and $watch.Elapsed.TotalSeconds -gt 90){throw 'Node absent without owner exit; no retry'}
  Start-Sleep -Seconds 15
 }
 $exitPath=Join-Path $work 'logs\exit.json'
 if(!(Test-Path $exitPath)){throw 'Observation ended without owner exit; do not restart or claim success'}
 $exit=Get-Content $exitPath -Raw|ConvertFrom-Json;Save 'node-exit.json' $exit
 Save 'task-result.json' (Get-ScheduledTaskInfo -TaskName $taskName|Select-Object LastTaskResult,LastRunTime)
 $dumps=@(Get-ChildItem -LiteralPath $dumpFolder -Filter ("novovm-node.exe."+$nodePid+'.dmp') -File)
 $dumpRecords=@()
 foreach($d in $dumps){
  $hash=Hash $d.FullName;Start-Sleep -Seconds 2;if($hash -cne (Hash $d.FullName)){throw 'Dump still changing; preserve'}
  $fs=[IO.File]::OpenRead($d.FullName);try{$br=[IO.BinaryReader]::new($fs);if($br.ReadUInt32() -ne 0x504d444d){throw 'Invalid dump'};$fs.Position=24;$flags=$br.ReadUInt64()}finally{$fs.Dispose()}
  $dumpRecords+=@{name=$d.Name;bytes=$d.Length;sha256=$hash;flags=('0x{0:x}' -f $flags);full_memory_flag=(($flags -band 2) -ne 0);path=$d.FullName}
 }
 Save 'dump-inventory-public.json' @{pid=$nodePid;dumps=$dumpRecords;no_restart=$true;root_cause_resolved=$false}
 Save 'result-public.json' @{utc=[DateTime]::UtcNow.ToString('o');status='NODE_EXITED';resource_limit_stop=$budgetStop;node_pid=$nodePid;node_exit_code=$exit.exit_code;dumps_found=$dumps.Count;task=$taskName;private_work=$work;node_started_once=$true;double_relay_started=$false;root_cause_resolved=$false}
} catch {
 Save 'error-public.json' @{utc=[DateTime]::UtcNow.ToString('o');error=$_.Exception.Message;node_start_requested=$started;no_retry=$true;private_work=$work}
 exit 1
}
