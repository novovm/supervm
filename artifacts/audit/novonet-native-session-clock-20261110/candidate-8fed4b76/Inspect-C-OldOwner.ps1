#Requires -Version 7.0
# Read only the exact original C owner; no update, service or database operation.
param([Parameter(Mandatory)][string]$EvidenceRoot)
$ErrorActionPreference='Stop'
$principal=[Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())
if(!$principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)){throw 'Administrator PowerShell required'}
$repo=(Resolve-Path (Join-Path $PSScriptRoot '../../../..')).Path
$inspection=Join-Path $repo 'artifacts/audit/novonet-seed-retention-20261110/candidate-1ef31893/Inspect-C-Progress.ps1'
$tokens=$null;$errors=$null
$ast=[Management.Automation.Language.Parser]::ParseFile($inspection,[ref]$tokens,[ref]$errors)
if($errors.Count){throw 'Original public reader parse failed'}
foreach($name in @('Assert-NoRedirect','Shape','PublicFacts','Read-PublicArtifact')){
    $fn=@($ast.FindAll({param($n) $n -is [Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -ceq $name},$false))
    if($fn.Count -ne 1){throw 'Original public reader function missing'}
    . ([scriptblock]::Create($fn[0].Extent.Text))
}
$helper=Join-Path $repo 'scripts/windows/NovonetExistingProcess.ps1'
$text=(Get-Content -LiteralPath $helper -Raw).Replace("`r`n","`n")
if([Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes($text))).ToLowerInvariant() -cne '659d2cdecf3e5ca0e58472a25c9f85b4f85a05095fca7b0cc0fc4277077f7b66'){throw 'Original process helper differs'}
. $helper
$out=[IO.Path]::GetFullPath($EvidenceRoot)
if(Test-Path -LiteralPath $out){throw 'Fresh evidence directory required'}
$work='C:\ProgramData\NOVOVM\selfrun\91129655606029\C\novonet-observation-1ef31893-20261110'
$exe='C:\ProgramData\NOVOVM\releases\novonet-observation-1ef31893-windows-x86_64\bin\novovm-node.exe'
Assert-NoRedirect $work
$report=[ordered]@{host_id='C';started_utc=[DateTime]::UtcNow.ToString('o');scope='read-only original 1ef lifecycle; no updater invoked';errors=@()}
try{
    $task=Get-ScheduledTask -TaskName 'NOVOVM-C-1ef31893-Existing-20261110'
    $info=$task|Get-ScheduledTaskInfo
    $sid=if($task.Principal.UserId.StartsWith('S-1-')){$task.Principal.UserId}else{([Security.Principal.NTAccount]::new($task.Principal.UserId)).Translate([Security.Principal.SecurityIdentifier]).Value}
    $report.task=[ordered]@{name=$task.TaskName;state=$task.State.ToString();sid=$sid;last_task_result=[uint32]$info.LastTaskResult;last_task_result_hex=('{0:X8}' -f [uint32]$info.LastTaskResult);last_run_utc=$info.LastRunTime.ToUniversalTime().ToString('o');restart_count=[int]$task.Settings.RestartCount;trigger_count=@($task.Triggers|Where-Object {$null -ne $_}).Count;execution_time_limit=$task.Settings.ExecutionTimeLimit}
}catch{$report.errors+=@{stage='task';type=$_.Exception.GetType().Name}}
try{
    $execution=Get-NovonetNodeExecutionState
    $report.active_nodes=@($execution.active|ForEach-Object {[ordered]@{pid=$_.ProcessId;path=$_.ExecutablePath;created_utc=$_.CreationDate.ToUniversalTime().ToString('o');sid=(Invoke-CimMethod $_ -MethodName GetOwnerSid).Sid}})
    $report.exited_nodes=@($execution.exited)
}catch{$report.errors+=@{stage='kernel_process';type=$_.Exception.GetType().Name}}
try{$report.listeners=@(Get-NetUDPEndpoint|Where-Object LocalPort -In @(41052,42052)|Select-Object LocalAddress,LocalPort,OwningProcess)}catch{$report.errors+=@{stage='listeners';type=$_.Exception.GetType().Name}}
$report.artifacts=@()
foreach($name in @('owner.json','progress.json','exit.json','node.stdout')){
    try{$report.artifacts+=Read-PublicArtifact (Join-Path $work "logs\$name")}catch{$report.errors+=@{stage=$name;type=$_.Exception.GetType().Name}}
}
try{
    $stderr=Join-Path $work 'logs\node.stderr'
    if(Test-Path -LiteralPath $stderr){
        Assert-NoRedirect $stderr
        $item=Get-Item -LiteralPath $stderr
        $report.stderr=[ordered]@{path=$stderr;bytes=$item.Length;last_write_utc=$item.LastWriteTimeUtc.ToString('o');content_exported=$false}
        if($item.Length -le 4MB){
            $raw=Get-Content -LiteralPath $stderr -Raw
            $report.stderr.sha256=(Get-FileHash -LiteralPath $stderr).Hash
            $report.stderr.known_markers=@(@('native send operation deadline','original tick stopped','panicked at','Access is denied','LOCK','Permission denied')|Where-Object {$null -ne $raw -and $raw.Contains($_)})
        }
    }else{$report.stderr=@{path=$stderr;status='missing'}}
}catch{$report.errors+=@{stage='stderr';type=$_.Exception.GetType().Name}}
$since=[DateTime]::Now.AddDays(-2)
if($null -ne $report.task){$since=([DateTime]$report.task.last_run_utc).ToLocalTime().AddMinutes(-1);if($since -lt [DateTime]::Now.AddDays(-2)){$since=[DateTime]::Now.AddDays(-2)}}
$eventErrors=@()
$events=@(Get-WinEvent -FilterHashtable @{LogName='Application';Id=1000;StartTime=$since} -MaxEvents 100 -ErrorAction SilentlyContinue -ErrorVariable eventErrors)
$report.event_query_errors=@($eventErrors|ForEach-Object {$_.FullyQualifiedErrorId})
$report.crash_events=@($events|ForEach-Object {
    $event=$_;$xml=[xml]$event.ToXml();$fields=@{}
    foreach($field in $xml.Event.EventData.Data){$fields[$field.GetAttribute('Name')]=[string]$field.InnerText}
    if($fields.AppPath -ceq $exe){[ordered]@{record_id=$event.RecordId;utc=$event.TimeCreated.ToUniversalTime().ToString('o');application=$fields.AppName;application_path=$fields.AppPath;exception_code=$fields.ExceptionCode;process_id_raw=$fields.ProcessId;process_created_raw=$fields.ProcessCreationTime;module=$fields.ModuleName;module_path=$fields.ModulePath;report_id=$fields.IntegratorReportId}}
})
$report.node_started=$false;$report.database_opened=$false;$report.keys_read=$false;$report.completed_utc=[DateTime]::UtcNow.ToString('o')
New-Item -ItemType Directory -Path $out|Out-Null
Assert-NoRedirect $out
$report|ConvertTo-Json -Depth 30|Out-File (Join-Path $out 'C-old-owner-public.json') -Encoding utf8 -NoClobber
Write-Output "READ_ONLY_COMPLETE: $out\C-old-owner-public.json; not recovery authorization or successful update"
