#Requires -Version 7.0
#Requires -RunAsAdministrator
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$root=$PSScriptRoot
$settings=Get-Content -LiteralPath (Join-Path $root 'settings.json') -Raw|ConvertFrom-Json
function Save([string]$path,$value){
 $b=[Text.UTF8Encoding]::new($false).GetBytes(($value|ConvertTo-Json -Depth 12))
 $f=[IO.File]::Open($path,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
 try{$f.Write($b,0,$b.Length);$f.Flush($true)}finally{$f.Dispose()}
}
if ($settings.host -cne 'C' -or (Test-Path -LiteralPath $settings.evidence)) {throw 'Invalid host or consumed observer directory'}
if ((Get-FileHash -LiteralPath $settings.watch_script -Algorithm SHA256).Hash.ToLowerInvariant() -cne $settings.watch_sha256) {throw 'Original watcher hash changed'}
Save (Join-Path $root 'invoked-once.json') ([ordered]@{host='C';started_utc=[DateTime]::UtcNow.ToString('o');source=$settings.source;script_sha256=$settings.watch_sha256;observer_invocations=1;node_mutation_requested=$false;new_object_requested=$false;D_touched=$false})
try {
 $arguments=@('-NoProfile','-NonInteractive','-File',('"'+$settings.watch_script+'"'),'-HostId','C','-Source',$settings.source,'-NodeSha256',$settings.node_sha256,'-WorkName',$settings.work_name,'-EvidenceRoot',('"'+$settings.evidence+'"'))
 $p=Start-Process -FilePath 'C:\Program Files\PowerShell\7\pwsh.exe' -WindowStyle Hidden -ArgumentList $arguments -RedirectStandardOutput (Join-Path $root 'observer.stdout.log') -RedirectStandardError (Join-Path $root 'observer.stderr.log') -PassThru
 Save (Join-Path $root 'helper-started.json') ([ordered]@{host='C';helper_pid=$p.Id;helper_created_utc=$p.StartTime.ToUniversalTime().ToString('o');source=$settings.source;script=$settings.watch_script;script_sha256=$settings.watch_sha256;host_id='C';evidence=$settings.evidence;hidden=$true;maximum_wait_seconds=1800;stdout='observer.stdout.log';stderr='observer.stderr.log';node_mutated=$false;object_sent=$false})
 $p.WaitForExit()
 Save (Join-Path $root 'observer-exit.json') ([ordered]@{host='C';helper_pid=$p.Id;finished_utc=[DateTime]::UtcNow.ToString('o');exit_code=$p.ExitCode;node_mutated=$false;object_sent=$false})
 exit $p.ExitCode
} catch {
 Save (Join-Path $root 'launcher-error.json') ([ordered]@{host='C';time_utc=[DateTime]::UtcNow.ToString('o');error=$_.ToString();retry_executed=$false})
 throw
}