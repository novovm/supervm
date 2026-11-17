#Requires -Version 7.0
# Fixed candidate adapter on the original LocalService existing-start owner.
param([switch]$Owner)
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$PSNativeCommandUseErrorActionPreference=$false
$repo='D:\WorksArea\SUPERVM'
$taskRoot="$repo\target\aoem-latest-57090b6b-20261117"
$state='C:\ProgramData\NOVOVM\selfrun\91129655606029\A'
$oldWork="$state\novovm-common-db667aa4-20261117"
$work="$state\novovm-common-5b656189-20261117"
$release='C:\ProgramData\NOVOVM\releases\novovm-common-5b656189-windows-x86_64'
$oldRelease='C:\ProgramData\NOVOVM\releases\novovm-common-db667aa4-windows-x86_64'
$source='5b656189784c854ac356aae77610c931b9f2189d'
$exe="$release\bin\novovm-node.exe"
$nodePin='4b0d03e8ad7a5a93051cee2672d38aa9a4ec8c0cb99669ae89af62b8eeef4038'
$oldNodePin='442afbc0e5c5aad9f2f952b5dbff9a08021556114c07a7698c65acf629273825'
$corePin='f9e8ded21753dfd32573de7bf66da8a785120444e810ba9fa865bfa4d639fcaa'
$manifestPin='6bf12700acb241fb2cc802057f04f53d883c5dbb8eddfd79f57221adf1638cd3'
$newTask='NOVOVM-A-5b656189-Existing-20261117'
$oldTask='NOVOVM-A-db667aa4-Existing-20261117'
if($Owner){
 try{
  if([Security.Principal.WindowsIdentity]::GetCurrent().User.Value -cne 'S-1-5-19'){throw 'LocalService required'}
  foreach($p in @(@($exe,$nodePin),@("$release/aoem/windows/core/bin/aoem_ffi.dll",$corePin),@("$release/release-manifest.json",$manifestPin))){if((Get-FileHash $p[0]).Hash.ToLowerInvariant() -cne $p[1]){throw 'Fixed owner package differs'}}
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
  & $exe 1> "$work/logs/node.stdout" 2> "$work/logs/node.stderr"
  $code=$LASTEXITCODE
  @{utc=[datetime]::UtcNow.ToString('o');exit_code=$code;start='existing'}|ConvertTo-Json|Set-Content "$work/logs/exit.json"
  exit $code
 }catch{@{utc=[datetime]::UtcNow.ToString('o');error=$_.Exception.Message}|ConvertTo-Json|Set-Content "$work/logs/error.json";exit 1}
}
$principal=[Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())
if(!$principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)){throw 'Administrator required'}
$full=Get-Content "$PSScriptRoot/full-final.json" -Raw|ConvertFrom-Json
if($full.head_sha -cne $source -or $full.status -cne 'completed' -or $full.conclusion -cne 'success'){throw 'Exact Full not passed'}
$q=Get-Content "$PSScriptRoot/capacity8-final-windows-qualification.json" -Raw|ConvertFrom-Json
if($q.source -cne $source -or $q.native_recovery_exit -ne 0 -or !$q.all_members_verified -or $q.node_sha256 -cne $nodePin -or $q.manifest_sha256 -cne $manifestPin){throw 'Fixed qualification differs'}
$original="$repo/scripts/windows/Update-NovonetExisting.ps1"
if((Get-FileHash $original).Hash.ToLowerInvariant() -cne '7b50ae0c70ba039dce93ae44b8d18b541e77d3c4ff45e95eedf60c3a0d29d5cd'){throw 'Original helpers changed'}
$tokens=$null;$errors=$null
$ast=[Management.Automation.Language.Parser]::ParseFile($original,[ref]$tokens,[ref]$errors)
if($errors.Count){throw 'Original helpers parse failed'}
foreach($name in @('Write-NewJson','Assert-NoRedirect','Set-OwnerAcl','Get-CustodyHashes','Assert-FirewallFilterScope')){
 $fn=@($ast.FindAll({param($n) $n -is [Management.Automation.Language.FunctionDefinitionAst]},$false)|Where-Object Name -CEQ $name)
 if($fn.Count -ne 1){throw 'Original helper missing'}
 . ([scriptblock]::Create($fn[0].Extent.Text))
}
. "$repo/scripts/windows/NovonetExistingProcess.ps1"
. "$repo/artifacts/audit/novonet-single-relay-20261111/Read-RelayProgress.ps1"
foreach($path in @($work,$release)){if(Test-Path -LiteralPath $path){throw 'Candidate path exists; inspect, do not repeat'}}
if(Get-ScheduledTask -TaskName $newTask -ErrorAction SilentlyContinue){throw 'Candidate task exists'}
$nodes=@((Get-NovonetNodeExecutionState).active)
if($nodes.Count -ne 1 -or $nodes[0].ExecutablePath -cne "$oldRelease\bin\novovm-node.exe" -or (Invoke-CimMethod $nodes[0] -MethodName GetOwnerSid).Sid -cne 'S-1-5-19'){throw 'Current A owner differs'}
$prior=Read-RelayProgress "$oldWork/logs/progress.json" $taskRoot
if($prior.recovery_in_progress -or $prior.novonet.diagnostic_relay.active){throw 'A still recovering or diagnostic busy'}
$config=Get-Content "$oldWork/run.json" -Raw|ConvertFrom-Json
$network=Get-Content $config.novonet_config_file -Raw|ConvertFrom-Json
if($config.database -cne "$state\chain-state" -or $config.profile -cne 'native_execution_tick' -or $config.udp_addr -cne '192.168.71.74:41051'){throw 'Original A config differs'}
$command='C:\ProgramData\NOVOVM\selfrun\91129655606029\A\novonet-observation-a25452f4-20261112\commands\control.nvprobe'
if($network.probe_control_file.Replace('/','\') -cne $command){throw 'Original command path differs'}
$keys=Get-CustodyHashes $config $network
$task=Get-ScheduledTask -TaskName $oldTask
if($task.State.ToString() -cne 'Running' -or $task.Settings.RestartCount -ne 0 -or @($task.Triggers|Where-Object {$null -ne $_}).Count -ne 0){throw 'Old task lifecycle differs'}
$package=[IO.Path]::GetFullPath("$repo/artifacts/product-overlay/novovm-common-5b656189-aoem570-windows-x86_64")
if((Get-FileHash "$package/release-manifest.json").Hash.ToLowerInvariant() -cne $manifestPin){throw 'Fixed manifest differs'}
$manifest=Get-Content "$package/release-manifest.json" -Raw|ConvertFrom-Json
if($manifest.git_commit -cne $source -or $manifest.artifacts.Count -ne 91){throw 'Fixed source/count differs'}
foreach($entry in $manifest.artifacts){$path=[IO.Path]::GetFullPath((Join-Path $package $entry.path));if(!$path.StartsWith($package+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'Member escaped'};Assert-NoRedirect $path;if((Get-Item $path).Length -ne $entry.bytes -or (Get-FileHash $path).Hash.ToLowerInvariant() -cne $entry.sha256){throw 'Fixed member differs'}}
Copy-Item -LiteralPath $package -Destination $release -Recurse
foreach($item in @((Get-Item $release))+@(Get-ChildItem $release -Recurse -Force)){Set-OwnerAcl $item.FullName}
New-Item -ItemType Directory -Path "$work/logs"|Out-Null
Set-OwnerAcl $work
Set-OwnerAcl "$work/logs" -Writable
Write-NewJson "$work/before-private.json" @{prior=$prior;key_hashes=$keys;pid=$nodes[0].ProcessId;created=$nodes[0].CreationDate;old_task=$oldTask}
Set-OwnerAcl "$work/before-private.json"
$config.library="$release\aoem\windows\core\bin\aoem_ffi.dll"
Write-NewJson "$work/run.json" $config
Set-OwnerAcl "$work/run.json"
Copy-Item -LiteralPath $PSCommandPath -Destination "$work/Start-Owner.ps1"
Set-OwnerAcl "$work/Start-Owner.ps1"
$rule=Get-NetFirewallRule -Name 'NOVOVM-db667aa4-A-20261117'
if($rule.Enabled -ne 'True' -or $rule.Action -ne 'Allow' -or ($rule|Get-NetFirewallApplicationFilter).Program -ine "$oldRelease\bin\novovm-node.exe"){throw 'Original narrow program rule differs'}
$newRuleName='NOVOVM-5b656189-A-20261117'
if(Get-NetFirewallRule -Name $newRuleName -ErrorAction SilentlyContinue){throw 'New rule already exists'}
Copy-NetFirewallRule -Name $rule.Name -NewName $newRuleName|Out-Null
$fresh=Get-NetFirewallRule -Name $newRuleName
$fresh|Get-NetFirewallApplicationFilter|Set-NetFirewallApplicationFilter -Program $exe
foreach($cmd in @('Get-NetFirewallPortFilter','Get-NetFirewallAddressFilter','Get-NetFirewallInterfaceFilter','Get-NetFirewallInterfaceTypeFilter','Get-NetFirewallSecurityFilter','Get-NetFirewallServiceFilter')){Assert-FirewallFilterScope (& $cmd -AssociatedNetFirewallRule $rule) (& $cmd -AssociatedNetFirewallRule $fresh) $rule.Name $fresh.Name}
Stop-NovonetExactProcess -ProcessId $nodes[0].ProcessId -ExpectedPath "$oldRelease\bin\novovm-node.exe" -ExpectedSha256 $oldNodePin
Copy-Item -LiteralPath "$oldWork/logs" -Destination "$work/old-logs" -Recurse
if(Test-Path -LiteralPath $command){Assert-NoRedirect $command;Copy-Item -LiteralPath $command -Destination "$work/completed-old-control.nvprobe";Remove-Item -LiteralPath $command}
if(@(Get-NetUDPEndpoint|Where-Object LocalPort -in @(41051,42051)).Count){throw 'Original ports still owned'}
$action=New-ScheduledTaskAction -Execute 'C:\Program Files\PowerShell\7\pwsh.exe' -Argument "-NoProfile -ExecutionPolicy RemoteSigned -File $work\Start-Owner.ps1 -Owner" -WorkingDirectory $work
$account=New-ScheduledTaskPrincipal -UserId 'NT AUTHORITY\LOCAL SERVICE' -LogonType ServiceAccount
$settings=New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([timespan]::Zero) -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
Register-ScheduledTask -TaskName $newTask -Action $action -Principal $account -Settings $settings|Out-Null
Start-ScheduledTask -TaskName $newTask
$clock=[Diagnostics.Stopwatch]::StartNew()
do{
 if((Test-Path "$work/logs/error.json") -or (Test-Path "$work/logs/exit.json")){throw 'New node failed; preserve logs; no automatic retry'}
 try{$current=Get-Content "$work/logs/progress.json" -Raw -ErrorAction Stop|ConvertFrom-Json -ErrorAction Stop}catch{$current=$null}
 if($null -ne $current -and $current.entry -ceq 'original_native_execution_tick' -and $current.recovery_in_progress -eq $false){break}
 if($clock.Elapsed.TotalSeconds -gt 120){throw 'No existing readiness; preserve node and logs'}
 Start-Sleep -Milliseconds 500
}while($true)
if($current.current_height -ne $prior.current_height -or ($current.head|ConvertTo-Json -Compress) -cne ($prior.head|ConvertTo-Json -Compress) -or $current.execution_failures -ne 0){throw 'Original head/height differs'}
if(($keys|ConvertTo-Json -Compress) -cne ((Get-CustodyHashes $config $network)|ConvertTo-Json -Compress)){throw 'Original custody changed'}
$nodes=@((Get-NovonetNodeExecutionState).active)
if($nodes.Count -ne 1 -or $nodes[0].ExecutablePath -cne $exe -or (Invoke-CimMethod $nodes[0] -MethodName GetOwnerSid).Sid -cne 'S-1-5-19'){throw 'New owner differs'}
$module=@((Get-Process -Id $nodes[0].ProcessId).Modules|Where-Object ModuleName -eq 'aoem_ffi.dll')
if($module.Count -ne 1 -or $module[0].FileName -ine "$release\aoem\windows\core\bin\aoem_ffi.dll" -or (Get-FileHash $module[0].FileName).Hash.ToLowerInvariant() -cne $corePin){throw 'Loaded AOEM differs'}
$ports=@(Get-NetUDPEndpoint|Where-Object OwningProcess -eq $nodes[0].ProcessId|Select-Object LocalAddress,LocalPort,OwningProcess)
if(41051 -notin $ports.LocalPort -or 42051 -notin $ports.LocalPort){throw 'Original listeners unavailable'}
Write-NewJson "$work/health-public.json" @{status='EXISTING_RUNNING_HEALTHY_NOT_DELIVERY';utc=[datetime]::UtcNow.ToString('o');source=$source;pid=$nodes[0].ProcessId;node_sha256=$nodePin;aoem_sha256=$corePin;actual_aoem=$module[0].FileName;manifest_sha256=$manifestPin;start='existing';database=$config.database;sid='S-1-5-19';height=$current.current_height;head=$current.head;custody_unchanged=$true;execution_failures=0;listeners=$ports;initialize=$false;config="$work/run.json";progress="$work/logs/progress.json"}
Write-Output 'A_5B_EXISTING_RUNNING; authenticated query remains separate'
