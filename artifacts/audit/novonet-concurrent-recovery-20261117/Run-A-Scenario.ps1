#Requires -Version 7.0
#Requires -RunAsAdministrator
# One bounded real-host workload; only existing CLI, owner observations and B unit.
param([Parameter(Mandatory)][string]$CReadyFile,[Parameter(Mandatory)][string]$DReadyFile,[Parameter(Mandatory)][string]$EvidenceRoot,[string]$ContinueRestoredAttempt)
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$PSNativeCommandUseErrorActionPreference=$false
$repo='D:\WorksArea\SUPERVM'
. "$repo/artifacts/audit/novonet-single-relay-20261111/Read-RelayProgress.ps1"
. "$repo/artifacts/audit/novonet-single-relay-20261111/Assert-RelayReadiness.ps1"
$source='5b656189784c854ac356aae77610c931b9f2189d'
$nodePin='4b0d03e8ad7a5a93051cee2672d38aa9a4ec8c0cb99669ae89af62b8eeef4038'
$release='C:\ProgramData\NOVOVM\releases\novovm-common-5b656189-windows-x86_64'
$work='C:\ProgramData\NOVOVM\selfrun\91129655606029\A\novovm-common-5b656189-20261117'
$out=[IO.Path]::GetFullPath($EvidenceRoot)
if(Test-Path -LiteralPath $out){throw 'Preserve previous attempt; no repeated workload'}
$nodes=@(Get-CimInstance Win32_Process -Filter "Name='novovm-node.exe'"|Where-Object ExecutablePath -IEQ "$release\bin\novovm-node.exe")
if($nodes.Count-ne 1 -or (Invoke-CimMethod $nodes[0] -MethodName GetOwnerSid).Sid-cne 'S-1-5-19'){throw 'Wrong original A owner'}
foreach($pin in @(@('novovm-node.exe',$nodePin),@('novovmctl.exe','de39d4f88cc794306f48a0ce5404fb97030edafa111867583b6a5ccfbe63134c'))){if((Get-FileHash "$release/bin/$($pin[0])").Hash.ToLowerInvariant()-cne $pin[1]){throw 'A fixed program differs'}}
foreach($pair in @(@('C',$CReadyFile),@('D',$DReadyFile))){
 $parent=Split-Path -Parent $pair[1];$ready=Get-Content -Raw -LiteralPath $pair[1]|ConvertFrom-Json
 $live=Get-Content -Raw -LiteralPath "$parent/liveness.json"|ConvertFrom-Json
 foreach($terminal in @('error.json','result.json','collector-exit.json')){if(Test-Path -LiteralPath "$parent/$terminal"){throw 'Observer already ended'}}
 Assert-RelayReadiness $ready $live $pair[0] $source $nodePin
 $requiredCircuitCount=if($ContinueRestoredAttempt){1}else{4}
 if($ready.terminal_circuit_count-ne $requiredCircuitCount){throw 'Observer circuit scope differs from actual remaining workload'}
 if($ContinueRestoredAttempt){
  $expectedOrigin='novovm-mldsa65:6144e3de68011031592a7f66422b077989e643b7281b8904384b6cf1c321d5b3'
  $expectedPath=@('novovm-mldsa65:3a44e06e371273a050641a80d9103ac4d357954aabc5336072cfb72864b4cf8f','novovm-mldsa65:516fc66162a3caa1d0153da9c42b5bbae9eae1e8bdcfb097b70aaa4ee97dd9f2','novovm-mldsa65:49ba6bfc9206a0bc5fcb93e83975c10d10a228dbf7cd9bf4a2d909ca7df0d946')
  if($ready.expected_origin-cne $expectedOrigin -or (@($ready.expected_path)-join ',')-cne ($expectedPath-join ',')){throw 'Recovered-route observer is not bound to A-B-C-D'}
 }
}
$config=Get-Content -Raw -LiteralPath "$work/run.json"|ConvertFrom-Json
$net=Get-Content -Raw -LiteralPath $config.novonet_config_file|ConvertFrom-Json
$command=$net.probe_control_file
if($command.Replace('\','/')-cne 'C:/ProgramData/NOVOVM/selfrun/91129655606029/A/novonet-observation-a25452f4-20261112/commands/control.nvprobe' -or !$net.relay_forwarding){throw 'Original A command owner differs'}
function Hex($v){[Convert]::ToHexString([byte[]]$v).ToLowerInvariant()}
$ids=@($config.genesis.validators|ForEach-Object{Hex ([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes("novovm-round-bft/v2/mldsa65/validator-id`0")+[byte[]]$_.public_key))})
$pins=@('b5d25773baaf82e8c109d41782a15ac3e45b57562f9bb023c8c1cab63d6acc43','4cecd4ba4092c821333501687e7a363d829d544c1a203ea2390d76e8b2a5627f','1c054a44ee79b50bf5d059a2f8fc15dd45afcdf8fa10d44a5e12c07afab89a2a','d90f5145f324790d6cef7b592d25747d0fb21958327eaeeb2367c800d74a8f90')
if(($ids-join ',')-cne ($pins-join ',')){throw 'Fixed original identities differ'}
New-Item -ItemType Directory -Path $out|Out-Null
Copy-Item -LiteralPath $CReadyFile -Destination "$out/C-ready.json"
Copy-Item -LiteralPath $DReadyFile -Destination "$out/D-ready.json"
$bFile='/home/ecs-user/novonet-concurrent-5b656189-20261118/B-Scenario.py'
$bPin=(Get-FileHash -LiteralPath "$PSScriptRoot/B-Scenario.py").Hash.ToLowerInvariant()
$bCommand="echo '$bPin  $bFile' | sha256sum -c - >/dev/null && sudo -n python3 $bFile"
function B([string]$Action,[string]$Extra=''){
 $text=& ssh.exe -F 'C:\Users\leadb\.ssh\validator-B.conf' -o BatchMode=yes -o ConnectTimeout=10 validator-B "$bCommand $Action $Extra" 2> "$out/B-$Action.stderr"
 if($LASTEXITCODE-ne 0){throw "B $Action failed; no blind retry"}
 ($text-join "`n")|ConvertFrom-Json
}
function BJob([string]$Action,[string]$Extra=''){
 Start-ThreadJob -ArgumentList $bCommand,$Action,$Extra,$out -ScriptBlock {
  param($cmd,$action,$extra,$root)
  & ssh.exe -F 'C:\Users\leadb\.ssh\validator-B.conf' -o BatchMode=yes -o ConnectTimeout=10 validator-B "$cmd $action $extra" 1> "$root/B-$action.json" 2> "$root/B-$action.stderr"
  $LASTEXITCODE
 }
}
function FinishBJob($Job,[string]$Action){
 $null=$Job|Wait-Job -Timeout 45
 if($Job.State-ne 'Completed'){throw "B $Action not completed; preserve running management job"}
 $exit=@($Job|Receive-Job);$Job|Remove-Job
 if($exit.Count-ne 1 -or $exit[0]-ne 0){throw "B $Action failed"}
 Get-Content -Raw -LiteralPath "$out/B-$Action.json"|ConvertFrom-Json
}
function P {Read-RelayProgress -Path "$work/logs/progress.json" -EvidenceRoot $out}
function Save($Value,[string]$File){$Value|ConvertTo-Json -Depth 65|Set-Content -LiteralPath "$out/$File.json" -Encoding utf8}
function PublicA($p){[ordered]@{utc=[DateTime]::UtcNow.ToString('o');pid=$nodes[0].ProcessId;diagnostic=$p.novonet.diagnostic_relay;transport=$p.native_network_probe.transport;execution_failures=$p.execution_failures}}
function Invoke-OriginalNativeProbe([string]$Label,[string[]]$Arguments){
 & "$release/bin/novovmctl.exe" native-network-probe @Arguments --output $command 1> "$out/$Label-cli.json" 2> "$out/$Label-cli.stderr"
 if($LASTEXITCODE-ne 0){throw "A $Label local command failed; no resend"}
}
function ArmA([string]$Label,[int]$Target,[int[]]$Via){
 $p=P
 if($p.novonet.diagnostic_relay.active){throw 'A diagnostic still occupied'}
 $rejectedBefore=$p.novonet.probe_control.rejected_commands
 if(Test-Path -LiteralPath $command){Copy-Item -LiteralPath $command -Destination "$out/$Label-prior-control.nvprobe"}
 $nonce=Hex ([Security.Cryptography.RandomNumberGenerator]::GetBytes(32))
 $cliArgs=@('prepare','--sender',$ids[0],'--recipient',$ids[$Target],'--nonce',$nonce)
 foreach($v in $Via){$cliArgs+=@('--via',$ids[$v])}
 $armedTick=[Diagnostics.Stopwatch]::GetTimestamp()
 Invoke-OriginalNativeProbe $Label $cliArgs
 $arm="$out/$Label-arm.nvprobe";Copy-Item -LiteralPath $command -Destination $arm
 $clock=[Diagnostics.Stopwatch]::StartNew()
 do {
  $p=P;$r=$p.novonet.diagnostic_relay.reservation
  if($p.novonet.probe_control.rejected_commands-gt $rejectedBefore){Save (PublicA $p) "$Label-owner-rejected";throw "A original owner rejected new reservation: $($p.novonet.probe_control.last)"}
  if($null-ne $r -and (Hex $r.nonce)-ceq $nonce -and $r.ready){
   $session=@($p.native_network_probe.transport.relay_circuits|Where-Object{$_.route.role-ceq 'origin' -and $null-ne $_.native_session -and $null-eq $_.stream_dropped_us})
   if($session.Count-ne 1){throw 'A nonunique actual authenticated route'}
   Save (PublicA $p) "$Label-ready"
   return [pscustomobject]@{label=$Label;nonce=$nonce;arm=$arm;armed_tick=$armedTick;ready_tick=[Diagnostics.Stopwatch]::GetTimestamp();session=(Hex $session[0].native_session);circuit=(Hex $session[0].route.circuit)}
  }
  $failed=@($p.novonet.diagnostic_relay.events|Where-Object{$_.event-ceq 'relay_diagnostic_failed' -and $null-ne $_.probe -and (Hex $_.probe.nonce)-ceq $nonce})
  if($failed.Count){Save (PublicA $p) "$Label-first-failure";throw 'A actual setup failed; no object sent'}
  Start-Sleep -Milliseconds 100
 }while($clock.Elapsed.TotalSeconds-lt 35)
 throw 'A no actual ready reservation; no object sent'
}
function WaitADone($Attempt){
 $clock=[Diagnostics.Stopwatch]::StartNew()
 do{
  $p=P;$events=@($p.novonet.diagnostic_relay.events|Where-Object{$null-ne $_.probe -and (Hex $_.probe.nonce)-ceq $Attempt.nonce})
  $done=@($events|Where-Object event -CEQ 'relay_probe_object_done')
  if($done.Count-eq 1){Save (PublicA $p) "$($Attempt.label)-after";return $done[0]}
  if(@($events|Where-Object event -In @('relay_diagnostic_failed','relay_probe_terminal_failure')).Count){Save (PublicA $p) "$($Attempt.label)-failed";throw 'A original object failed; no retry'}
  Start-Sleep -Milliseconds 100
 }while($clock.Elapsed.TotalSeconds-lt 15)
 Save (PublicA $p) "$($Attempt.label)-no-DONE";throw 'A no original DONE; no resend'
}
function WaitLease($Attempt){
 # Preserve the existing60s reservation. This is orchestration, not a longer protocol deadline.
 while([Diagnostics.Stopwatch]::GetElapsedTime($Attempt.ready_tick).TotalSeconds-lt 61){Start-Sleep -Milliseconds 500}
}
function ClockSample([string]$Label){
 $samples=@()
 $info=[Diagnostics.ProcessStartInfo]::new('ssh.exe');$info.UseShellExecute=$false;$info.CreateNoWindow=$true
 $info.RedirectStandardInput=$true;$info.RedirectStandardOutput=$true;$info.RedirectStandardError=$true
 foreach($value in @('-F','C:\Users\leadb\.ssh\validator-B.conf','-o','BatchMode=yes','-o','ConnectTimeout=10','validator-B',"$bCommand clock-stream")){$info.ArgumentList.Add($value)}
 $process=[Diagnostics.Process]::Start($info)
 try{
  $line=$process.StandardOutput.ReadLineAsync()
  if(!$line.Wait(15000) -or !($line.Result|ConvertFrom-Json).ready){throw 'Management clock stream not ready'}
  foreach($i in 0..2){
   $challenge=[guid]::NewGuid().ToString('N')
   $t1=[DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds();$clock=[Diagnostics.Stopwatch]::StartNew()
   $process.StandardInput.WriteLine($challenge);$process.StandardInput.Flush()
   $line=$process.StandardOutput.ReadLineAsync()
   if(!$line.Wait(10000)){throw 'Bounded management timing response missing'}
   $b=$line.Result|ConvertFrom-Json;$t4=[DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
   if($b.challenge-cne $challenge -or [Math]::Abs(($t4-$t1)-$clock.ElapsedMilliseconds)-gt 20){throw 'Management timing challenge/clock jump mismatch'}
   $samples+=[pscustomobject]@{lo=[double]$b.wall_ms-$t4-1;hi=[double]$b.wall_ms-$t1+1;rtt_ms=$clock.Elapsed.TotalMilliseconds}
  }
  $process.StandardInput.Close()
  if(!$process.WaitForExit(5000) -or $process.ExitCode-ne 0){throw 'Management clock stream did not exit successfully'}
 }finally{
  if(!$process.HasExited){$process.Kill()}
  $process.Dispose()
 }
 Save $samples $Label
 $samples|Sort-Object rtt_ms|Select-Object -First 1
}
$bJob=$null;$first=$null;$fault=$null;$recovered=$null;$completed=$false
try{
 Save (B 'preflight') 'B-preflight'
 if($ContinueRestoredAttempt){
  # Continue only the unfinished fresh-route delivery, never the two completed
  # objects or a second outage. Retain the failed automatic restoration verdict.
  $previous=[IO.Path]::GetFullPath($ContinueRestoredAttempt)
  $failure=Get-Content "$previous/error.json" -Raw|ConvertFrom-Json
  $two=Get-Content "$previous/concurrency-origin-result.json" -Raw|ConvertFrom-Json
  $restored=Get-Content "$previous/B-original-owner-restored.json" -Raw|ConvertFrom-Json
  $currentB=Get-Content "$out/B-preflight.json" -Raw|ConvertFrom-Json
  if($failure.error-cne 'B interrupt failed; no blind retry' -or !$two.two_sender_DONE -or $restored.source-cne 'db667aa420477e76766352c8efd7fea83fdb6eac' -or $restored.start-cne 'existing' -or !$restored.config_unchanged -or !$restored.network_unchanged -or !$restored.custody_unchanged -or (Test-Path "$previous/recovery-origin-result.json")){throw 'Not the exact interrupted/restored unfinished attempt'}
  # The approved existing upgrade replaced both processes. Correlate its actual
  # receipt; never require old volatile circuit state in a new process.
  $upgrade=Get-Content "$PSScriptRoot/candidate-5b656189/B-existing-public.json" -Raw|ConvertFrom-Json
  if($upgrade.source-cne $source -or $upgrade.old_exit.pid-ne $restored.pid -or $currentB.pid-ne $upgrade.pid -or $upgrade.start-cne 'existing' -or $upgrade.initialize_executed -or !$upgrade.custody_unchanged -or $upgrade.after_progress.execution_failures-ne 0 -or $upgrade.after_progress.recovery_in_progress){throw 'Actual restored-to-upgraded B chain differs'}
  $prior=Get-Content "$previous/before-B-interruption-ready.json" -Raw|ConvertFrom-Json
  $oldRoutes=@($prior.transport.relay_circuits|Where-Object{$_.route.role-ceq 'origin' -and $null-ne $_.native_session -and $null-eq $_.stream_dropped_us})
  if($oldRoutes.Count-ne 1){throw 'Original interrupted route ambiguous'}
  $oldCircuit=Hex $oldRoutes[0].route.circuit;$oldSession=Hex $oldRoutes[0].native_session
  $p=P;Save (PublicA $p) 'A-original-circuit-after-restoration'
  $historicalPath="$PSScriptRoot/candidate-5b656189/prior-circuit-after-restoration.json"
  if((Get-FileHash $historicalPath).Hash.ToLowerInvariant()-cne '3aabb9264f356c7562a2b93c1800c7b58e375b43036d83c46f7c1f2fc8508a1e'){throw 'Historical circuit evidence differs'}
  $historical=Get-Content $historicalPath -Raw|ConvertFrom-Json
  $closed=@($historical.transport.relay_circuits|Where-Object{(Hex $_.route.circuit)-ceq $oldCircuit})
  if($closed.Count-ne 1 -or $null-eq $closed[0].stream_dropped_us -or $p.novonet.diagnostic_relay.active){throw 'Original interruption/expired reservation not established'}
  $recovered=ArmA 'after-B-recovery' 3 @(1,2)
  if($recovered.session-ceq $oldSession -or $recovered.circuit-ceq $oldCircuit){throw 'Interrupted session/circuit reused'}
  Invoke-OriginalNativeProbe 'after-B-recovery-send' @('send-relay','--from',$recovered.arm)
  $recoveryDone=WaitADone $recovered
  Save ([ordered]@{new_session=$recovered.session;new_circuit=$recovered.circuit;old_session=$oldSession;old_circuit=$oldCircuit;new_object=(Hex $recoveryDone.object);sender_DONE=$true;original_reserved_route_interrupted=$true;automatic_restore_attempt_failed=$true;original_owner_restored_separately=$true;approved_existing_upgrade_after_restore=$true;old_circuit_evidence_is_historical=$true;planned_five_second_downtime_achieved=$false;old_object_resumed=$false;prior_attempt=$previous;C_D_receiver_and_forwarding_evidence_pending=$true}) 'recovery-origin-result'
  $completed=$true
  Save @{status='RECOVERED_FRESH_OBJECT_ORIGIN_DONE_C_D_AUDIT_PENDING';utc=[DateTime]::UtcNow.ToString('o');completed_objects_resent=$false;second_outage=$false;automatic_restore_pass=$false} 'result'
  return
 }
 $timing=ClockSample 'management-clock-before'
 $bJob=BJob 'arm'
 # Two distinct native owners, two recipient owners; no node exceeds its one-flight diagnostic limit.
 $first=ArmA 'concurrent-A' 2 @(1,3)
 $bReady=FinishBJob $bJob 'arm';$bJob=$null
 $target=[DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()+4000
 $bTarget=[long][Math]::Round($target+($timing.lo+$timing.hi)/2)
 $bJob=BJob 'send-at' "--wall-ms $bTarget"
 $deadline=[Diagnostics.Stopwatch]::GetTimestamp()+[long](($target-[DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds())/1000*[Diagnostics.Stopwatch]::Frequency)
 while([Diagnostics.Stopwatch]::GetTimestamp()-lt $deadline){Start-Sleep -Milliseconds 1}
 Invoke-OriginalNativeProbe 'concurrent-A-send' @('send-relay','--from',$first.arm)
 $aDone=WaitADone $first
 $bDone=FinishBJob $bJob 'send-at';$bJob=$null
 Save $bDone 'B-concurrent-result'
 $timingAfter=ClockSample 'management-clock-after'
 $lo=[Math]::Min($timing.lo,$timingAfter.lo);$hi=[Math]::Max($timing.hi,$timingAfter.hi)
 $aReport=Get-Content -Raw -LiteralPath "$out/concurrent-A-after.json"|ConvertFrom-Json
 $aStart=@($aReport.diagnostic.events|Where-Object{$_.event-ceq 'relay_probe_admitted' -and (Hex $_.probe.nonce)-ceq $first.nonce})
 $bStart=@($bDone.events|Where-Object event -CEQ 'relay_probe_admitted');$bEnd=@($bDone.events|Where-Object event -CEQ 'relay_probe_object_done')
 if($aStart.Count-ne 1 -or $bStart.Count-ne 1 -or $bEnd.Count-ne 1){throw 'Original admission/DONE evidence incomplete'}
 $overlapStart=[Math]::Max([double]$aStart[0].wall_ms,[double]$bStart[0].wall_ms-$lo)
 $overlapEnd=[Math]::Min([double]$aDone.wall_ms,[double]$bEnd[0].wall_ms-$hi)
 Save ([ordered]@{two_sender_DONE=$true;management_offset_interval_ms=@($lo,$hi);guaranteed_overlap_ms=[Math]::Max(0,$overlapEnd-$overlapStart);concurrent_inflight_proven=($overlapEnd-gt $overlapStart);scope='Two actual original objects; no serial-as-concurrent claim; timing via authenticated management channel, never authorization';A_object=(Hex $aDone.object);A_session=(Hex $aDone.session);B_object=(Hex $bEnd[0].object);B_session=(Hex $bEnd[0].session);receiver_and_hop_evidence_pending=$true}) 'concurrency-origin-result'
 WaitLease $first
 $fault=ArmA 'before-B-interruption' 3 @(1,2)
 Save (B 'interrupt') 'B-interrupted-and-existing-restored'
 $p=P;Save (PublicA $p) 'A-after-B-interruption'
 $old=@($p.native_network_probe.transport.relay_circuits|Where-Object{(Hex $_.route.circuit)-ceq $fault.circuit})
 if($old.Count-ne 1 -or $null-eq $old[0].stream_dropped_us){throw 'Original circuit interruption not observed; do not claim recovery'}
 Invoke-OriginalNativeProbe 'cancel-interrupted-reservation' @('cancel','--from',$fault.arm)
 WaitLease $fault
 $recovered=ArmA 'after-B-recovery' 3 @(1,2)
 if($recovered.session-ceq $fault.session -or $recovered.circuit-ceq $fault.circuit){throw 'Old interrupted session/circuit reused'}
 Invoke-OriginalNativeProbe 'after-B-recovery-send' @('send-relay','--from',$recovered.arm)
 $recoveryDone=WaitADone $recovered
 Save ([ordered]@{new_session=$recovered.session;new_circuit=$recovered.circuit;old_session=$fault.session;old_circuit=$fault.circuit;new_object=(Hex $recoveryDone.object);sender_DONE=$true;original_reserved_route_interrupted=$true;inflight_old_object_automatic_resume_tested=$false;scope='Original B unit stop/existing restart, circuit loss, reauthentication and one fresh object; not old-object resurrection';C_D_receiver_and_forwarding_evidence_pending=$true;resend=$false}) 'recovery-origin-result'
 $completed=$true
 Save @{status='ORIGIN_WORKLOAD_COMPLETE_C_D_AUDIT_PENDING';utc=[DateTime]::UtcNow.ToString('o');no_direct_or_previous_single_object_retests=$true;business_receipt=$false} 'result'
}catch{
 Save @{utc=[DateTime]::UtcNow.ToString('o');error=$_.Exception.Message;object_resend=$false;completed=$completed} 'error'
 throw
}finally{
 if(!$completed){
  foreach($attempt in @($first,$fault,$recovered)|Where-Object{$null-ne $_}){
   try{Invoke-OriginalNativeProbe "$($attempt.label)-cancel-on-stop" @('cancel','--from',$attempt.arm)}catch{Save @{error=$_.Exception.Message} "$($attempt.label)-cancel-error"}
  }
 }
}
