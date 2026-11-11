#Requires -Version 7.0
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
. "$PSScriptRoot/../../artifacts/audit/novonet-single-relay-20261111/Read-RelayProgress.ps1"
$root=Join-Path ([IO.Path]::GetTempPath()) ('relay-reader-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory $root|Out-Null
$file=Join-Path $root 'progress.json'
$valid='{"chain_id":91129655606029,"execution_failures":0,"novonet":{"diagnostic_relay":{"enabled":true}},"native_network_probe":{"transport":{"relay_circuits":[]}}}'
$checks=0
function Reject([string]$Text,[string]$Match) {
    [IO.File]::WriteAllText($file,$Text)
    try {Read-RelayProgress $file $root -WaitMilliseconds 5|Out-Null;throw 'UNEXPECTED_ACCEPT'}
    catch {if($_.Exception.Message -notlike "*$Match*"){throw}}
    $script:checks++
}
[IO.File]::WriteAllText($file,$valid)
$p=Read-RelayProgress $file $root
if($p.chain_id -ne 91129655606029){throw 'Valid result differs'};$checks++
Reject '{"chain_id":91129655606029,"execution_failures":0}' 'missing_novonet'
Reject '{' 'incomplete_json'
Reject '' 'incomplete_json'
Reject ($valid.Replace('91129655606029','123')) 'Wrong chain'
Reject ($valid.Replace('"execution_failures":0','"execution_failures":1')) 'Execution failure'
Reject ($valid.Replace('"enabled":true','"enabled":false')) 'disabled'
Reject ($valid.Replace('"relay_circuits":[]','"other":[]')) 'Missing native transport'
[IO.File]::WriteAllText($file,$valid)
[IO.File]::SetLastWriteTimeUtc($file,[DateTime]::UtcNow.AddSeconds(-16))
try {Read-RelayProgress $file $root|Out-Null;throw 'UNEXPECTED_ACCEPT'}catch{if($_.Exception.Message -notlike '*Stale progress*'){throw}};$checks++
# Simulate the actual writer's absent-observation -> complete report transition.
[IO.File]::WriteAllText($file,'{"chain_id":91129655606029,"execution_failures":0}')
$job=Start-ThreadJob -ArgumentList $file,$valid -ScriptBlock {param($f,$v);Start-Sleep -Milliseconds 200;[IO.File]::WriteAllText($f,$v)}
try {$p=Read-RelayProgress $file $root -WaitMilliseconds 3000;if(!$p.novonet.diagnostic_relay.enabled){throw 'No complete snapshot'}}finally{$job|Wait-Job|Receive-Job;$job|Remove-Job};$checks++
$anomaly=Get-Content "$root/read-anomaly-first.json" -Raw|ConvertFrom-Json
if($anomaly.accepted -or $anomaly.reason -ne 'missing_novonet'){throw 'Missing original anomaly'};$checks++
$tokens=$null;$errors=$null
[Management.Automation.Language.Parser]::ParseFile("$PSScriptRoot/../../artifacts/audit/novonet-single-relay-20261111/Watch-Relay.ps1",[ref]$tokens,[ref]$errors)|Out-Null
if($errors.Count){throw ($errors|Out-String)};$checks++
[IO.File]::WriteAllText($file,('x'*(4MB+1)))
try {Read-RelayProgress $file $root|Out-Null;throw 'UNEXPECTED_ACCEPT'}catch{if($_.Exception.Message -notlike '*Invalid observation*'){throw}};$checks++
Write-Output "PASS $checks relay observation reader checks; evidence $root"
