#Requires -Version 7.0
# D-scoped common95 existing cutover. Existing native owner, original keys/database; no initializer.
param([switch]$Owner, [switch]$VerifyOnly)
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$PSNativeCommandUseErrorActionPreference=$false
$taskRoot='D:\WEB3_AI\SUPERVM\target\d-common95-cutover-20261117'
$state='C:\ProgramData\NOVOVM\selfrun\91129655606029\D'
$work=Join-Path $state 'novovm-common-95ec9c2e-20261117'
$release='C:\ProgramData\NOVOVM\releases\novovm-common-95ec9c2e-windows-x86_64'
$exe=Join-Path $release 'bin\novovm-node.exe'
$nodePin='754eb3f50ab6111ec494d30b00af52e8dbee43038f54e0a1c313e8579bfc6bf7'
$corePin='f9e8ded21753dfd32573de7bf66da8a785120444e810ba9fa865bfa4d639fcaa'
$manifestPin='82e5229ed041af942e0333597ce1fbfcca6f9f66f69dd4bb598d69b5c0e66ea8'
$newTask='NOVOVM-D-Common95-Existing-20261117'
function Get-VerifiedDeploymentHelperText {
    param([string]$Path, [string]$ExpectedSha256)
    $bytes=[IO.File]::ReadAllBytes($Path)
    $normalized=[IO.MemoryStream]::new()
    try {
        # Git may check out CRLF; only CR immediately before LF is normalized.
        for($i=0;$i -lt $bytes.Length;$i++) {
            if($bytes[$i] -eq 13 -and $i+1 -lt $bytes.Length -and $bytes[$i+1] -eq 10){continue}
            $normalized.WriteByte($bytes[$i])
        }
        $verifiedBytes=$normalized.ToArray()
        $actual=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($verifiedBytes)).ToLowerInvariant()
        if($actual -cne $ExpectedSha256){throw 'Original deployment helpers changed'}
        return [Text.UTF8Encoding]::new($false,$true).GetString($verifiedBytes)
    } finally {$normalized.Dispose()}
}
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
$receipt='D:\WEB3_AI\SUPERVM\artifacts\audit\aoem-latest-57090b6b-20261117\candidate-common-95ec9c2e'
$q=Get-Content "$receipt/final-windows-qualification.json" -Raw|ConvertFrom-Json
if($q.native_recovery_exit -ne 0 -or !$q.all_members_verified -or $q.node_sha256 -cne $nodePin -or $q.library_sha256 -cne $corePin -or $q.manifest_sha256 -cne $manifestPin){throw 'Current package qualification differs'}
$original='D:\WEB3_AI\SUPERVM\scripts\windows\Update-NovonetExisting.ps1'
$helperText=Get-VerifiedDeploymentHelperText $original '7b50ae0c70ba039dce93ae44b8d18b541e77d3c4ff45e95eedf60c3a0d29d5cd'
$tokens=$null;$errors=$null
$ast=[Management.Automation.Language.Parser]::ParseInput($helperText,[ref]$tokens,[ref]$errors)
if($errors.Count){throw 'Original helper parse failed'}
foreach($name in @('Write-NewJson','Assert-NoRedirect','Set-OwnerAcl','Get-CustodyHashes','Assert-FirewallFilterScope')){
    $fn=@($ast.FindAll({param($n) $n -is [Management.Automation.Language.FunctionDefinitionAst]},$false)|Where-Object Name -CEQ $name)
    if($fn.Count -ne 1){throw 'Original helper missing'}
    . ([scriptblock]::Create($fn[0].Extent.Text))
}
. 'D:\WEB3_AI\SUPERVM\scripts\windows\NovonetExistingProcess.ps1'
Wait-NovonetNodeExit
if(@(Get-NetUDPEndpoint|Where-Object LocalPort -in @(41053,42053)).Count){throw 'Original D ports still owned'}
$oldWork=Join-Path $state 'novonet-observation-a25452f4-20261112'
$oldTask=Get-ScheduledTask -TaskName 'NOVOVM-D-a25452f4-Existing-20261112'
$oldInfo=Get-ScheduledTaskInfo -TaskName $oldTask.TaskName
$oldExit=Get-Content "$oldWork/logs/exit.json" -Raw|ConvertFrom-Json
$oldSid=if($oldTask.Principal.UserId.StartsWith('S-1-')){$oldTask.Principal.UserId}else{([Security.Principal.NTAccount]::new($oldTask.Principal.UserId)).Translate([Security.Principal.SecurityIdentifier]).Value}
if($oldTask.State.ToString() -cne 'Ready' -or $oldSid -cne 'S-1-5-19' -or $oldInfo.LastTaskResult -ne 0 -or $oldExit.exit_code -ne 0 -or $oldExit.start -cne 'existing'){throw 'D original clean existing exit not proven'}
if(Test-Path $work){throw 'Candidate work already exists; do not repeat cutover'}
if(Test-Path $release){throw 'Candidate release already exists; inspect first'}
if(Get-ScheduledTask -TaskName $newTask -ErrorAction SilentlyContinue){throw 'Candidate task already exists'}
$config=Get-Content "$oldWork/run.json" -Raw|ConvertFrom-Json
$network=Get-Content $config.novonet_config_file -Raw|ConvertFrom-Json
foreach($path in @($state,$config.database,$config.novonet_config_file,$oldWork)){Assert-NoRedirect $path}
if($config.database -cne "$state\chain-state" -or $config.profile -cne 'native_execution_tick' -or $config.udp_addr -cne '192.168.71.59:41053'){throw 'Original D config differs'}
if($network.probe_control_file -and (Test-Path $network.probe_control_file)){throw 'Unexpected pending probe; no activation'}
$prior=Get-Content "$oldWork/logs/progress.json" -Raw|ConvertFrom-Json
if($prior.chain_id -ne 91129655606029 -or $prior.execution_failures -ne 0){throw 'Prior original state projection invalid'}
$keys=Get-CustodyHashes $config $network
$package='D:\WEB3_AI\SUPERVM\target\d-common95-package-20261117'
if((Get-FileHash "$package/release-manifest.json").Hash.ToLowerInvariant() -cne $manifestPin){throw 'Candidate source manifest changed'}
$manifest=Get-Content "$package/release-manifest.json" -Raw|ConvertFrom-Json
if($manifest.git_commit -cne '95ec9c2eced988c5798d549971354ef8722367b7' -or $manifest.artifacts.Count -ne 91){throw 'Fixed package source/count differs'}
foreach($entry in $manifest.artifacts){
    $path=[IO.Path]::GetFullPath((Join-Path $package $entry.path))
    if(!$path.StartsWith($package+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'Candidate member escaped package'}
    Assert-NoRedirect $path
    if((Get-Item -LiteralPath $path).Length -ne $entry.bytes -or (Get-FileHash -LiteralPath $path).Hash.ToLowerInvariant() -cne $entry.sha256){throw 'Candidate member changed'}
}
$rule=Get-NetFirewallRule -Name 'NOVOVM-a25452f4-D-20261112'
$newRuleName='NOVOVM-common95-D-20261117'
if(Get-NetFirewallRule -Name $newRuleName -ErrorAction SilentlyContinue){throw 'New program rule already exists'}
if($VerifyOnly){Write-Output 'D_COMMON95_PREFLIGHT_PASS_NO_MUTATION';return}
if(Test-Path $taskRoot){throw 'Evidence already exists; preserve and stop'}
New-Item -ItemType Directory -Path $taskRoot|Out-Null
Copy-Item -LiteralPath $package -Destination $release -Recurse
foreach($item in @((Get-Item $release))+@(Get-ChildItem $release -Recurse -Force)){Set-OwnerAcl $item.FullName}
New-Item -ItemType Directory -Path "$work/logs"|Out-Null
Set-OwnerAcl $work
Set-OwnerAcl "$work/logs" -Writable
Copy-Item -LiteralPath $config.database -Destination "$work/database-cold-before" -Recurse
foreach($item in Get-ChildItem -LiteralPath $config.database -File -Recurse -Force){
    Assert-NoRedirect $item.FullName
    $relative=[IO.Path]::GetRelativePath($config.database,$item.FullName)
    $copy=Join-Path "$work/database-cold-before" $relative
    if((Get-FileHash -LiteralPath $item.FullName).Hash -cne (Get-FileHash -LiteralPath $copy).Hash){throw 'Cold database copy differs; no startup'}
}
Write-NewJson "$work/before-private.json" @{prior=$prior;key_hashes=$keys;prior_clean_exit=$oldExit}
Set-OwnerAcl "$work/before-private.json"
$config.library="$release\aoem\windows\core\bin\aoem_ffi.dll"
Write-NewJson "$work/run.json" $config
Set-OwnerAcl "$work/run.json"
Copy-Item -LiteralPath $PSCommandPath -Destination "$work/Start-Owner.ps1"
Set-OwnerAcl "$work/Start-Owner.ps1"
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
if(41053 -notin $ports.LocalPort -or 42053 -notin $ports.LocalPort){throw 'Original listeners not ready'}
Write-NewJson "$taskRoot/D-common95-public.json" @{status='D_COMMON95_EXISTING_READY_FOR_A_QUERY';utc=[datetime]::UtcNow.ToString('o');source='95ec9c2eced988c5798d549971354ef8722367b7';program_source='1eb2f202d821fbb2e7cccee4e8bbfb3cd789c864';pid=$nodes[0].ProcessId;node_sha256=$nodePin;aoem_sha256=$corePin;actual_aoem=$module[0].FileName;config_path="$work/run.json";progress_path="$work/logs/progress.json";task=$newTask;release_manifest_sha256=$manifestPin;start='existing';database=$config.database;sid='S-1-5-19';height=$current.current_height;head=$current.head;state_projection_unchanged=$true;custody_unchanged=$true;execution_failures=0;listeners=$ports;prior_clean_exit=$oldExit;initialize=$false;keys_generated=$false;double_relay_done=$false}
Write-Output 'D_COMMON95_EXISTING_RUNNING; authenticated query and actual relay delivery remain separate'
