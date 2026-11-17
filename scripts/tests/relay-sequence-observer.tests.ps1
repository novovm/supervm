#Requires -Version 7.0
$ErrorActionPreference='Stop'
$path="$PSScriptRoot/../../artifacts/audit/novonet-single-relay-20261111/Watch-Relay.ps1"
$tokens=$null;$errors=$null
$ast=[Management.Automation.Language.Parser]::ParseFile($path,[ref]$tokens,[ref]$errors)
if($errors.Count){throw 'Observer parse failed'}
$branch=$ast.Find({param($n) $n -is [Management.Automation.Language.IfStatementAst] -and $n.Extent.Text.StartsWith('if($TerminalCircuitCount -gt 1)')},$true)
if($null-eq $branch){throw 'Actual sequence comparison branch missing'}
$run=[scriptblock]::Create($branch.Extent.Text)
$keyFunction=$ast.Find({param($n) $n -is [Management.Automation.Language.FunctionDefinitionAst] -and $n.Name-eq 'ProbeKey'},$true)
. ([scriptblock]::Create($keyFunction.Extent.Text))
$circuits=@([pscustomobject]@{forwarded=@(@{packets=8;bytes=8000},@{packets=8;bytes=8000});stream_dropped_us=$null})
$p=[pscustomobject]@{novonet=@{diagnostic_relay=@{events=@(@{event='new';probe=@{nonce=[byte[]]::new(32)}})}};native_network_probe=@{transport=@{elapsed_us=1}}}
$oldNonces=@();$checks=1
$TerminalCircuitCount=4;. $run;$before=$encoded
$p.native_network_probe.transport.elapsed_us=9999;. $run
if($encoded-cne $before){throw 'Unrelated elapsed time spends snapshot budget'};$checks++
$circuits[0].forwarded[0].packets=9;. $run
if($encoded-ceq $before){throw 'Actual relay progress omitted'};$checks++
$before=$encoded;$p.novonet.diagnostic_relay.events[0].event='received';. $run
if($encoded-ceq $before){throw 'Actual receiver event omitted'};$checks++
$TerminalCircuitCount=1;$encoded='legacy-comparison';. $run
if($encoded-cne 'legacy-comparison'){throw 'Legacy comparison changed'};$checks++
$assignment=$ast.Find({param($n) $n -is [Management.Automation.Language.AssignmentStatementAst] -and $n.Left.Extent.Text-eq '$terminal'},$true)
if($null-eq $assignment){throw 'Actual terminal assignment missing'}
$decide=[scriptblock]::Create($assignment.Extent.Text)
$TerminalCircuitCount=4;$closedCount=1;$receiverTerminal=$true;$HostId='D';. $decide
if($terminal){throw 'Sequence exits after first received object'};$checks++
$closedCount=4;. $decide
if(!$terminal){throw 'Complete sequence does not terminate'};$checks++
$TerminalCircuitCount=1;$closedCount=0;. $decide
if(!$terminal){throw 'Legacy target early exit changed'};$checks++
$HostId='C';$receiverTerminal=$false;$closedCount=1;. $decide
if(!$terminal){throw 'Legacy relay early exit changed'};$checks++
if(!$ast.Extent.Text.Contains('$saved -lt 64')){throw 'Snapshot budget changed'};$checks++
$coordinator="$PSScriptRoot/../../artifacts/audit/novonet-concurrent-recovery-20261117/Run-A-Scenario.ps1"
$coordinatorAst=[Management.Automation.Language.Parser]::ParseFile($coordinator,[ref]$tokens,[ref]$errors)
if($errors.Count){throw 'Actual coordinator parse failed'};$checks++
foreach($f in $coordinatorAst.FindAll({param($n) $n -is [Management.Automation.Language.FunctionDefinitionAst]},$true)){
 if(Get-Alias -Name $f.Name -ErrorAction SilentlyContinue){throw "Coordinator function shadowed by PowerShell alias: $($f.Name)"}
};$checks++
$start=$coordinatorAst.Find({param($n) $n -is [Management.Automation.Language.AssignmentStatementAst] -and $n.Left.Extent.Text-eq '$overlapStart'},$true)
$end=$coordinatorAst.Find({param($n) $n -is [Management.Automation.Language.AssignmentStatementAst] -and $n.Left.Extent.Text-eq '$overlapEnd'},$true)
$overlap=[scriptblock]::Create($start.Extent.Text+"`n"+$end.Extent.Text)
$lo=525;$hi=540;$aStart=@(@{wall_ms=1000});$aDone=@{wall_ms=1100};$bStart=@(@{wall_ms=1550});$bEnd=@(@{wall_ms=1650})
. $overlap
if($overlapEnd-$overlapStart-ne 75){throw 'Actual uncertain-clock overlap calculation differs'};$checks++
$bStart=@(@{wall_ms=1700});$bEnd=@(@{wall_ms=1750});. $overlap
if($overlapEnd-gt $overlapStart){throw 'Serial delivery incorrectly counted concurrent'};$checks++
$bStart=@(@{wall_ms=1550});$bEnd=@(@{wall_ms=1650});$lo=0;$hi=1000;. $overlap
if($overlapEnd-gt $overlapStart){throw 'Uncertain timing incorrectly counted concurrent'};$checks++
$routeFunction=$ast.Find({param($n) $n -is [Management.Automation.Language.FunctionDefinitionAst] -and $n.Name-eq 'Test-ExpectedRelayRoute'},$true)
if($null-eq $routeFunction){throw 'Expected-route terminal predicate missing'}
. ([scriptblock]::Create($routeFunction.Extent.Text))
$origin='novovm-mldsa65:'+('a'*64);$pathPeers=@('novovm-mldsa65:'+('b'*64),'novovm-mldsa65:'+('c'*64),'novovm-mldsa65:'+('d'*64))
$route=[pscustomobject]@{origin=$origin;path=$pathPeers}
if(!(Test-ExpectedRelayRoute $route $origin $pathPeers)){throw 'Actual requested route rejected'};$checks++
if(Test-ExpectedRelayRoute $route ('novovm-mldsa65:'+('e'*64)) $pathPeers){throw 'Wrong origin terminates observer'};$checks++
if(Test-ExpectedRelayRoute $route $origin @($pathPeers[0],$pathPeers[2],$pathPeers[1])){throw 'Wrong path order terminates observer'};$checks++
if(Test-ExpectedRelayRoute $route $origin @($pathPeers[0],$pathPeers[1])){throw 'Truncated path terminates observer'};$checks++
if(!(Test-ExpectedRelayRoute $route '' @())){throw 'Legacy unfiltered stop decision changed'};$checks++
$select=$ast.Find({param($n) $n -is [Management.Automation.Language.AssignmentStatementAst] -and $n.Left.Extent.Text-eq '$terminalCircuits'},$true)
$runSelect=[scriptblock]::Create($select.Extent.Text)
$ExpectedOrigin=$origin;$ExpectedPath=$pathPeers
$circuits=@(@{route=$route;stream_dropped_us=$null},@{route=@{origin=$origin;path=@($pathPeers[2],$pathPeers[0])};stream_dropped_us=1})
. $runSelect
if($terminalCircuits.Count-ne 1 -or @($terminalCircuits|Where-Object {$null-ne $_.stream_dropped_us}).Count-ne 0 -or $circuits.Count-ne 2){throw 'Unrelated closed circuit counted or raw circuits filtered'};$checks++
$receiverAssignment=$ast.Find({param($n) $n -is [Management.Automation.Language.AssignmentStatementAst] -and $n.Left.Extent.Text-eq '$receiverTerminal'},$true)
$runReceiver=[scriptblock]::Create($receiverAssignment.Extent.Text)
$session=[byte[]](1..16);$sessionHex=[Convert]::ToHexString($session).ToLowerInvariant()
$p.novonet.diagnostic_relay.events=@(@{event='relay_probe_received_integrity_verified';probe=@{nonce=[byte[]]::new(32)};session=$session})
$terminalSessions=@('0'*32);. $runReceiver
if($receiverTerminal){throw 'Unrelated receiver session terminates expected route'};$checks++
$terminalSessions=@($sessionHex);. $runReceiver
if(!$receiverTerminal){throw 'Actual expected receiver session not terminal'};$checks++
$ExpectedOrigin='';$terminalSessions=@();. $runReceiver
if(!$receiverTerminal){throw 'Legacy receiver terminal behavior changed'};$checks++
Write-Output "PASS $checks sequence-observer checks; no node, clock or network mutation"
