#Requires -Version 7.0
#Requires -RunAsAdministrator
# Same installed owner/task, same database; preserve the failed run before restart.
param([Parameter(Mandatory)][string]$EvidenceRoot)
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$work='C:\ProgramData\NOVOVM\selfrun\91129655606029\C\novonet-observation-875d3307-20261111'
$release='C:\ProgramData\NOVOVM\releases\novonet-observation-875d3307-windows-x86_64'
$taskName='NOVOVM-C-875d3307-Existing-20261111'
$exe=Join-Path $release 'bin\novovm-node.exe'
$owner=Join-Path $work 'ExistingOwner.ps1'
$logs=Join-Path $work 'logs'
$out=[IO.Path]::GetFullPath($EvidenceRoot)

function Assert-C875Task($Task,$Info,[string]$OwnerPath) {
    $sid=if($Task.Principal.UserId.StartsWith('S-1-')){$Task.Principal.UserId}else{([Security.Principal.NTAccount]::new($Task.Principal.UserId)).Translate([Security.Principal.SecurityIdentifier]).Value}
    $expected='-NoProfile -NonInteractive -ExecutionPolicy RemoteSigned -File "'+$OwnerPath+'" -HostId C -Owner'
    $actions=@($Task.Actions)
    if($Task.TaskName -cne 'NOVOVM-C-875d3307-Existing-20261111' -or $Task.State.ToString() -cne 'Ready' -or $sid -cne 'S-1-5-19' -or [int]$Task.Settings.RestartCount -ne 0 -or @($Task.Triggers|Where-Object {$null -ne $_}).Count -ne 0 -or [long]$Info.LastTaskResult -ne 3221225477){throw 'Current C875 crashed task identity differs'}
    if($actions.Count -ne 1 -or $actions[0].Execute -ine 'C:\Program Files\PowerShell\7\pwsh.exe' -or $actions[0].Arguments -cne $expected){throw 'Current task is not the fixed existing owner'}
}
function Assert-C875Exit($Exit,$Crash) {
    if($Exit.start -cne 'existing' -or ([long]$Exit.exit_code -band 4294967295L) -ne 3221225477 -or $Crash.exception_code -cne 'c0000005' -or $Crash.fault_pid -ne 31296){throw 'C875 task/event/exit do not match the reported run'}
    $when=([DateTime]$Exit.time).ToUniversalTime()
    if($when -lt $Crash.event_utc.AddSeconds(-5) -or $when -gt [DateTime]::UtcNow.AddSeconds(5)){throw 'C875 exit/event timeline differs'}
}
function Move-C875Logs([string]$WorkRoot,[string]$SavedLogs) {
    $resolved=[IO.Path]::GetFullPath($WorkRoot).TrimEnd('\')
    $original=Join-Path $resolved 'logs'
    $destination=[IO.Path]::GetFullPath($SavedLogs)
    if([IO.Path]::GetDirectoryName($destination) -ine $resolved -or [IO.Path]::GetFileName($destination) -cnotmatch '^logs-crash-875-[0-9a-f]{32}$'){throw 'Crash log destination outside exact work directory'}
    Assert-NoRedirect $original
    if(Test-Path -LiteralPath $destination){throw 'Crash evidence destination already exists'}
    # Check every descendant before a recursive directory move; no junctions.
    foreach($item in Get-ChildItem -LiteralPath $original -Recurse -Force){if($item.Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Redirected log descendant'}}
    $acl=Get-Acl -LiteralPath $original
    Move-Item -LiteralPath $original -Destination $destination
    New-Item -ItemType Directory -Path $original|Out-Null
    Set-Acl -LiteralPath $original -AclObject $acl
}

if(Test-Path -LiteralPath $out){throw 'Fresh evidence directory required'}
if(!(Test-Path -LiteralPath (Split-Path $out -Parent) -PathType Container)){throw 'Evidence parent must already exist'}
# Import only the audited functions and fixed package constants, not its controller.
if((Get-FileHash -LiteralPath $owner).Hash.ToLowerInvariant() -cne 'f7c185500abd955089c92b8bd0edefb3cf20a20df65bfbfe53c1c9c0bcd55cb2'){throw 'Installed875 owner differs; no recovery performed'}
$tokens=$null;$errors=$null
$ast=[Management.Automation.Language.Parser]::ParseFile($owner,[ref]$tokens,[ref]$errors)
if($errors.Count){throw 'Installed owner parse failure'}
foreach($name in @('Write-NewJson','Assert-NoRedirect','Assert-Package','Get-CustodyHashes','Assert-CrashedOwnerEvidence','ConvertFrom-NovonetCrashEvent','Assert-RecoveredProgress')){
    $def=@($ast.FindAll({param($n) $n -is [Management.Automation.Language.FunctionDefinitionAst]},$false)|Where-Object Name -CEQ $name)
    if($def.Count -ne 1){throw 'Original recovery function missing'}
    . ([scriptblock]::Create($def[0].Extent.Text))
}
foreach($name in @('$source','$manifestPin','$nodePin','$corePin')){
    $assignment=@($ast.EndBlock.Statements|Where-Object {$_ -is [Management.Automation.Language.AssignmentStatementAst] -and $_.Left.Extent.Text -ceq $name})
    if($assignment.Count -ne 1){throw 'Original fixed package constant missing'}
    . ([scriptblock]::Create($assignment[0].Extent.Text))
}
Assert-NoRedirect (Split-Path $out -Parent)
Assert-NoRedirect $work
Assert-NoRedirect $owner
Assert-Package $release
$processHelper=Join-Path $PSScriptRoot 'NovonetExistingProcess.ps1'
$processText=(Get-Content -Raw -LiteralPath $processHelper).Replace("`r`n","`n")
if([Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes($processText))).ToLowerInvariant() -cne '659d2cdecf3e5ca0e58472a25c9f85b4f85a05095fca7b0cc0fc4277077f7b66'){throw 'Original process barrier differs'}
. $processHelper
New-Item -ItemType Directory -Path $out|Out-Null
$started=$false;$saved=$null
try {
    $task=Get-ScheduledTask -TaskName $taskName
    $info=Get-ScheduledTaskInfo -TaskName $taskName
    Assert-C875Task $task $info $owner
    $execution=Get-NovonetNodeExecutionState
    Write-NewJson "$out/process-before-public.json" $execution
    if(@($execution.active).Count -or @(Get-NetUDPEndpoint|Where-Object LocalPort -in 41052,42052).Count){throw 'Node or port still active; nothing stopped'}
    Export-ScheduledTask -TaskName $taskName|Set-Content -LiteralPath "$out/task-before.xml" -Encoding utf8
    $events=@(Get-WinEvent -FilterHashtable @{LogName='Application';Id=1000;StartTime=$info.LastRunTime} -MaxEvents 256)
    $matched=@(foreach($event in $events){
        $entry=ConvertFrom-NovonetCrashEvent ([xml]$event.ToXml()) $event.TimeCreated.ToUniversalTime() $event.RecordId $exe
        if($null -ne $entry){[pscustomobject]@{summary=$entry;event=$event}}
    })
    if($matched.Count -eq 0){throw 'Matching Application1000 crash event missing; no restart'}
    $match=$matched|Sort-Object {$_.summary.event_utc} -Descending|Select-Object -First 1
    $crash=$match.summary
    $crash|Add-Member -NotePropertyMembers @{task_state=$task.State.ToString();task_sid='S-1-5-19';restart_count=[int]$task.Settings.RestartCount;trigger_count=@($task.Triggers|Where-Object {$null -ne $_}).Count;task_exit_code=[long]$info.LastTaskResult;last_run_utc=$info.LastRunTime.ToUniversalTime()}
    $match.event.ToXml()|Set-Content -LiteralPath "$out/crash-event-local.xml" -Encoding utf8
    $exit=Get-Content -Raw -LiteralPath "$logs/exit.json"|ConvertFrom-Json
    Assert-CrashedOwnerEvidence $crash $exe $exit
    Assert-C875Exit $exit $crash
    $fields=@{}
    foreach($field in ([xml]$match.event.ToXml()).SelectNodes("/*[local-name()='Event']/*[local-name()='EventData']/*[local-name()='Data']")){$fields[$field.GetAttribute('Name')]=[string]$field.InnerText}
    Write-NewJson "$out/crash-public.json" ([ordered]@{host='C';source=$source;crash=$crash;exit=$exit;fault_module=$fields['ModuleName'];fault_module_version=$fields['ModuleVersion'];fault_offset=$fields['FaultingOffset'];root_cause_known=$false})
    $configPath=Join-Path $work 'run.json'
    $networkPath=Join-Path $work 'network.json'
    Assert-NoRedirect $configPath;Assert-NoRedirect $networkPath;Assert-NoRedirect $logs
    $config=Get-Content -Raw -LiteralPath $configPath|ConvertFrom-Json
    $network=Get-Content -Raw -LiteralPath $networkPath|ConvertFrom-Json
    if($config.database -ine 'C:\ProgramData\NOVOVM\selfrun\91129655606029\C\chain-state' -or $config.library -ine "$release\aoem\windows\core\bin\aoem_ffi.dll" -or $config.novonet_config_file -ine $networkPath){throw 'Original library/database/network paths differ'}
    Assert-NoRedirect $config.database
    if((Get-FileHash -LiteralPath $config.library).Hash.ToLowerInvariant() -cne $corePin){throw 'Original AOEM hash differs'}
    if(Test-Path -LiteralPath $network.probe_control_file){throw 'Pending diagnostic command; preserve and inspect before recovery'}
    $custody=Get-CustodyHashes $config $network
    $configHash=(Get-FileHash -LiteralPath $configPath).Hash
    $networkHash=(Get-FileHash -LiteralPath $networkPath).Hash
    $prior=Get-Content -Raw -LiteralPath "$logs/progress.json"|ConvertFrom-Json
    Assert-RecoveredProgress $prior $prior $false
    # Logs are moved, never a live database backup. All state/journal/cache stays put.
    $saved=Join-Path $work ('logs-crash-875-'+[Guid]::NewGuid().ToString('N'))
    Write-NewJson "$out/before-public.json" ([ordered]@{host='C';source=$source;database=$config.database;task=$taskName;node_sha256=$nodePin;aoem_sha256=$corePin;original_logs=$logs;preserved_logs=$saved;config_sha256=$configHash;network_sha256=$networkHash;current_height=$prior.current_height;head=$prior.head;start='existing'})
    Assert-C875Task (Get-ScheduledTask -TaskName $taskName) (Get-ScheduledTaskInfo -TaskName $taskName) $owner
    Wait-NovonetNodeExit
    if(@(Get-NetUDPEndpoint|Where-Object LocalPort -in 41052,42052).Count){throw 'Port owner appeared during preflight'}
    Move-C875Logs $work $saved
    Write-NewJson "$out/start-once.json" @{utc=[DateTime]::UtcNow.ToString('o');task=$taskName;start='existing';attempts=1}
    Start-ScheduledTask -TaskName $taskName
    $started=$true
    $wait=[Diagnostics.Stopwatch]::StartNew();$current=$null;$node=$null
    do {
        if(Test-Path -LiteralPath "$logs/exit.json"){throw 'Recovered owner exited; no automatic retry'}
        $nodes=@((Get-NovonetNodeExecutionState).active)
        if($nodes.Count -gt 1){throw 'Multiple nodes after recovery'}
        if($nodes.Count -eq 1){
            $node=$nodes[0]
            if($node.ExecutablePath -ine $exe -or (Invoke-CimMethod $node -MethodName GetOwnerSid).Sid -cne 'S-1-5-19'){throw 'Recovered process owner/path differs'}
            if(Test-Path -LiteralPath "$logs/progress.json"){
                try {$current=Get-Content -Raw -LiteralPath "$logs/progress.json"|ConvertFrom-Json}catch{$current=$null}
                if($null -ne $current){
                    Assert-RecoveredProgress $current $prior $false
                    $ports=@(Get-NetUDPEndpoint|Where-Object {$_.OwningProcess -eq $node.ProcessId -and $_.LocalPort -in 41052,42052})
                    if($ports.Count -eq 2 -and ([DateTime]::UtcNow-(Get-Item "$logs/progress.json").LastWriteTimeUtc).TotalSeconds -le 15){break}
                }
            }
        }
        Start-Sleep -Milliseconds 250
    }while($wait.Elapsed.TotalSeconds -lt 30)
    if($wait.Elapsed.TotalSeconds -ge 30){throw 'Recovery health not confirmed in original30-second observation bound'}
    if((Get-FileHash $configPath).Hash -cne $configHash -or (Get-FileHash $networkPath).Hash -cne $networkHash){throw 'Configuration changed during recovery'}
    $afterKeys=Get-CustodyHashes $config $network
    foreach($key in $custody.Keys){if($afterKeys[$key] -cne $custody[$key]){throw 'Custody changed during recovery'}}
    $loaded=@((Get-Process -Id $node.ProcessId).Modules|Where-Object FileName -IEQ $config.library)
    if($loaded.Count -ne 1){throw 'Actual AOEM module not confirmed'}
    Write-NewJson "$out/result-public.json" ([ordered]@{status='C_875_RECOVERED_READY_FOR_A_QUERY';utc=[DateTime]::UtcNow.ToString('o');host='C';source=$source;pid=$node.ProcessId;sid='S-1-5-19';node_sha256=$nodePin;aoem_sha256=$corePin;database=$config.database;start='existing';current_height=$current.current_height;head=$current.head;execution_failures=$current.execution_failures;ports=@($ports|Select-Object LocalAddress,LocalPort,OwningProcess);custody_unchanged=$true;configuration_unchanged=$true;preserved_logs=$saved;initialize_executed=$false;new_object_sent=$false;crash_root_cause_resolved=$false;external_authenticated_query_required=$true})
}catch{
    Write-NewJson "$out/error-public.json" ([ordered]@{utc=[DateTime]::UtcNow.ToString('o');error=$_.Exception.Message;start_requested=$started;preserved_logs=$saved;retry_performed=$false;initialize_executed=$false;node_stopped_by_helper=$false})
    throw
}
