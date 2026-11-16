#Requires -Version 7.0
# Current A cutover only. Existing native owner, original keys/database; no initializer.
param([switch]$Owner)
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$PSNativeCommandUseErrorActionPreference=$false
$taskRoot='D:\WorksArea\SUPERVM\target\aoem-latest-57090b6b-20261117'
$state='C:\ProgramData\NOVOVM\selfrun\91129655606029\A'
$work=Join-Path $state 'novovm-common-95ec9c2e-20261117'
$release='C:\ProgramData\NOVOVM\releases\novovm-common-95ec9c2e-windows-x86_64'
$exe=Join-Path $release 'bin\novovm-node.exe'
$nodePin='754eb3f50ab6111ec494d30b00af52e8dbee43038f54e0a1c313e8579bfc6bf7'
$corePin='f9e8ded21753dfd32573de7bf66da8a785120444e810ba9fa865bfa4d639fcaa'
$manifestPin='82e5229ed041af942e0333597ce1fbfcca6f9f66f69dd4bb598d69b5c0e66ea8'
$newTask='NOVOVM-A-Common95-Existing-20261117'
if($Owner){
    try{
        if([Security.Principal.WindowsIdentity]::GetCurrent().User.Value -cne 'S-1-5-19'){throw 'Original LocalService account required'}
        if((Get-FileHash $exe).Hash.ToLowerInvariant() -cne $nodePin){throw 'Fixed node changed'}
        if((Get-FileHash "$release/aoem/windows/core/bin/aoem_ffi.dll").Hash.ToLowerInvariant() -cne $corePin){throw 'Fixed AOEM changed'}
        if((Get-FileHash "$release/release-manifest.json").Hash.ToLowerInvariant() -cne $manifestPin){throw 'Fixed manifest changed'}
        $env:AOEM_DLL_MANIFEST="$release/aoem/manifest/aoem-manifest.json"
        $env:NOVOVM_AOEM_MANIFEST=$env:AOEM_DLL_MANIFEST
        $env:AOEM_DLL_MANIFEST_REQUIRED='1'
        $env:NOVOVM_AOEM_PLUGIN_DIR="$release/aoem/windows/core/plugins"
        $env:AOEM_FFI_PLUGIN_DIR=$env:NOVOVM_AOEM_PLUGIN_DIR
        $env:AOEM_SEMANTIC_GRAPH_V2_WORKERS='1'
        $env:NOVOVM_NODE_MODE='native_execution_tick'
        $env:NOVOVM_NATIVE_EXECUTION_CONFIG="$work/run.json"
        $env:NOVOVM_NATIVE_EXECUTION_START='existing'
        Remove-Item Env:NOVOVM_NATIVE_EXECUTION_RUN_MS -ErrorAction SilentlyContinue
        $env:NOVOVM_PROFILE_NATIVE_DELIVERY='1'
        $env:NOVOVM_NATIVE_EXECUTION_PIPELINE_PROGRESS_REPORT_PATH="$work/logs/progress.json"
        @{utc=[datetime]::UtcNow.ToString('o');source='95ec9c2eced988c5798d549971354ef8722367b7';start='existing';sid='S-1-5-19';initialize=$false}|ConvertTo-Json|Set-Content "$work/logs/owner.json"
        & $exe 1> "$work/logs/node.stdout" 2> "$work/logs/node.stderr"
        $code=$LASTEXITCODE
        @{utc=[datetime]::UtcNow.ToString('o');exit_code=$code;start='existing'}|ConvertTo-Json|Set-Content "$work/logs/exit.json"
        exit $code
    }catch{@{error=$_.Exception.Message;utc=[datetime]::UtcNow.ToString('o')}|ConvertTo-Json|Set-Content "$work/logs/error.json";exit 1}
}
$principal=[Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())
if(!$principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)){throw 'Administrator required'}
$full=gh run view 43952963989 --json headSha,status,conclusion | ConvertFrom-Json
if($LASTEXITCODE -ne 0 -or $full.headSha -cne '1eb2f202d821fbb2e7cccee4e8bbfb3cd789c864' -or $full.status -cne 'completed' -or $full.conclusion -cne 'success'){throw 'Exact Full not passed'}
$q=Get-Content "$taskRoot/final-windows-qualification.json" -Raw|ConvertFrom-Json
if($q.native_recovery_exit -ne 0 -or !$q.all_members_verified -or $q.node_sha256 -cne $nodePin -or $q.library_sha256 -cne $corePin -or $q.manifest_sha256 -cne $manifestPin){throw 'Current package qualification differs'}
$original='D:\WorksArea\SUPERVM\scripts\windows\Update-NovonetExisting.ps1'
if((Get-FileHash $original).Hash.ToLowerInvariant() -cne '7b50ae0c70ba039dce93ae44b8d18b541e77d3c4ff45e95eedf60c3a0d29d5cd'){throw 'Original deployment helpers changed'}
$tokens=$null;$errors=$null
$ast=[Management.Automation.Language.Parser]::ParseFile($original,[ref]$tokens,[ref]$errors)
if($errors.Count){throw 'Original helper parse failed'}
foreach($name in @('Write-NewJson','Assert-NoRedirect','Set-OwnerAcl','Get-CustodyHashes','Assert-FirewallFilterScope')){
    $fn=@($ast.FindAll({param($n) $n -is [Management.Automation.Language.FunctionDefinitionAst]},$false)|Where-Object Name -CEQ $name)
    if($fn.Count -ne 1){throw 'Original helper missing'}
    . ([scriptblock]::Create($fn[0].Extent.Text))
}
. 'D:\WorksArea\SUPERVM\scripts\windows\NovonetExistingProcess.ps1'
Wait-NovonetNodeExit
if(@(Get-NetUDPEndpoint|Where-Object LocalPort -in @(41051,42051)).Count){throw 'Original A ports still owned'}
$oldTask=Get-ScheduledTask -TaskName 'NOVOVM-A-AOEM57090b6b-Existing-20261117'
$oldInfo=Get-ScheduledTaskInfo -TaskName $oldTask.TaskName
$oldExit=Get-Content "$taskRoot/A-logs/exit.json" -Raw|ConvertFrom-Json
$oldSid=if($oldTask.Principal.UserId.StartsWith('S-1-')){$oldTask.Principal.UserId}else{([Security.Principal.NTAccount]::new($oldTask.Principal.UserId)).Translate([Security.Principal.SecurityIdentifier]).Value}
if($oldTask.State.ToString() -cne 'Ready' -or $oldSid -cne 'S-1-5-19' -or $oldInfo.LastTaskResult -ne 1 -or $oldExit.exit_code -ne 1 -or ([datetime]$oldExit.utc).ToUniversalTime() -ne ([datetime]'2026-11-16T20:45:07.913344Z').ToUniversalTime()){throw 'Current disk-full exit evidence differs'}
if((Get-FileHash "$taskRoot/A-logs/node.stderr").Hash.ToLowerInvariant() -cne '185cc7afe22e0f7139fa4b4411ce4e3f6723351905b7cc94d103097d3a0e184e'){throw 'Original disk-full stderr changed'}
if(Test-Path $work){throw 'Candidate work already exists; do not repeat cutover'}
if(Test-Path $release){throw 'Candidate release already exists; inspect first'}
if(Get-ScheduledTask -TaskName $newTask -ErrorAction SilentlyContinue){throw 'Candidate task already exists'}
$config=Get-Content "$taskRoot/A-run.json" -Raw|ConvertFrom-Json
$network=Get-Content $config.novonet_config_file -Raw|ConvertFrom-Json
if($config.database -cne "$state\chain-state" -or $config.profile -cne 'native_execution_tick' -or $config.udp_addr -cne '192.168.71.74:41051'){throw 'Original A config differs'}
if(Test-Path $network.probe_control_file){throw 'Unexpected pending probe; no activation'}
$prior=Get-Content "$taskRoot/A-logs/progress.json" -Raw|ConvertFrom-Json
if($prior.chain_id -ne 91129655606029 -or $prior.execution_failures -ne 0){throw 'Prior original state projection invalid'}
$keys=Get-CustodyHashes $config $network
$package='D:\WorksArea\SUPERVM\artifacts\product-overlay\novovm-common-95ec9c2e-aoem570-windows-x86_64'
if((Get-FileHash "$package/release-manifest.json").Hash.ToLowerInvariant() -cne $manifestPin){throw 'Candidate source manifest changed'}
$manifest=Get-Content "$package/release-manifest.json" -Raw|ConvertFrom-Json
foreach($entry in $manifest.artifacts){if((Get-FileHash (Join-Path $package $entry.path)).Hash.ToLowerInvariant() -cne $entry.sha256){throw 'Candidate member changed'}}
Copy-Item -LiteralPath $package -Destination $release -Recurse
foreach($item in @((Get-Item $release))+@(Get-ChildItem $release -Recurse -Force)){Set-OwnerAcl $item.FullName}
New-Item -ItemType Directory -Path "$work/logs"|Out-Null
Set-OwnerAcl $work
Set-OwnerAcl "$work/logs" -Writable
Copy-Item -LiteralPath $config.database -Destination "$work/database-cold-before" -Recurse
Write-NewJson "$work/before-private.json" @{prior=$prior;key_hashes=$keys;disk_full_exit=$oldExit}
Set-OwnerAcl "$work/before-private.json"
$config.library="$release\aoem\windows\core\bin\aoem_ffi.dll"
Write-NewJson "$work/run.json" $config
Set-OwnerAcl "$work/run.json"
Copy-Item -LiteralPath $PSCommandPath -Destination "$work/Start-Owner.ps1"
Set-OwnerAcl "$work/Start-Owner.ps1"
$rule=Get-NetFirewallRule -Name 'NOVOVM-a25452f4-A-20261112'
$newRuleName='NOVOVM-common95-A-20261117'
if(Get-NetFirewallRule -Name $newRuleName -ErrorAction SilentlyContinue){throw 'New program rule already exists'}
$profiles=@(Get-NetFirewallProfile|Select-Object Name,Enabled)
Copy-NetFirewallRule -Name $rule.Name -NewName $newRuleName|Out-Null
$fresh=Get-NetFirewallRule -Name $newRuleName
$fresh|Get-NetFirewallApplicationFilter|Set-NetFirewallApplicationFilter -Program $exe
foreach($command in @('Get-NetFirewallPortFilter','Get-NetFirewallAddressFilter','Get-NetFirewallInterfaceFilter','Get-NetFirewallInterfaceTypeFilter','Get-NetFirewallSecurityFilter','Get-NetFirewallServiceFilter')){
    Assert-FirewallFilterScope (& $command -AssociatedNetFirewallRule $rule) (& $command -AssociatedNetFirewallRule $fresh) $rule.Name $fresh.Name
}
if(($profiles|ConvertTo-Json -Compress) -cne (@(Get-NetFirewallProfile|Select-Object Name,Enabled)|ConvertTo-Json -Compress)){throw 'Global firewall profiles changed'}
Write-NewJson "$work/firewall-public.json" @{old_rule=$rule.Name;new_rule=$newRuleName;same_scope=$true;program=$exe;profiles_unchanged=$true}
$action=New-ScheduledTaskAction -Execute 'C:\Program Files\PowerShell\7\pwsh.exe' -Argument "-NoProfile -ExecutionPolicy RemoteSigned -File $work\Start-Owner.ps1 -Owner" -WorkingDirectory $work
$account=New-ScheduledTaskPrincipal -UserId 'NT AUTHORITY\LOCAL SERVICE' -LogonType ServiceAccount
$settings=New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([timespan]::Zero) -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
Register-ScheduledTask -TaskName $newTask -Action $action -Principal $account -Settings $settings|Out-Null
Start-ScheduledTask -TaskName $newTask
$clock=[Diagnostics.Stopwatch]::StartNew()
do{
    if(Test-Path "$work/logs/error.json"){throw 'New owner preparation failed; preserve logs'}
    if(Test-Path "$work/logs/exit.json"){throw 'New node exited; preserve state, no old-version retry'}
    try{$current=Get-Content "$work/logs/progress.json" -Raw -ErrorAction Stop|ConvertFrom-Json -ErrorAction Stop}catch{$current=$null}
    if($null -ne $current -and $current.entry -ceq 'original_native_execution_tick' -and $current.recovery_in_progress -eq $false){break}
    if($clock.Elapsed.TotalSeconds -gt 120){throw 'No current existing-start readiness'}
    Start-Sleep -Milliseconds 500
}while($true)
if($current.current_height -ne $prior.current_height -or ($current.head|ConvertTo-Json -Compress) -cne ($prior.head|ConvertTo-Json -Compress) -or $current.execution_failures -ne 0){throw 'Original state projection differs'}
if(($keys|ConvertTo-Json -Compress) -cne ((Get-CustodyHashes $config $network)|ConvertTo-Json -Compress)){throw 'Original custody changed'}
$nodes=@((Get-NovonetNodeExecutionState).active)
if($nodes.Count -ne 1 -or $nodes[0].ExecutablePath -cne $exe -or (Invoke-CimMethod $nodes[0] -MethodName GetOwnerSid).Sid -cne 'S-1-5-19'){throw 'New actual owner differs'}
$module=@((Get-Process -Id $nodes[0].ProcessId).Modules|Where-Object ModuleName -eq 'aoem_ffi.dll')
if($module.Count -ne 1 -or (Get-FileHash $module[0].FileName).Hash.ToLowerInvariant() -cne $corePin){throw 'Loaded AOEM differs'}
$ports=@(Get-NetUDPEndpoint|Where-Object OwningProcess -eq $nodes[0].ProcessId|Select-Object LocalAddress,LocalPort,OwningProcess)
if(41051 -notin $ports.LocalPort -or 42051 -notin $ports.LocalPort){throw 'Original listeners not ready'}
Write-NewJson "$taskRoot/A-common95-public.json" @{status='EXISTING_RUNNING_HEALTHY_NOT_DELIVERY';utc=[datetime]::UtcNow.ToString('o');source='95ec9c2eced988c5798d549971354ef8722367b7';program_source='1eb2f202d821fbb2e7cccee4e8bbfb3cd789c864';pid=$nodes[0].ProcessId;node_sha256=$nodePin;aoem_sha256=$corePin;actual_aoem=$module[0].FileName;start='existing';database=$config.database;sid='S-1-5-19';height=$current.current_height;head=$current.head;state_projection_unchanged=$true;custody_unchanged=$true;execution_failures=0;listeners=$ports;old_disk_full_exit=$oldExit;initialize=$false;keys_generated=$false;double_relay_done=$false}
Write-Output 'A_COMMON95_EXISTING_RUNNING; authenticated query and actual relay delivery remain separate'
