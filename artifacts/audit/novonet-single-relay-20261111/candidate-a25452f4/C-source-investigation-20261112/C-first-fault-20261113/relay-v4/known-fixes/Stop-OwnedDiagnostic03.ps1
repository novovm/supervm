#Requires -Version 7.0
#Requires -RunAsAdministrator
$ErrorActionPreference='Stop';Set-StrictMode -Version Latest
$base='D:\WEB3_AI\SUPERVM\target\c-first-fault-20261113'
$work=Join-Path $base 'run-private-03'
$out=Join-Path $base 'stop-owned-03-public'
if(Test-Path -LiteralPath $out){throw 'One-shot stop evidence already exists'}
New-Item -ItemType Directory -Path $out|Out-Null
function Save($name,$data){$p=Join-Path $out $name;if(Test-Path -LiteralPath $p){throw 'Evidence already exists'};$data|ConvertTo-Json -Depth 8|Set-Content -LiteralPath $p}
try{
 $d=Get-Content (Join-Path $work 'node-debugger.json') -Raw|ConvertFrom-Json
 $ready=Get-Content (Join-Path $base 'launch-public-03\diagnostic-ready-public.json') -Raw|ConvertFrom-Json
 $failed=Get-Content (Join-Path $work 'observer\error.json') -Raw|ConvertFrom-Json
 if($d.pid -ne 12524 -or $ready.node_pid -ne 17748 -or $ready.observer_pid -ne 37152 -or $failed.error -cne 'No complete diagnostic snapshot within observation bound: missing_novonet'){throw 'Original diagnostic/error context differs'}
 $debug=Get-Process -Id $d.pid -ErrorAction SilentlyContinue
 $node=Get-Process -Id $ready.node_pid -ErrorAction SilentlyContinue
 if(!$debug){Save 'already-exited-public.json' @{utc=[DateTime]::UtcNow.ToString('o');debugger_absent=$true;node_present=($null -ne $node);nothing_stopped=$true};exit 0}
 # ConvertFrom-Json may yield DateTime; compare normalized ticks, never ISO text to DateTime.
 if([IO.Path]::GetFullPath($debug.Path) -ine [IO.Path]::GetFullPath($d.path) -or $debug.StartTime.ToUniversalTime().Ticks -ne ([DateTime]$d.created_utc).ToUniversalTime().Ticks){throw 'Debugger PID/path/creation mismatch; nothing stopped'}
 $debugRow=Get-CimInstance Win32_Process -Filter "ProcessId=$($d.pid)"
 if(!$debugRow -or !$debugRow.CommandLine.Contains((Join-Path $work 'node.cdb')) -or (Invoke-CimMethod $debugRow -MethodName GetOwnerSid).Sid -cne 'S-1-5-19'){throw 'Debugger command/owner mismatch; nothing stopped'}
 if($node){
  $nodeRow=Get-CimInstance Win32_Process -Filter "ProcessId=$($ready.node_pid)"
  if(!$nodeRow -or $nodeRow.ParentProcessId -ne $d.pid -or $nodeRow.CreationDate.ToUniversalTime().Ticks -ne ([DateTime]$ready.node_creation_utc).ToUniversalTime().Ticks -or $nodeRow.ExecutablePath -ine 'C:\ProgramData\NOVOVM\releases\novonet-observation-a25452f4-windows-x86_64\bin\novovm-node.exe' -or (Invoke-CimMethod $nodeRow -MethodName GetOwnerSid).Sid -cne 'S-1-5-19'){throw 'Owned node identity mismatch; nothing stopped'}
  # Open and retain kernel handles before stopping the debugger.
  [void]$node.Handle
 }
 [void]$debug.Handle
 Save 'before-public.json' @{utc=[DateTime]::UtcNow.ToString('o');debugger_pid=$debug.Id;node_pid=if($node){$node.Id}else{$null};identity_checks='PID/path/creation/command/LocalService';reason='original_observer_failed_missing_novonet';node_restarted=$false;A_B_D_operated=$false}
 Stop-Process -Id $debug.Id -Force
 if(!$debug.WaitForExit(20000)){throw 'Debugger exit not confirmed'}
 $nodeExited=if($node){$node.WaitForExit(20000)}else{$true}
 Save 'exit-public.json' @{utc=[DateTime]::UtcNow.ToString('o');debugger_exit_code=$debug.ExitCode;node_exited=$nodeExited;actual_node_exit_code=if($node -and $nodeExited){$node.ExitCode}else{$null};controlled_debugger_termination=$true;natural_node_crash_not_inferred=$true;nodes=@(Get-CimInstance Win32_Process -Filter "Name='novovm-node.exe'"|Select-Object ProcessId,ParentProcessId,CreationDate);root_cause_fixed=$false;relay_ready=$false}
 if(Test-Path (Join-Path $work 'final-progress-private.json')){throw 'Final progress already archived'}
 Copy-Item -LiteralPath 'C:\ProgramData\NOVOVM\selfrun\91129655606029\C\novonet-observation-a25452f4-20261112\logs\progress.json' -Destination (Join-Path $work 'final-progress-private.json')
 Save 'archive-public.json' @{utc=[DateTime]::UtcNow.ToString('o');final_progress_sha256=(Get-FileHash (Join-Path $work 'final-progress-private.json')).Hash.ToLowerInvariant();final_progress_bytes=(Get-Item (Join-Path $work 'final-progress-private.json')).Length;raw_uploaded=$false;original_database_reinitialized=$false}
 if(!$nodeExited){throw 'Owned node exit not confirmed; preserve and stop'}
}catch{Save 'error-public.json' @{utc=[DateTime]::UtcNow.ToString('o');error=$_.Exception.Message;retry=$false};exit 1}
