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
$observerAst=[Management.Automation.Language.Parser]::ParseFile("$PSScriptRoot/../../artifacts/audit/novonet-single-relay-20261111/Watch-Relay.ps1",[ref]$tokens,[ref]$errors)
$binding=$observerAst.Find({param($n) $n -is [Management.Automation.Language.IfStatementAst] -and $n.Extent.Text.StartsWith('if($Common95)')},$true)
if($null -eq $binding){throw 'Missing fixed deployment binding'}
$applyBinding=[scriptblock]::Create($binding.Extent.Text)
$Common95=$true;$Source='95ec9c2eced988c5798d549971354ef8722367b7';$NodeSha256='754eb3f50ab6111ec494d30b00af52e8dbee43038f54e0a1c313e8579bfc6bf7';$WorkName='novovm-common-95ec9c2e-20261117'
foreach($HostId in @('C','D')){
    . $applyBinding
    $expectedExe=if($HostId -eq 'C'){'C:\ProgramData\NOVOVM\releases\novovm-common-95ec9c2e-C-20261117\bin\novovm-node.exe'}else{'C:\ProgramData\NOVOVM\releases\novovm-common-95ec9c2e-windows-x86_64\bin\novovm-node.exe'}
    if($exe -cne $expectedExe){throw 'Wrong actual common executable'}
    $expectedFile=if($HostId -eq 'C'){'C:\Windows\ServiceProfiles\LocalService\AppData\Local\NOVOVM-Common95-C-20261117\progress.json'}else{'C:\ProgramData\NOVOVM\selfrun\91129655606029\D\novovm-common-95ec9c2e-20261117\logs\progress.json'}
    if($file -cne $expectedFile){throw 'Wrong current receiver progress binding'};$checks++
}
$Source='0'*40
try{. $applyBinding;throw 'UNEXPECTED_ACCEPT'}catch{if($_.Exception.Message -notlike '*fixed program differs*'){throw}};$checks++
$Source='95ec9c2eced988c5798d549971354ef8722367b7';$NodeSha256='0'*64
try{. $applyBinding;throw 'UNEXPECTED_ACCEPT'}catch{if($_.Exception.Message -notlike '*fixed program differs*'){throw}};$checks++
$Common95=$false;$exe='legacy-exe';$file='legacy-progress';. $applyBinding
if($exe -cne 'legacy-exe' -or $file -cne 'legacy-progress'){throw 'Legacy binding changed'};$checks++
$roleBinding=$observerAst.Find({param($n) $n -is [Management.Automation.Language.IfStatementAst] -and $n.Extent.Text.StartsWith('if($CommonRoleFix)')},$true)
if($null -eq $roleBinding){throw 'Missing fixed role-fix deployment binding'}
$applyRoleBinding=[scriptblock]::Create($roleBinding.Extent.Text)
$CommonRoleFix=$true;$Source='db667aa420477e76766352c8efd7fea83fdb6eac';$NodeSha256='442afbc0e5c5aad9f2f952b5dbff9a08021556114c07a7698c65acf629273825';$WorkName='novovm-common-db667aa4-20261117'
foreach($HostId in @('C','D')){
    . $applyRoleBinding
    $expectedExe=if($HostId -eq 'C'){'C:\ProgramData\NOVOVM\releases\novovm-common-db667aa4-C-20261117\bin\novovm-node.exe'}else{'C:\ProgramData\NOVOVM\releases\novovm-common-db667aa4-windows-x86_64\bin\novovm-node.exe'}
    $expectedFile=if($HostId -eq 'C'){'C:\Windows\ServiceProfiles\LocalService\AppData\Local\NOVOVM-CommonDb667-C-20261117\progress.json'}else{'C:\ProgramData\NOVOVM\selfrun\91129655606029\D\novovm-common-db667aa4-20261117\logs\progress.json'}
    if($exe -cne $expectedExe -or $file -cne $expectedFile){throw 'Wrong role-fix owner binding'};$checks++
}
$Source='0'*40
try{. $applyRoleBinding;throw 'UNEXPECTED_ACCEPT'}catch{if($_.Exception.Message -notlike '*fixed program differs*'){throw}};$checks++
$Source='db667aa420477e76766352c8efd7fea83fdb6eac';$NodeSha256='0'*64
try{. $applyRoleBinding;throw 'UNEXPECTED_ACCEPT'}catch{if($_.Exception.Message -notlike '*fixed program differs*'){throw}};$checks++
$CommonRoleFix=$false;$exe='legacy-exe';$file='legacy-progress';. $applyRoleBinding
if($exe -cne 'legacy-exe' -or $file -cne 'legacy-progress'){throw 'Inactive role-fix binding changed paths'};$checks++
$file=Join-Path $root 'progress.json'
[IO.File]::WriteAllText($file,('x'*(4MB+1)))
try {Read-RelayProgress $file $root|Out-Null;throw 'UNEXPECTED_ACCEPT'}catch{if($_.Exception.Message -notlike '*Invalid observation*'){throw}};$checks++
Write-Output "PASS $checks relay observation reader checks; evidence $root"
