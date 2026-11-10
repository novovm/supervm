$ErrorActionPreference='Stop'
$base='D:\WEB3_AI\SUPERVM\target\c-w32time-sync-20261110-relaunch-123109'
$records=[Collections.Generic.List[object]]::new()
$start=[DateTimeOffset]::UtcNow.ToString('o')
$sw=[Diagnostics.Stopwatch]::StartNew()
$syncAttempted=$false; $syncExit=$null; $outcome='preflight'; $failure=$null
function Query([string]$Name,[string[]]$Arguments){
 $s=[DateTimeOffset]::UtcNow.ToString('o')
 & "$env:SystemRoot\System32\w32tm.exe" @Arguments 1> "$base/$Name.stdout.txt" 2> "$base/$Name.stderr.txt"
 $ec=$LASTEXITCODE
 $records.Add([ordered]@{name=$Name;command=('w32tm '+($Arguments -join ' '));start_utc=$s;end_utc=[DateTimeOffset]::UtcNow.ToString('o');exit_code=$ec})
 return $ec
}
function Snapshot([string]$Name){
 Get-Service W32Time | Select-Object Name,Status,StartType | ConvertTo-Json | Set-Content "$base/$Name-service.json"
 Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Services\W32Time\Parameters' | Select-Object Type,NtpServer | ConvertTo-Json | Set-Content "$base/$Name-parameters.json"
 Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Services\W32Time' | Select-Object Start | ConvertTo-Json | Set-Content "$base/$Name-start.json"
}
try {
 Snapshot 'before'
 $null=Query 'before-status' @('/query','/status','/verbose')
 $null=Query 'before-source' @('/query','/source')
 $null=Query 'before-configuration' @('/query','/configuration')
 $p=Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Services\W32Time\Parameters'
 if($p.Type -ne 'NTP' -or [string]::IsNullOrWhiteSpace($p.NtpServer)){throw 'No confirmed existing manual NTP source; no synchronization attempted'}
 $s=[DateTimeOffset]::UtcNow.ToString('o')
 $started=$false
 try { if((Get-Service W32Time).Status -ne 'Running'){Start-Service W32Time; $started=$true}; 'Start-Service check completed' | Set-Content "$base/start-service.stdout.txt"; $ec=0 }
 catch { $_ | Out-String | Set-Content "$base/start-service.stderr.txt"; $ec=1; throw }
 finally { $records.Add([ordered]@{name='start-service';command='Start-Service W32Time only if stopped';start_utc=$s;end_utc=[DateTimeOffset]::UtcNow.ToString('o');exit_code=$ec;exit_code_kind='PowerShell success/failure mapped 0/1';started=$started}) }
 $configurationExit=Query 'running-configuration' @('/query','/configuration')
 $null=Query 'running-source' @('/query','/source')
 $null=Query 'running-status' @('/query','/status','/verbose')
 if($configurationExit -ne 0){throw 'Existing effective configuration query failed; no synchronization attempted'}
 $syncAttempted=$true
 $syncExit=Query 'resync' @('/resync')
 $outcome=if($syncExit -eq 0){'resync_command_succeeded_verify_status'}else{'resync_failed_stop'}
} catch { $failure=$_.ToString(); $outcome='stopped_on_error'; $failure | Set-Content "$base/error.txt" }
finally {
 Snapshot 'after'
 $null=Query 'after-status' @('/query','/status','/verbose')
 $null=Query 'after-source' @('/query','/source')
 $null=Query 'after-configuration' @('/query','/configuration')
 [ordered]@{host_id='C';start_utc=$start;end_utc=[DateTimeOffset]::UtcNow.ToString('o');elapsed_monotonic_ms=$sw.ElapsedMilliseconds;outcome=$outcome;sync_attempted=$syncAttempted;sync_exit_code=$syncExit;commands=$records.ToArray();source_changed=$false;startup_type_changed=$false;manual_time_set=$false;node_operated=$false} | ConvertTo-Json -Depth 8 | Set-Content "$base/execution.json"
}