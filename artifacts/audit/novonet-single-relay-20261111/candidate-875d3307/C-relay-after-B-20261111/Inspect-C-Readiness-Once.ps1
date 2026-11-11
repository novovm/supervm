#Requires -Version 7.0
#Requires -RunAsAdministrator
$ErrorActionPreference='Stop'
$root=$PSScriptRoot
$output=Join-Path $root 'readiness-admin-public.json'
if(Test-Path -LiteralPath $output){throw 'Evidence already exists; do not overwrite'}
$r=[ordered]@{host='C';started_utc=[DateTime]::UtcNow.ToString('o');read_only=$true;node_mutated=$false;observer_started=$false;object_sent=$false;D_touched=$false}
try {
 $r.processes=@(Get-CimInstance Win32_Process -Filter "Name='novovm-node.exe'" | ForEach-Object { $sid=Invoke-CimMethod $_ -MethodName GetOwnerSid; [ordered]@{pid=$_.ProcessId;created_utc=$_.CreationDate.ToUniversalTime().ToString('o');path=$_.ExecutablePath;owner_sid=$sid.Sid} })
}catch{$r.process_error=$_.Exception.Message}
try {
 $t=Get-ScheduledTask -TaskName 'NOVOVM-C-875d3307-Existing-20261111' -ErrorAction Stop
 $i=Get-ScheduledTaskInfo -TaskName $t.TaskName -ErrorAction Stop
 $r.task=[ordered]@{name=$t.TaskName;state=[string]$t.State;user=$t.Principal.UserId;last_run_utc=$i.LastRunTime.ToUniversalTime().ToString('o');last_task_result=$i.LastTaskResult;execution_time_limit=$t.Settings.ExecutionTimeLimit}
}catch{$r.task_error=$_.Exception.Message}
try{$r.udp=@(Get-NetUDPEndpoint -ErrorAction Stop|Where-Object LocalPort -in 41052,42052|Select-Object LocalAddress,LocalPort,OwningProcess)}catch{$r.udp_error=$_.Exception.Message}
$base='C:\ProgramData\NOVOVM\selfrun\91129655606029\C\novonet-observation-875d3307-20261111\logs'
foreach($name in @('progress.json','exit.json')){
 try{
  $p=Join-Path $base $name
  $f=Get-Item -LiteralPath $p -ErrorAction Stop
  $meta=[ordered]@{exists=$true;length=$f.Length;modified_utc=$f.LastWriteTimeUtc.ToString('o');sha256=(Get-FileHash -LiteralPath $p).Hash.ToLowerInvariant()}
  if($name -eq 'exit.json'){
   if($f.Length -gt 1048576){throw 'Exit summary too large'}
   $j=Get-Content -LiteralPath $p -Raw|ConvertFrom-Json
   $fields=[ordered]@{}
   foreach($key in @('exit_code','started_utc','finished_utc','ended_utc','pid')){if($j.PSObject.Properties.Name -contains $key){$fields[$key]=$j.$key}}
   $meta.summary=$fields
  }
  $r[$name]=$meta
 }catch{$r[$name]=[ordered]@{error=$_.Exception.Message}}
}
$r.finished_utc=[DateTime]::UtcNow.ToString('o')
$r.status='C_NOT_READY_NO_VERIFIED_OWNER'
$r|ConvertTo-Json -Depth 8|Set-Content -LiteralPath $output -Encoding utf8
exit 0
