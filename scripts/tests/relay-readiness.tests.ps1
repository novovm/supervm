#Requires -Version 7.0
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
. "$PSScriptRoot/../../artifacts/audit/novonet-single-relay-20261111/Assert-RelayReadiness.ps1"
$now=[DateTime]::UtcNow
$source='875d3307a1fc2ca34b45464ac3bf51a03619adc2'
$hash='9174f29bad4bf1feb9acfbd5d2b5ed36bb47ff2a2cd1519d7742a2e1b944dee7'
$r=[pscustomobject]@{status='D_RELAY_OBSERVER_ARMED';observer_version=3;observer_pid=10;pid=20;source=$source;node_sha256=$hash;read_only=$true;utc=$now.AddMinutes(-40).ToString('o');maximum_wait_seconds=7200}
$l=[pscustomobject]@{host='D';observer_version=3;observer_pid=10;node_pid=20;active=$true;utc=$now.AddSeconds(-10).ToString('o');elapsed_seconds=2390;maximum_wait_seconds=7200}
$count=0
Assert-RelayReadiness $r $l 'D' $source $hash $now;$count++
foreach($case in @('wrong-source','wrong-node','wrong-host','inactive','pid','node-pid','version','readonly','stale','future','limit','elapsed','nan','mono-expiry','wall-expiry','lifetime-mismatch')){
    $a=$r.PSObject.Copy();$b=$l.PSObject.Copy()
    switch($case){
        'wrong-source'{$a.source='other'} 'wrong-node'{$a.node_sha256='other'}
        'wrong-host'{$b.host='C'} 'inactive'{$b.active=$false} 'pid'{$b.observer_pid=11}
        'node-pid'{$b.node_pid=21} 'version'{$b.observer_version=2} 'readonly'{$a.read_only=$false}
        'stale'{$b.utc=$now.AddSeconds(-601).ToString('o')} 'future'{$b.utc=$now.AddSeconds(61).ToString('o')}
        'limit'{$a.maximum_wait_seconds=7201} 'elapsed'{$b.elapsed_seconds=-1} 'nan'{$b.elapsed_seconds=[double]::NaN}
        'mono-expiry'{$b.elapsed_seconds=7071} 'wall-expiry'{$a.utc=$now.AddSeconds(-7081).ToString('o')}
        'lifetime-mismatch'{$b.maximum_wait_seconds=1800}
    }
    $rejected=$false;try{Assert-RelayReadiness $a $b 'D' $source $hash $now}catch{$rejected=$true}
    if(!$rejected){throw "Unsafe $case accepted"};$count++
}
$a=$r.PSObject.Copy();$b=$l.PSObject.Copy()
$a.observer_version=2;$b.observer_version=2;$a.maximum_wait_seconds=1800;$a.utc=$now.AddSeconds(-100).ToString('o')
Assert-RelayReadiness $a $b 'D' $source $hash $now;$count++
$a.utc=$now.AddSeconds(-1501).ToString('o')
$rejected=$false;try{Assert-RelayReadiness $a $b 'D' $source $hash $now}catch{$rejected=$true}
if(!$rejected){throw 'Expired legacy deadline accepted'};$count++
# Actual D receipt arrived with less than52 seconds remaining: reject, never fake send.
$dir="$PSScriptRoot/../../artifacts/audit/novonet-single-relay-20261111/candidate-875d3307/D-relay-after-B-20261111"
$actualR=Get-Content -Raw "$dir/ready.json"|ConvertFrom-Json
$actualL=Get-Content -Raw "$dir/observations-20261111T1519Z/liveness.json"|ConvertFrom-Json
$rejected=$false;try{Assert-RelayReadiness $actualR $actualL 'D' $source $hash ([DateTime]'2026-11-11T15:21:38Z')}catch{$rejected=$true}
if(!$rejected){throw 'Actual late D handoff accepted'};$count++
Write-Output "PASS $count observer readiness cases; no network or node actions"
