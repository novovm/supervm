#Requires -Version 7.0
$ErrorActionPreference='Stop'
$root=$PSScriptRoot
$checks=[Collections.Generic.List[object]]::new()
function Check([string]$name,[bool]$ok){$checks.Add(@{name=$name;pass=$ok});if(!$ok){throw "Preflight failed: $name"}}
$ownerPath=Join-Path $root 'FirstFaultWithRelayOwner-v4.ps1'
$launchPath=Join-Path $root 'Start-FirstFaultWithRelay-v4.ps1'
foreach($path in @($ownerPath,$launchPath)){
 $t=$null;$e=$null;[void][Management.Automation.Language.Parser]::ParseFile($path,[ref]$t,[ref]$e)
 Check ('AST '+[IO.Path]::GetFileName($path)) ($e.Count -eq 0)
}
$s=Get-Content -LiteralPath $ownerPath -Raw
$l=Get-Content -LiteralPath $launchPath -Raw
Check 'node122minutes' ($s.Contains("`$env:NOVOVM_NATIVE_EXECUTION_RUN_MS='7320000'"))
Check 'debugger124minutes' ($s.Contains("DebugOnce 'node' `$node '' 7440"))
Check 'original_progress_path' ($s.Contains("`$env:NOVOVM_NATIVE_EXECUTION_PIPELINE_PROGRESS_REPORT_PATH='C:\ProgramData\NOVOVM\selfrun\91129655606029\C\novonet-observation-a25452f4-20261112\logs\progress.json'"))
Check 'existing_only' ($s.Contains("`$env:NOVOVM_NATIVE_EXECUTION_START='existing'"))
Check 'correct_dump_paths' (([regex]::Matches($s,'\.dump /ma \$dumpCommandPath;')).Count -eq 2 -and !$s.Contains('.dump /ma $dump;'))
Check 'dump_completion_check' ($s.Contains('^Dump successfully written'))
Check 'dump_header_and_full_flag' ($s.Contains("-cne 'MDMP'") -and $s.Contains('ToUInt64($header,24) -band 2'))
Check 'memory_disk_bound' ($s.Contains('-gt 6GB') -and $s.Contains('-lt 10GB'))
Check 'original_observer_two_hours' ($l.Contains("'-MaximumWaitSeconds','7200'"))
Check 'archive_before_node_start' ($l.IndexOf('Progress archive hash mismatch') -lt $l.IndexOf('Start-ScheduledTask'))
Check 'fresh_progress_gate' ($l.Contains('LastWriteTimeUtc -gt $launchUtc'))
Check 'observer_pid_identity' ($l.Contains('$rawReady.pid -ne $nodes[0].ProcessId'))
Check 'bounded_observer_stop' ($l.Contains('$current.CommandLine.Contains((Join-Path $work ''observer''))') -and $l.Contains('$current.CreationDate.ToUniversalTime().ToString(''o'') -cne $observerCreation'))
Check 'diagnostic_not_relay_ready' ($l.Contains("status='C_FAULT_DIAGNOSTIC_RUNNING'") -and $l.Contains('A_must_not_send=$true'))
Check 'both_ifeo_rejected' ($l.Contains("foreach(`$image in 'novovm-node.exe','c-pageheap-control.exe')"))
Check 'old_diagnostic_untouched' ($l.Contains("'launch-public-02\result-public.json'") -and $l.Contains("'run-private-03'"))
$watch='D:\WEB3_AI\SUPERVM\artifacts\audit\novonet-single-relay-20261111\Watch-Relay.ps1'
$reader=Join-Path (Split-Path $watch) 'Read-RelayProgress.ps1'
Check 'original_watch_sha' ((Get-FileHash $watch).Hash -ceq 'D0DCE2979CB51C39EDF545637D276A80D56B921BFD194130842B3A475DF966D9')
Check 'original_reader_sha' ((Get-FileHash $reader).Hash -ceq 'CCC8F9592A9FB12D68531DC2ACE804EFB544B9921E4F43C51147F2DE57CBCD44')
# Exercise the actual v4 dump validator against the previous real fixture dump.
$t=$null;$e=$null;$ast=[Management.Automation.Language.Parser]::ParseFile($ownerPath,[ref]$t,[ref]$e)
$fn=$ast.Find({param($n) $n -is [Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq 'AssertPrivateDump'},$true)
. ([scriptblock]::Create($fn.Extent.Text))
$valid=AssertPrivateDump (Join-Path $root 'dump-path-control-01\control-private.dmp')
Check 'actual_full_memory_dump_validation' $valid.full_memory
$out=Join-Path $root 'relay-v4-preflight-public.json'
if(Test-Path -LiteralPath $out){throw 'Fresh test evidence required'}
@{utc=[DateTime]::UtcNow.ToString('o');checks=$checks.ToArray();node_started=$false;observer_started=$false;registry_changed=$false;live_pageheap_controls_pending=$true;root_cause_fixed=$false}|ConvertTo-Json -Depth 5|Set-Content -LiteralPath $out
"$($checks.Count) preparation checks passed; live controls remain mandatory"
