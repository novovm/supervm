#Requires -Version 7.0
#Requires -RunAsAdministrator
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$root=$PSScriptRoot
$pwsh='C:\Program Files\PowerShell\7\pwsh.exe'
$watch='D:\WEB3_AI\SUPERVM\artifacts\audit\novonet-single-relay-20261111\Watch-Relay.ps1'
$reader=Join-Path (Split-Path $watch) 'Read-RelayProgress.ps1'
$exe='C:\ProgramData\NOVOVM\releases\novonet-observation-a25452f4-windows-x86_64\bin\novovm-node.exe'
$work='C:\ProgramData\NOVOVM\selfrun\91129655606029\C\novonet-observation-a25452f4-20261112'
$nodeHash='1758e4a79dd34d1688d8f7bf4b60f46ed9ced518186f42fdb16baf04c0d0c46d'
function Save([string]$name,$data){
 $bytes=[Text.UTF8Encoding]::new($false).GetBytes(($data|ConvertTo-Json -Depth 15))
 $f=[IO.File]::Open((Join-Path $root $name),[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::Read)
 try{$f.Write($bytes,0,$bytes.Length);$f.Flush($true)}finally{$f.Dispose()}
}
if(Test-Path "$root/invoked-once.json"){throw 'Already invoked'}
foreach($pin in @(@($watch,'d0dce2979cb51c39edf545637d276a80d56b921bfd194130842b3a475df966d9'),@($reader,'ccc8f9592a9fb12d68531dc2ace804efb544b9921e4f43c51147f2de57cbcd44'))){if((Get-FileHash $pin[0]).Hash.ToLowerInvariant() -cne $pin[1]){throw 'Observer/reader pin differs'}}
Save 'invoked-once.json' @{host='C';utc=[DateTime]::UtcNow.ToString('o');handoff='16b78764';launcher_pid=$PID;maximum_wait_seconds=7200;node_mutated=$false;D_operated=$false;object_sent=$false}
try{
 $old=@(Get-CimInstance Win32_Process -Filter "Name='pwsh.exe'"|Where-Object {$_.CommandLine -like '*Watch-Relay.ps1*'})
 Save 'old-observer-check.json' @{utc=[DateTime]::UtcNow.ToString('o');matching_count=$old.Count;processes=@($old|Select-Object ProcessId,CreationDate,ExecutablePath,CommandLine);terminated=$false}
 if($old.Count){throw 'Same-purpose observer already present; do not overlap'}
 $nodes=@(Get-CimInstance Win32_Process -Filter "Name='novovm-node.exe'")
 $ports=@(Get-NetUDPEndpoint|Where-Object LocalPort -in 41052,42052)
 $taskName='NOVOVM-C-a25452f4-Existing-20261112'
 $task=Get-ScheduledTask -TaskName $taskName
 $info=Get-ScheduledTaskInfo -TaskName $taskName
 $exitRecord=if(Test-Path "$work/logs/exit.json"){Get-Content "$work/logs/exit.json" -Raw|ConvertFrom-Json}else{$null}
 $crashes=@(Get-WinEvent -FilterHashtable @{LogName='Application';Id=1000;StartTime=$info.LastRunTime} -ErrorAction SilentlyContinue|ForEach-Object{
  $f=@{};foreach($d in ([xml]$_.ToXml()).SelectNodes("/*[local-name()='Event']/*[local-name()='EventData']/*[local-name()='Data']")){$f[$d.GetAttribute('Name')]=[string]$d.InnerText}
  if($f['AppPath'] -ieq $exe){[ordered]@{record_id=$_.RecordId;utc=$_.TimeCreated.ToUniversalTime().ToString('o');app=$f['AppName'];path=$f['AppPath'];pid_raw=$f['ProcessId'];created_raw=$f['ProcessCreationTime'];module=$f['ModuleName'];exception=$f['ExceptionCode'];offset=$f['FaultingOffset']}}
 })
 Save 'node-before-public.json' @{utc=[DateTime]::UtcNow.ToString('o');nodes=@($nodes|Select-Object ProcessId,CreationDate,ExecutablePath);ports=@($ports|Select-Object LocalAddress,LocalPort,OwningProcess);task_name=$taskName;task_state=$task.State.ToString();task_result=[long]$info.LastTaskResult;last_run_utc=$info.LastRunTime.ToUniversalTime().ToString('o');node_sha256=(Get-FileHash $exe).Hash.ToLowerInvariant();original_exit=$exitRecord;crashes=$crashes;node_mutated=$false}
 if($nodes.Count -ne 1 -or $nodes[0].ExecutablePath -ine $exe -or (Invoke-CimMethod $nodes[0] -MethodName GetOwnerSid).Sid -cne 'S-1-5-19'){throw 'Current a254 LocalService node not confirmed; no restart or observer'}
 if((Get-FileHash $exe).Hash.ToLowerInvariant() -cne $nodeHash -or $ports.Count -ne 2 -or @($ports|Where-Object OwningProcess -NE $nodes[0].ProcessId).Count){throw 'Current a254 node hash/listeners differ'}
 $args=@('-NoProfile','-NonInteractive','-File',('"'+$watch+'"'),'-HostId','C','-Source','a25452f4c49be37cd984e650f2079642e6c273b3','-NodeSha256',$nodeHash,'-WorkName','novonet-observation-a25452f4-20261112','-EvidenceRoot',('"'+$root+'\observer"'),'-MaximumWaitSeconds','7200')
 $p=Start-Process -FilePath $pwsh -WindowStyle Hidden -ArgumentList $args -RedirectStandardOutput "$root/observer.stdout.log" -RedirectStandardError "$root/observer.stderr.log" -PassThru
 Save 'observer-started.json' @{host='C';pid=$p.Id;created_utc=$p.StartTime.ToUniversalTime().ToString('o');node_pid=$nodes[0].ProcessId;script=$watch;watch_sha256=(Get-FileHash $watch).Hash.ToLowerInvariant();reader_sha256=(Get-FileHash $reader).Hash.ToLowerInvariant();hidden=$true;maximum_wait_seconds=7200;node_mutated=$false;D_operated=$false;object_sent=$false}
 $p.WaitForExit();$code=$p.ExitCode
 Save 'observer-exit.json' @{utc=[DateTime]::UtcNow.ToString('o');pid=$p.Id;exit_code=$code}
 exit $code
}catch{Save 'launcher-error-public.json' @{host='C';utc=[DateTime]::UtcNow.ToString('o');status='C_RELAY_OBSERVER_NOT_ARMED';error=$_.Exception.Message;node_mutated=$false;retry_performed=$false;D_operated=$false;object_sent=$false};throw}
