#Requires -Version 7.0
param([string]$PackageRoot,[switch]$InspectPreparedFirewall)
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$path=Join-Path $PSScriptRoot '../windows/Update-NovonetExisting.ps1'
$tokens=$null;$parseErrors=$null
$ast=[Management.Automation.Language.Parser]::ParseFile((Resolve-Path -LiteralPath $path),[ref]$tokens,[ref]$parseErrors)
if($parseErrors.Count){throw 'Update script parse errors'}
$functions=@($ast.FindAll({param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst]},$false))
foreach($name in @('Write-NewJson','Assert-NoRedirect','Assert-Package','Assert-FirewallFilterScope','Assert-CrashedOwnerEvidence','ConvertFrom-NovonetCrashEvent','Assert-StoppedOwnerEvidence','Assert-OriginalFirewallScope','Assert-RollbackReady','Assert-MissingProgressRecovery','Assert-RecoveredProgress')) {
    $function=@($functions|Where-Object Name -CEQ $name)
    if($function.Count -ne 1){throw 'Expected original deployment gate missing'}
    . ([scriptblock]::Create($function[0].Extent.Text))
}
$source='3fa11f940b18e54b46e6b927d3d66294bea37088'
$manifestPin='424e70b8b0acf847598f2bd7b5aefb4276bad052ab2afb64f7e8412d60d9e1bb'
$nodePin='5a471fc805f86aaf57d6b06a4cfc1f4ba8aacff7eea67c74fabe036718a27b27'
$root=Join-Path $PSScriptRoot ('../../target/relay-evidence-20261109/gate-tests-'+[Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $root|Out-Null
$json=Join-Path $root 'record.json'
Write-NewJson $json ([ordered]@{test=$true;start='existing'})
if((Get-Content -LiteralPath $json -Raw|ConvertFrom-Json).start -cne 'existing'){throw 'New evidence did not round-trip'}
$rejected=$false
try{Write-NewJson $json @{test=$false}}catch [IO.IOException]{$rejected=$true}
if(!$rejected -or !(Get-Content -LiteralPath $json -Raw|ConvertFrom-Json).test){throw 'Existing evidence must remain intact'}
Assert-NoRedirect $json
$bad=Join-Path $root 'wrong-candidate'
New-Item -ItemType Directory -Path $bad|Out-Null
Write-NewJson (Join-Path $bad 'release-manifest.json') @{git_commit='wrong'}
$rejected=$false
try{Assert-Package $bad}catch{if($_.Exception.Message -cne 'Fixed manifest mismatch'){throw};$rejected=$true}
if(!$rejected){throw 'Wrong candidate accepted'}
$rejected=$false
try{& $path -HostId B -Owner}catch [Management.Automation.ParameterBindingException]{$rejected=$true}
if(!$rejected){throw 'B must not be in this update scope'}
$commands=@($ast.FindAll({param($node) $node -is [Management.Automation.Language.CommandAst]},$true))
foreach($command in $commands) {
    if($command.GetCommandName() -in @('Remove-Item','Initialize-Disk','Set-NetFirewallProfile')){throw 'Forbidden state/profile mutation added'}
    if($command.Extent.Text -match 'native-genesis\s+initialize'){throw 'Initialization must not be callable'}
}
if($PackageRoot){Assert-Package $PackageRoot}
Write-Output 'PASS 7 deployment gates: syntax, new-only evidence, duplicate preservation, real path, bad package, host scope, forbidden actions'
if($PackageRoot){Write-Output 'PASS actual fixed Windows package through the same deployment gate'}

function New-FilterFixture([string]$Name) {
    $values=[ordered]@{
        InstanceID=$Name
        CreationClassName=('MSFT|FW|FirewallRule|'+$Name)
        SystemCreationClassName='MSFT_NetFirewallRule'
        Protocol='UDP'
        LocalPort=@('41051','42051')
        RemotePort=@('Any')
        LocalAddress=@('192.168.71.74')
        RemoteAddress=@('192.168.71.0/255.255.255.0','47.103.59.206')
        InterfaceAlias=@('Any')
        InterfaceType=@('Any')
        LocalUser='Any'
        ServiceName='Any'
    }
    $properties=@($values.Keys|ForEach-Object {[pscustomobject]@{Name=$_}})
    $values.CimClass=[pscustomobject]@{CimClassName='fixture_filter';CimClassProperties=$properties}
    return [pscustomobject]$values
}
$oldFilter=New-FilterFixture 'old'
$newFilter=New-FilterFixture 'new'
Assert-FirewallFilterScope $oldFilter $newFilter 'old' 'new'
$cases=@{
    LocalPort=@('Any')
    RemotePort=@('1234')
    LocalAddress=@('Any')
    RemoteAddress=@('Any')
    Protocol='TCP'
    InterfaceAlias=@('Other')
    InterfaceType=@('Wireless')
    LocalUser='Other'
    ServiceName='Other'
    SystemCreationClassName='Other'
}
foreach($name in $cases.Keys) {
    $changed=New-FilterFixture 'new'
    $changed.$name=$cases[$name]
    $rejected=$false
    try{Assert-FirewallFilterScope $oldFilter $changed 'old' 'new'}catch{if($_.Exception.Message -cne 'Copied firewall filter scope differs'){throw};$rejected=$true}
    if(!$rejected){throw "Changed $name must fail"}
}
$newFilter=New-FilterFixture 'wrong-rule'
$rejected=$false
try{Assert-FirewallFilterScope $oldFilter $newFilter 'old' 'new'}catch{if($_.Exception.Message -cne 'Copied firewall filter rule association differs'){throw};$rejected=$true}
if(!$rejected){throw 'Wrong associated rule must fail'}
$newFilter=New-FilterFixture 'new'
$newFilter.CimClass.CimClassName='wrong-class'
$rejected=$false
try{Assert-FirewallFilterScope $oldFilter $newFilter 'old' 'new'}catch{if($_.Exception.Message -cne 'Copied firewall filter class differs'){throw};$rejected=$true}
if(!$rejected){throw 'Wrong filter class must fail'}
Write-Output 'PASS 13 firewall cases: renamed association accepted; 10 semantic changes, wrong association and wrong class rejected'

if($InspectPreparedFirewall) {
    $oldRule=Get-NetFirewallRule -Name 'NOVOVM-5181da73-A-20261109'
    $newRule=Get-NetFirewallRule -Name 'NOVOVM-883c6183-A-20261109'
    foreach($command in @('Get-NetFirewallPortFilter','Get-NetFirewallAddressFilter','Get-NetFirewallInterfaceFilter','Get-NetFirewallInterfaceTypeFilter','Get-NetFirewallSecurityFilter','Get-NetFirewallServiceFilter')) {
        Assert-FirewallFilterScope (& $command -AssociatedNetFirewallRule $oldRule) (& $command -AssociatedNetFirewallRule $newRule) $oldRule.Name $newRule.Name
    }
    Write-Output 'PASS 6 actual prepared Windows firewall filter comparisons; read-only, no rule changes'
}

function New-CrashFixture {
    [pscustomobject]@{task_state='Ready';task_sid='S-1-5-19';restart_count=0;trigger_count=0;task_exit_code=3221225477;application='novovm-node.exe';application_path='C:\fixed\novovm-node.exe';exception_code='c0000005';fault_pid=23024;last_run_utc=[DateTime]::UtcNow.AddHours(-4);created_utc=[DateTime]::UtcNow.AddHours(-4).AddSeconds(1);event_utc=[DateTime]::UtcNow.AddHours(-2)}
}
Assert-CrashedOwnerEvidence (New-CrashFixture) 'C:\fixed\novovm-node.exe'
$changes=@{task_state='Running';task_sid='S-1-5-18';restart_count=1;trigger_count=1;task_exit_code=1;application='other.exe';application_path='C:\other\novovm-node.exe';exception_code='other';fault_pid=0;created_utc=[DateTime]::UtcNow.AddHours(-5);event_utc=[DateTime]::UtcNow.AddHours(-6)}
foreach($name in $changes.Keys) {
    $fixture=New-CrashFixture;$fixture.$name=$changes[$name];$rejected=$false
    try{Assert-CrashedOwnerEvidence $fixture 'C:\fixed\novovm-node.exe'}catch{$rejected=$true}
    if(!$rejected){throw "Invalid crashed owner $name accepted"}
}
Write-Output 'PASS 12 crashed-owner gates: exact crash accepted; live task, authority, restart, trigger, result, application, path, exception, pid and timeline counterexamples rejected'
$receipt=Join-Path $PSScriptRoot '../../artifacts/audit/validator-c-5181-update-preflight-20261109/application-crash-events.json'
if(Test-Path -LiteralPath $receipt) {
    $record=Get-Content -LiteralPath $receipt -Raw|ConvertFrom-Json
    $fields=@{};foreach($field in $record.fields){$fields[$field.name]=$field.value}
    $reported=New-CrashFixture
    $reported.application=$fields.AppName;$reported.application_path=$fields.AppPath;$reported.exception_code=$fields.ExceptionCode
    $reported.fault_pid=[Convert]::ToInt32($fields.ProcessId,16)
    $reported.created_utc=[DateTime]::FromFileTimeUtc([Convert]::ToInt64($fields.ProcessCreationTime,16))
    $reported.event_utc=[DateTime]::Parse($record.utc).ToUniversalTime()
    $reported.last_run_utc=[DateTime]::Parse('2026-11-08T10:30:13+08:00').ToUniversalTime()
    Assert-CrashedOwnerEvidence $reported 'C:\ProgramData\NOVOVM\releases\novonet-probe-ddd89e8e-windows-x86_64\bin\novovm-node.exe'
    Write-Output 'PASS actual C crash receipt through original crash-evidence gate; not a live C process-state qualification'

    # Rebuild XML from every original public field, including the two empty package fields.
    $eventXml=[xml]'<Event><EventData /></Event>'
    $eventData=$eventXml.SelectSingleNode('/Event/EventData')
    foreach($field in $record.fields) {
        $node=$eventXml.CreateElement('Data')
        $node.SetAttribute('Name',$field.name)
        if($null -ne $field.value){$node.InnerText=[string]$field.value}
        $null=$eventData.AppendChild($node)
    }
    $oldRejected=$false
    try {
        foreach($field in $eventXml.Event.EventData.Data){$null=[string]$field.'#text'}
    }catch [Management.Automation.PropertyNotFoundException]{$oldRejected=$true}
    if(!$oldRejected){throw 'Original strict XML parser must reproduce C empty-field failure'}
    $expected='C:\ProgramData\NOVOVM\releases\novonet-probe-ddd89e8e-windows-x86_64\bin\novovm-node.exe'
    $converted=ConvertFrom-NovonetCrashEvent $eventXml ([DateTime]::Parse($record.utc).ToUniversalTime()) $record.record_id $expected
    if($converted.fault_pid -ne 23024 -or $converted.application_path -cne $expected -or $converted.report_id -cne '5e688e6e-3c03-4ea0-b514-01716e847494' -or $converted.created_utc -ne $reported.created_utc){throw 'Original complete C event did not retain required identity/timing fields'}
    $converted|Add-Member -NotePropertyMembers @{task_state='Ready';task_sid='S-1-5-19';restart_count=0;trigger_count=0;task_exit_code=3221225477;last_run_utc=$reported.last_run_utc}
    Assert-CrashedOwnerEvidence $converted $expected
    if(@(ConvertFrom-NovonetCrashEvent $eventXml $reported.event_utc $record.record_id 'C:\other\novovm-node.exe').Count){throw 'Unrelated application event must not qualify'}
    $bad=$eventXml.CloneNode($true)
    foreach($field in $bad.Event.EventData.Data){if($field.GetAttribute('Name') -ceq 'ExceptionCode'){$field.InnerText='other'}}
    if(@(ConvertFrom-NovonetCrashEvent $bad $reported.event_utc $record.record_id $expected).Count){throw 'Unrelated exception must not qualify'}
    $bad=$eventXml.CloneNode($true)
    foreach($field in $bad.Event.EventData.Data){if($field.GetAttribute('Name') -ceq 'ProcessId'){$field.InnerText=''}}
    $rejected=$false
    try{$null=ConvertFrom-NovonetCrashEvent $bad $reported.event_utc $record.record_id $expected}catch{if($_.Exception.Message -cne 'Matching crash event missing ProcessId'){throw};$rejected=$true}
    if(!$rejected){throw 'Required empty process identity must still fail'}
    $namespaced=[xml]$eventXml.OuterXml.Replace('<Event>','<Event xmlns="http://schemas.microsoft.com/win/2004/08/events/event">')
    $convertedNs=ConvertFrom-NovonetCrashEvent $namespaced $reported.event_utc $record.record_id $expected
    if($convertedNs.fault_pid -ne 23024 -or $convertedNs.report_id -cne $fields.IntegratorReportId){throw 'Windows event XML namespace must not hide required fields'}
    $unrelated=[xml]'<Event><EventData><Data Name="Other">unrelated provider</Data></EventData></Event>'
    if(@(ConvertFrom-NovonetCrashEvent $unrelated $reported.event_utc 1 $expected).Count){throw 'Unrelated provider without application fields must not qualify'}
    $pipelineEvent=[pscustomobject]@{TimeCreated=$reported.event_utc;RecordId=$record.record_id}
    $pipelineEvent|Add-Member -MemberType ScriptMethod -Name ToXml -Value {$namespaced.OuterXml}
    function Get-WinEvent {param($FilterHashtable) if($FilterHashtable.Id -ne 1000){throw 'Unexpected event filter'}; $pipelineEvent}
    $info=[pscustomobject]@{LastRunTime=$reported.last_run_utc};$oldExe=$expected
    $assignment=@($ast.FindAll({param($node) $node -is [Management.Automation.Language.AssignmentStatementAst] -and $node.Left.Extent.Text -ceq '$crashes'},$true))
    if($assignment.Count -ne 1){throw 'Actual original crash-reading pipeline missing'}
    . ([scriptblock]::Create($assignment[0].Extent.Text))
    if($crashes.Count -ne 1 -or $crashes[0].fault_pid -ne 23024 -or $crashes[0].record_id -ne 545045){throw 'Actual full Get-WinEvent to converter/sort pipeline differs'}
    Write-Output 'PASS 8 complete XML regression cases: old parser fails; complete C XML, Windows namespace and actual reader pipeline pass; unrelated events and empty required pid reject'
}

function New-StoppedFixture {
    [pscustomobject]@{task_state='Ready';task_sid='S-1-5-19';restart_count=0;trigger_count=0;task_exit_code=0;executing_nodes=0;bound_ports=0;last_run_utc=[DateTime]::UtcNow.AddHours(-25);progress_utc=[DateTime]::UtcNow.AddHours(-1)}
}
Assert-StoppedOwnerEvidence (New-StoppedFixture)
foreach($case in @{task_state='Running';task_sid='S-1-5-18';restart_count=1;trigger_count=1;task_exit_code=3221225477;executing_nodes=1;bound_ports=1;last_run_utc=[DateTime]::UtcNow.AddMinutes(1);progress_utc=[DateTime]::UtcNow.AddHours(-26)}.GetEnumerator()) {
    $fixture=New-StoppedFixture;$fixture.($case.Key)=$case.Value;$rejected=$false
    try{Assert-StoppedOwnerEvidence $fixture}catch{$rejected=$true}
    if(!$rejected){throw "Unproven clean stop accepted: $($case.Key)"}
}
Write-Output 'PASS 10 clean-stop gates: proven normal stop accepted; active, authority, restart, trigger, failure, live process/port and timeline counterexamples rejected'

$interrupted=New-StoppedFixture
$interrupted.task_exit_code=267014
Assert-StoppedOwnerEvidence $interrupted $true
foreach($case in @{task_state='Running';task_sid='S-1-5-18';restart_count=1;trigger_count=1;task_exit_code=0;executing_nodes=1;bound_ports=1;last_run_utc=[DateTime]::UtcNow.AddMinutes(1);progress_utc=[DateTime]::UtcNow.AddHours(-26)}.GetEnumerator()) {
    $fixture=$interrupted.PSObject.Copy();$fixture.($case.Key)=$case.Value;$rejected=$false
    try{Assert-StoppedOwnerEvidence $fixture $true}catch{$rejected=$true}
    if(!$rejected){throw "Invalid interrupted owner accepted: $($case.Key)"}
}
$rejected=$false
try{Assert-StoppedOwnerEvidence $interrupted}catch{$rejected=$true}
if(!$rejected){throw 'Interrupted task must not qualify as a clean shutdown'}
# Exercise the actual controller branch against the returned D record, without
# calling the updater or touching a task/database. Absence must be checked live again.
$receipt=Join-Path $PSScriptRoot '../../artifacts/audit/validator-d-8fed-preflight-stopped-20261111/preflight-public.json'
if(Test-Path -LiteralPath $receipt) {
    $actual=Get-Content -LiteralPath $receipt -Raw|ConvertFrom-Json
    $actual|Add-Member -NotePropertyMembers @{executing_nodes=@($actual.active_nodes).Count;bound_ports=@($actual.listeners).Count}
    $actual.last_run_utc=[DateTime]$actual.last_run_utc;$actual.progress_utc=[DateTime]$actual.progress_utc
    Assert-StoppedOwnerEvidence $actual $true
    Write-Output 'PASS actual D interrupted receipt; not a live D startup qualification'
}
Write-Output 'PASS 11 interrupted-owner gates: exact terminated task accepted; 9 unsafe states and clean-shutdown misclassification rejected'

$modeGuards=@($ast.EndBlock.Statements|Where-Object {
    $_ -is [Management.Automation.Language.IfStatementAst] -and
    ($_.Extent.Text -match "throw 'Choose one old-owner recovery path'" -or $_.Extent.Text -match "throw 'Interrupted recovery requires original progress and a fresh update'")
})
if($modeGuards.Count -ne 2){throw 'Actual recovery mode guards missing'}
$RecoverInterrupted=$true
foreach($mode in @('none','RecoverCrashed','ResumeStopped','RecoverMissingProgress','ResumePrepared','Rollback')) {
    $RecoverCrashed=$false;$ResumeStopped=$false;$RecoverMissingProgress=$false;$ResumePrepared=$false;$Rollback=$false
    if($mode -ne 'none'){Set-Variable -Name $mode -Value $true}
    $rejected=$false
    try{foreach($guard in $modeGuards){. ([scriptblock]::Create($guard.Extent.Text))}}catch{$rejected=$true}
    if($rejected -ne ($mode -ne 'none')){throw "Recovery mode guard differs: $mode"}
}
Write-Output 'PASS 6 actual mode-guard cases: interrupted alone accepted; conflicting recovery/rollback modes rejected'

$portFixture=[pscustomobject]@{Protocol='UDP';LocalPort=@('41051','42051');RemotePort='Any'}
$addressFixture=[pscustomobject]@{LocalAddress='192.168.71.74';RemoteAddress=@('192.168.71.0/255.255.255.0','47.103.59.206')}
$ruleFixture=[pscustomobject]@{Enabled='True';Action='Allow';Direction='Inbound'}
$appFixture=[pscustomobject]@{Program='C:\fixed\novovm-node.exe'}
Assert-OriginalFirewallScope $ruleFixture $portFixture $appFixture $addressFixture $appFixture.Program @(41051,42051) '192.168.71.74'
foreach($case in @(@('port','LocalPort',@('Any')),@('port','RemotePort',@('1234')),@('address','LocalAddress','Any'),@('address','RemoteAddress',@('Any')),@('address','RemoteAddress',@('192.168.71.0/24','1.1.1.1')),@('rule','Direction','Outbound'))) {
    $portCopy=$portFixture.PSObject.Copy();$addressCopy=$addressFixture.PSObject.Copy();$ruleCopy=$ruleFixture.PSObject.Copy()
    switch($case[0]){'port'{$portCopy.($case[1])=$case[2]} 'address'{$addressCopy.($case[1])=$case[2]} 'rule'{$ruleCopy.($case[1])=$case[2]}}
    $rejected=$false
    try{Assert-OriginalFirewallScope $ruleCopy $portCopy $appFixture $addressCopy $appFixture.Program @(41051,42051) '192.168.71.74'}catch{$rejected=$true}
    if(!$rejected){throw 'Changed original scope must not be cloned'}
}
Write-Output 'PASS 7 original-scope gates: exact current scope accepted; port/address expansion and wrong direction rejected'

$names=@('$state','$oldWork','$work','$oldRelease','$release','$oldTask','$newTask','$rollbackTask','$oldNodePin','$oldProgressPath','$oldRuleName')
$assignments=@($ast.EndBlock.Statements|Where-Object {$_ -is [Management.Automation.Language.AssignmentStatementAst] -and $_.Left.Extent.Text -in $names})
foreach($HostId in @('A','C','D')) {
    foreach($assignment in $assignments){. ([scriptblock]::Create($assignment.Extent.Text))}
    if($oldTask -ceq $newTask -or $oldRelease -ceq $release -or $oldWork -ceq $work){throw 'Old/new deployment paths alias'}
    if($HostId -ceq 'D') {
        if($oldNodePin -cne '9040ac63828c30011e8e7c4801be31183ecb69960854dd4974671df3d5d14d51' -or $oldTask -cne 'NOVOVM-D-08236129-Network-20261108' -or $oldProgressPath -cne "$oldWork\progress.json"){throw 'D original 082 mapping differs'}
    }elseif($oldNodePin -cne 'ff68c5930b9b205b25ba5351a17ad08aa68f51d7cf71779d0507b6269f4087f3' -or $oldProgressPath -cne "$oldWork\logs\progress.json"){throw 'A/C original 5181 mapping differs'}
}
$network=[pscustomobject]@{discovery=$true;relay_forwarding=$true}
$probeAssignment=@($ast.FindAll({param($node) $node -is [Management.Automation.Language.PipelineAst] -and $node.Extent.Text.StartsWith('$network|Add-Member -NotePropertyName probe_control_file')},$true))
if($probeAssignment.Count -ne 1){throw 'Original optional field adaptation missing'}
. ([scriptblock]::Create($probeAssignment[0].Extent.Text))
if($network.probe_control_file -cne "$work\commands\control.nvprobe" -or !$network.discovery -or !$network.relay_forwarding){throw '082 field adaptation changes routing or command owner'}
Write-Output 'PASS 4 actual source adaptations: A/C/D old-version mappings and missing 082 optional field'

$rollbackLogs=Join-Path $root 'rollback-logs'
New-Item -ItemType Directory -Path $rollbackLogs|Out-Null
Assert-RollbackReady $rollbackLogs $false
$rejected=$false
try{Assert-RollbackReady $rollbackLogs $true}catch{$rejected=$true}
if(!$rejected){throw 'Existing rollback task accepted'}
Write-NewJson (Join-Path $rollbackLogs 'owner.json') @{historical=$true}
$rejected=$false
try{Assert-RollbackReady $rollbackLogs $false}catch{$rejected=$true}
if(!$rejected){throw 'Prior rollback evidence accepted for overwrite'}
Write-Output 'PASS 3 rollback-readiness gates: fresh logs accepted; existing task or log rejected before process stop'

$lossLogs=Join-Path $root 'observation-loss'
New-Item -ItemType Directory -Path $lossLogs|Out-Null
[IO.File]::WriteAllBytes((Join-Path $lossLogs 'progress.json'),[byte[]]::new(813127))
[IO.File]::WriteAllBytes((Join-Path $lossLogs 'node.stdout'),[byte[]]::new(0))
Assert-MissingProgressRecovery 'C' $true $false $false $lossLogs
foreach($argsCase in @(@('A',$true,$false,$false),@('D',$true,$false,$false),@('C',$false,$false,$false),@('C',$true,$true,$false),@('C',$true,$false,$true))) {
    $rejected=$false
    try{Assert-MissingProgressRecovery $argsCase[0] $argsCase[1] $argsCase[2] $argsCase[3] $lossLogs}catch{$rejected=$true}
    if(!$rejected){throw 'Unreviewed recovery mode accepted'}
}
Write-NewJson (Join-Path $lossLogs 'exit.json') @{exit_code=0}
$rejected=$false
try{Assert-MissingProgressRecovery 'C' $true $false $false $lossLogs}catch{$rejected=$true}
if(!$rejected){throw 'Changed exit evidence accepted'}
$different=Join-Path $root 'different-loss'
New-Item -ItemType Directory -Path $different|Out-Null
[IO.File]::WriteAllBytes((Join-Path $different 'progress.json'),[byte[]]::new(813126))
$rejected=$false
try{Assert-MissingProgressRecovery 'C' $true $false $false $different}catch{$rejected=$true}
if(!$rejected){throw 'Unmatched progress loss accepted'}
Write-Output 'PASS 8 observation-loss scope gates; exact C bytes required; other hosts, lifecycle modes and changed evidence rejected'
$recovered=[pscustomobject]@{entry='original_native_execution_tick';chain_id=91129655606029;current_height=1;head=$null;execution_failures=0;recovery_in_progress=$false}
Assert-RecoveredProgress $recovered $null $true
Assert-RecoveredProgress $recovered $recovered $false
foreach($case in @{entry='other';chain_id=1;current_height=-1;execution_failures=1;recovery_in_progress=$true}.GetEnumerator()){
    $bad=$recovered.PSObject.Copy();$bad.($case.Key)=$case.Value;$rejected=$false
    try{Assert-RecoveredProgress $bad $null $true}catch{$rejected=$true}
    if(!$rejected){throw 'Invalid recovery projection accepted'}
}
foreach($field in @('entry','chain_id','current_height','head','execution_failures','recovery_in_progress')){
    $bad=$recovered.PSObject.Copy();$bad.PSObject.Properties.Remove($field);$rejected=$false
    try{Assert-RecoveredProgress $bad $null $true}catch{$rejected=$true}
    if(!$rejected){throw 'Missing recovered field accepted'}
}
foreach($tuple in @(@($recovered,$recovered,$true),@($recovered,$null,$false),@($null,$null,$true))){
    $rejected=$false
    try{Assert-RecoveredProgress $tuple[0] $tuple[1] $tuple[2]}catch{$rejected=$true}
    if(!$rejected){throw 'Invented or missing prior state accepted'}
}
$changed=$recovered.PSObject.Copy();$changed.current_height=2
$rejected=$false
try{Assert-RecoveredProgress $changed $recovered $false}catch{$rejected=$true}
if(!$rejected){throw 'Normal update state comparison weakened'}
Write-Output 'PASS 17 recovered-state gates; genesis null head allowed; absent prior state never certified unchanged'
