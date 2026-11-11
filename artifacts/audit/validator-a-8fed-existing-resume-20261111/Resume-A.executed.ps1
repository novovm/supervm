#Requires -Version 7.0
#Requires -RunAsAdministrator
# Resume the installed CURRENT owner; no updater, initialization or old binary.
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$out='D:\WorksArea\SUPERVM\target\native-time-upgrade-20261110\A-resume-20261111'
if(Test-Path -LiteralPath $out){throw 'Preserve prior recovery result; do not repeat'}
New-Item -ItemType Directory -Path $out|Out-Null
$work='C:\ProgramData\NOVOVM\selfrun\91129655606029\A\novonet-observation-8fed4b76-20261110'
$release='C:\ProgramData\NOVOVM\releases\novonet-observation-8fed4b76-windows-x86_64'
$owner=Join-Path $work 'ExistingOwner.ps1'
$exe=Join-Path $release 'bin\novovm-node.exe'
$logs=Join-Path $work 'logs'
$archive=Join-Path $work 'logs-interrupted-20261111'
$taskName='NOVOVM-A-8fed4b76-Existing-20261110'
$source='8fed4b76f518935b30fcfb13930ccd3d7aae7381'
$manifestPin='5ded6fb8bad59e5f2a2eb0d568e982dbf4f62dce595bc6a29a7dd0faa007201c'
$nodePin='021e7f8b0b41d8b72577d7394cd9c1fb9edb8388c4ae90903dca6f86a53a8a32'
$corePin='1b764a36c1f782425237d40446d4ea827f56a66fb341b870855d84c15acdce50'
$r=[ordered]@{utc=[DateTime]::UtcNow.ToString('o');source=$source;start='existing';initialize_executed=$false;keys_changed=$false;started=$false;healthy=$false;prior_exit_cause='unknown';logs_archived=$false}
try {
    if((Get-FileHash -LiteralPath $owner).Hash.ToLowerInvariant() -cne '1abe055135ac1469bac56b91d813844befc14173e8461edbf61e003e2f641a98'){throw 'Installed owner script differs'}
    # Invoke only the original pinned owner's validation helpers, never its main.
    $tokens=$null;$errors=$null
    $ast=[Management.Automation.Language.Parser]::ParseFile($owner,[ref]$tokens,[ref]$errors)
    if($errors.Count){throw 'Owner parse error'}
    foreach($name in @('Assert-NoRedirect','Assert-Package','Set-OwnerAcl','Get-CustodyHashes','Assert-RecoveredProgress')) {
        $f=@($ast.FindAll({param($n) $n -is [Management.Automation.Language.FunctionDefinitionAst]},$false)|Where-Object Name -CEQ $name)
        if($f.Count -ne 1){throw 'Original helper missing'}
        . ([scriptblock]::Create($f[0].Extent.Text))
    }
    $helper='D:\WorksArea\SUPERVM\scripts\windows\NovonetExistingProcess.ps1'
    $helperText=(Get-Content -LiteralPath $helper -Raw).Replace("`r`n","`n")
    if([Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes($helperText))).ToLowerInvariant() -cne '659d2cdecf3e5ca0e58472a25c9f85b4f85a05095fca7b0cc0fc4277077f7b66'){throw 'Process barrier helper differs'}
    . $helper
    Assert-Package $release
    Assert-NoRedirect $work
    Assert-NoRedirect $logs
    $task=Get-ScheduledTask -TaskName $taskName
    $info=$task|Get-ScheduledTaskInfo
    $sid=if($task.Principal.UserId.StartsWith('S-1-')){$task.Principal.UserId}else{([Security.Principal.NTAccount]::new($task.Principal.UserId)).Translate([Security.Principal.SecurityIdentifier]).Value}
    $argsExpected='-NoProfile -NonInteractive -ExecutionPolicy RemoteSigned -File "'+$owner+'" -HostId A -Owner'
    if($task.State -ne 'Ready' -or $info.LastTaskResult -ne 267014 -or $sid -cne 'S-1-5-19' -or $task.Settings.RestartCount -ne 0 -or @($task.Triggers|Where-Object {$null -ne $_}).Count -ne 0){throw 'Original interrupted task changed'}
    if(@($task.Actions).Count -ne 1 -or $task.Actions[0].Execute -ine 'C:\Program Files\PowerShell\7\pwsh.exe' -or $task.Actions[0].Arguments -cne $argsExpected){throw 'Task is not the pinned existing owner'}
    Wait-NovonetNodeExit
    if(@(Get-NetUDPEndpoint|Where-Object LocalPort -in 41051,42051).Count){throw 'A ports still occupied'}
    $config=Get-Content "$work\run.json" -Raw|ConvertFrom-Json
    $network=Get-Content -LiteralPath $config.novonet_config_file -Raw|ConvertFrom-Json
    if($config.database -cne 'C:\ProgramData\NOVOVM\selfrun\91129655606029\A\chain-state' -or $config.library -ine "$release\aoem\windows\core\bin\aoem_ffi.dll"){throw 'Original paths differ'}
    Assert-NoRedirect $config.database
    $keys=Get-CustodyHashes $config $network
    $previous=Get-Content "$logs\progress.json" -Raw|ConvertFrom-Json
    Assert-RecoveredProgress $previous $previous $false
    $r.prior_task_result=$info.LastTaskResult
    $r.prior_progress_sha256=(Get-FileHash "$logs\progress.json").Hash.ToLowerInvariant()
    if(Test-Path -LiteralPath $archive){throw 'Log archive already exists'}
    # Both resolved paths are immediate children of this exact A work directory.
    foreach($p in @($logs,$archive)){if([IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($p)) -cne $work){throw 'Archive path escaped A owner'}}
    Move-Item -LiteralPath $logs -Destination $archive
    $r.logs_archived=$true;$r.archive_path=$archive
    New-Item -ItemType Directory -Path $logs|Out-Null
    Set-OwnerAcl $logs -Writable
    Wait-NovonetNodeExit
    if((Get-ScheduledTask -TaskName $taskName).State -ne 'Ready' -or @(Get-NetUDPEndpoint|Where-Object LocalPort -in 41051,42051).Count){throw 'A lifecycle changed before start'}
    Start-ScheduledTask -TaskName $taskName
    $r.started=$true
    $timer=[Diagnostics.Stopwatch]::StartNew();$current=$null
    while($timer.Elapsed.TotalSeconds -lt 60) {
        if(Test-Path "$logs\progress.json") {try{$current=Get-Content "$logs\progress.json" -Raw|ConvertFrom-Json}catch{$current=$null}}
        if($null -ne $current){break}
        Start-Sleep -Milliseconds 500
    }
    Assert-RecoveredProgress $current $previous $false
    if(((Get-Date)-(Get-Item "$logs\progress.json").LastWriteTime).TotalSeconds -gt 30){throw 'Recovery progress stale'}
    $nodes=@((Get-NovonetNodeExecutionState).active)
    if($nodes.Count -ne 1 -or $nodes[0].ExecutablePath -cne $exe -or (Invoke-CimMethod $nodes[0] -MethodName GetOwnerSid).Sid -cne 'S-1-5-19'){throw 'New A identity differs'}
    $loaded=@((Get-Process -Id $nodes[0].ProcessId).Modules|Where-Object ModuleName -EQ 'aoem_ffi.dll')
    if($loaded.Count -ne 1 -or $loaded[0].FileName -ine $config.library -or (Get-FileHash $loaded[0].FileName).Hash.ToLowerInvariant() -cne $corePin){throw 'Actual AOEM differs'}
    $afterKeys=Get-CustodyHashes $config $network
    foreach($path in $keys.Keys){if($keys[$path] -cne $afterKeys[$path]){throw 'Custody changed'}}
    $r.pid=$nodes[0].ProcessId;$r.sid=$sid;$r.database=$config.database;$r.custody_unchanged=$true
    $r.node_sha256=$nodePin;$r.aoem_sha256=$corePin
    $r.progress=$current|Select-Object chain_id,current_height,head,execution_failures,recovery_in_progress
    $r.listeners=@(Get-NetUDPEndpoint|Where-Object OwningProcess -EQ $r.pid|Select-Object LocalAddress,LocalPort,OwningProcess)
    if(@($r.listeners|Where-Object LocalPort -in 41051,42051).Count -ne 2){throw 'A listeners not ready'}
    $r.healthy=$true;$r.completed_utc=[DateTime]::UtcNow.ToString('o')
}catch {
    $r.error=$_.Exception.Message
    if($r.started){
        try {
            Stop-ScheduledTask -TaskName $taskName
            $remaining=@((Get-NovonetNodeExecutionState).active)
            if($remaining.Count -eq 1){Stop-NovonetExactProcess -ProcessId $remaining[0].ProcessId -ExpectedPath $exe -ExpectedSha256 $nodePin}
            Wait-NovonetNodeExit
            $r.failed_candidate_stopped=$true
        }catch{$r.stop_error=$_.Exception.Message}
    }
}finally{$r|ConvertTo-Json -Depth 12|Set-Content "$out/result.json" -Encoding utf8}
if(!$r.healthy){throw 'A current-owner recovery failed; inspect result, no automatic retry'}
