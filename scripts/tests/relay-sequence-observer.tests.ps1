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
Write-Output "PASS $checks sequence-observer checks; no node, clock or network mutation"
