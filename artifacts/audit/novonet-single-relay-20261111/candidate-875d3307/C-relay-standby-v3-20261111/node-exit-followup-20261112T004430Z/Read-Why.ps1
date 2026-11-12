#Requires -Version 7.0
#Requires -RunAsAdministrator
$ErrorActionPreference='Stop'
$r=[ordered]@{host='C';utc=[DateTime]::UtcNow.ToString('o');read_only=$true}
$r.last_boot_utc=(Get-CimInstance Win32_OperatingSystem).LastBootUpTime.ToUniversalTime().ToString('o')
try{$t=Get-ScheduledTask -TaskName 'NOVOVM-C-875d3307-Existing-20261111';$i=Get-ScheduledTaskInfo -TaskName $t.TaskName;$r.task=[ordered]@{state=[string]$t.State;last_run_utc=$i.LastRunTime.ToUniversalTime().ToString('o');result=$i.LastTaskResult}}catch{$r.task_error=$_.Exception.Message}
$p='C:\ProgramData\NOVOVM\selfrun\91129655606029\C\novonet-observation-875d3307-20261111\logs\exit.json'
try{$j=Get-Content -LiteralPath $p -Raw|ConvertFrom-Json;$r.exit=[ordered]@{exit_code=$j.exit_code;start=$j.start;time=$j.time}}catch{$r.exit_error=$_.Exception.Message}
try{
$events=@(Get-WinEvent -FilterHashtable @{LogName='Application';Id=1000;StartTime=[DateTime]::Now.AddDays(-1)} -MaxEvents 128)
$r.crashes=@(foreach($ev in $events){$xml=[xml]$ev.ToXml();$data=@{};foreach($n in $xml.SelectNodes("/*[local-name()='Event']/*[local-name()='EventData']/*[local-name()='Data']")){$data[$n.GetAttribute('Name')]=[string]$n.InnerText};if($data['AppName'] -ieq 'novovm-node.exe'){[ordered]@{utc=$ev.TimeCreated.ToUniversalTime().ToString('o');record=$ev.RecordId;app=$data['AppName'];module=$data['ModuleName'];exception=$data['ExceptionCode'];offset=$data['FaultingOffset'];pid_raw=$data['ProcessId'];path=$data['AppPath']}}})
}catch{$r.events_error=$_.Exception.Message}
$r|ConvertTo-Json -Depth 6|Set-Content -LiteralPath (Join-Path $PSScriptRoot 'public.json') -Encoding utf8
