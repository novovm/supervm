#Requires -Version 7.0
#Requires -RunAsAdministrator
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$root=$PSScriptRoot
$repo='D:\WEB3_AI\SUPERVM'
$pwsh='C:\Program Files\PowerShell\7\pwsh.exe'
$watch=Join-Path $repo 'artifacts\audit\novonet-single-relay-20261111\Watch-Relay.ps1'
$reader=Join-Path (Split-Path $watch) 'Read-RelayProgress.ps1'
$oldRoot=Join-Path $repo 'target\c-current875-recovery-20261111T151030Z'
$exe='C:\ProgramData\NOVOVM\releases\novonet-observation-875d3307-windows-x86_64\bin\novovm-node.exe'
function Save([string]$name,$data){
 $b=[Text.UTF8Encoding]::new($false).GetBytes(($data|ConvertTo-Json -Depth 12))
 $f=[IO.File]::Open((Join-Path $root $name),[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::Read)
 try{$f.Write($b,0,$b.Length);$f.Flush($true)}finally{$f.Dispose()}
}
if(Test-Path -LiteralPath (Join-Path $root 'invoked-once.json')){throw 'Already invoked'}
foreach($v in @(@($watch,'d0dce2979cb51c39edf545637d276a80d56b921bfd194130842b3a475df966d9'),@($reader,'ccc8f9592a9fb12d68531dc2ace804efb544b9921e4f43c51147f2de57cbcd44'))){if((Get-FileHash -LiteralPath $v[0]).Hash.ToLowerInvariant() -cne $v[1]){throw 'Observer source changed'}}
Save 'invoked-once.json' ([ordered]@{host='C';utc=[DateTime]::UtcNow.ToString('o');handoff='c0a1dfb2';launcher_pid=$PID;maximum_wait_seconds=7200;maximum_snapshots=64;D_touched=$false;node_mutation_requested=$false;object_sent=$false})
try{
 $old=Get-Content -LiteralPath "$oldRoot/observer-started.json" -Raw|ConvertFrom-Json
 if($old.pid -ne 21548 -or $old.node_pid -ne 3228 -or $old.script -cne $watch){throw 'Previous observer receipt differs'}
 $prior=Get-CimInstance Win32_Process -Filter 'ProcessId=21548'
 $check=[ordered]@{host='C';utc=[DateTime]::UtcNow.ToString('o');old_pid=21548;old_created_utc=$old.created_utc;process_present=($null -ne $prior);termination_requested=$false;node_stopped=$false}
 if($null -ne $prior){
  $same=[Math]::Abs(($prior.CreationDate.ToUniversalTime()-[DateTime]$old.created_utc).TotalMilliseconds) -le 1
  $check.creation_matches=$same
  if($same){
   $tail='-NoProfile -NonInteractive -File "'+$watch+'" -HostId C -Source 875d3307a1fc2ca34b45464ac3bf51a03619adc2 -NodeSha256 9174f29bad4bf1feb9acfbd5d2b5ed36bb47ff2a2cd1519d7742a2e1b944dee7 -WorkName novonet-observation-875d3307-20261111 -EvidenceRoot "'+$oldRoot+'\observer"'
   if($prior.ExecutablePath -ine $pwsh -or $prior.CommandLine.Trim() -cnotin @(('"'+$pwsh+'" '+$tail),($pwsh+' '+$tail))){Save 'old-observer-check.json' $check;throw 'Old observer exact command differs; nothing terminated'}
   $handle=Get-Process -Id 21548 -ErrorAction Stop
   try{
    $null=$handle.Handle
    if([Math]::Abs(($handle.StartTime.ToUniversalTime()-[DateTime]$old.created_utc).TotalMilliseconds) -gt 1){throw 'Process identity changed'}
    if(!$handle.HasExited){$check.termination_requested=$true;$handle.Kill();if(!$handle.WaitForExit(10000)){throw 'Old observer did not exit'}}
    $check.kernel_exit_code=$handle.ExitCode
   }finally{$handle.Dispose()}
  }else{$check.pid_reused_unrelated_not_touched=$true}
 }else{
  $oldExit=Get-Content -LiteralPath "$oldRoot/observer-exit.json" -Raw|ConvertFrom-Json
  if($oldExit.pid -ne 21548){throw 'Previous exit PID differs'}
  $check.original_exit=$oldExit
  $check.original_result=Get-Content -LiteralPath "$oldRoot/observer/result.json" -Raw|ConvertFrom-Json
  $check.original_final_liveness=Get-Content -LiteralPath "$oldRoot/observer/liveness.json" -Raw|ConvertFrom-Json
 }
 Save 'old-observer-check.json' $check
 $nodes=@(Get-CimInstance Win32_Process -Filter "Name='novovm-node.exe'")
 if($nodes.Count -ne 1 -or $nodes[0].ProcessId -ne 3228 -or $nodes[0].ExecutablePath -ine $exe -or (Invoke-CimMethod $nodes[0] -MethodName GetOwnerSid).Sid -cne 'S-1-5-19'){throw 'Original C node3228 owner/path not confirmed; do not restart'}
 if((Get-FileHash -LiteralPath $exe).Hash.ToLowerInvariant() -cne '9174f29bad4bf1feb9acfbd5d2b5ed36bb47ff2a2cd1519d7742a2e1b944dee7'){throw 'Original node hash differs'}
 $ports=@(Get-NetUDPEndpoint|Where-Object {$_.OwningProcess -eq 3228 -and $_.LocalPort -in 41052,42052})
 if($ports.Count -ne 2){throw 'Original C UDP listeners not confirmed; no node action'}
 Save 'node-before-public.json' ([ordered]@{utc=[DateTime]::UtcNow.ToString('o');pid=3228;created_utc=$nodes[0].CreationDate.ToUniversalTime().ToString('o');owner_sid='S-1-5-19';node_sha256=(Get-FileHash $exe).Hash.ToLowerInvariant();ports=@($ports|Select-Object LocalAddress,LocalPort,OwningProcess);node_mutated=$false})
 $out=Join-Path $root 'observer'
 $args=@('-NoProfile','-NonInteractive','-File',('"'+$watch+'"'),'-HostId','C','-Source','875d3307a1fc2ca34b45464ac3bf51a03619adc2','-NodeSha256','9174f29bad4bf1feb9acfbd5d2b5ed36bb47ff2a2cd1519d7742a2e1b944dee7','-WorkName','novonet-observation-875d3307-20261111','-EvidenceRoot',('"'+$out+'"'),'-MaximumWaitSeconds','7200')
 $p=Start-Process -FilePath $pwsh -WindowStyle Hidden -ArgumentList $args -RedirectStandardOutput (Join-Path $root 'observer.stdout.log') -RedirectStandardError (Join-Path $root 'observer.stderr.log') -PassThru
 Save 'observer-started.json' ([ordered]@{host='C';observer_version=3;pid=$p.Id;created_utc=$p.StartTime.ToUniversalTime().ToString('o');node_pid=3228;source='875d3307a1fc2ca34b45464ac3bf51a03619adc2';watch_sha256=(Get-FileHash $watch).Hash.ToLowerInvariant();reader_sha256=(Get-FileHash $reader).Hash.ToLowerInvariant();maximum_wait_seconds=7200;hidden=$true;node_mutated=$false;object_sent=$false;D_touched=$false})
 $p.WaitForExit();$code=$p.ExitCode
 Save 'observer-exit.json' ([ordered]@{host='C';pid=$p.Id;finished_utc=[DateTime]::UtcNow.ToString('o');exit_code=$code})
 exit $code
}catch{Save 'launcher-error.json' ([ordered]@{host='C';utc=[DateTime]::UtcNow.ToString('o');error=$_.Exception.Message;node_mutated=$false;retry_performed=$false;D_touched=$false;object_sent=$false});throw}
