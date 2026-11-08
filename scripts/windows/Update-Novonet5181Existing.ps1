#Requires -Version 7.0
# One-shot update of the existing A/C LocalService node; never initializes state.
param(
    [Parameter(Mandatory=$true)][ValidateSet('A','C')][string]$HostId,
    [string]$PackageRoot,
    [string]$EvidenceRoot,
    [switch]$Owner,
    [switch]$Rollback,
    [switch]$ResumePrepared,
    [switch]$RecoverCrashed
)
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$PSNativeCommandUseErrorActionPreference=$false
$source='5181da733d59d998afa898bb9964db5deae9e72c'
$manifestPin='f381ccd4eed0a9176e3b91aded8f699f19bb64bd983307c44f1dbf8086162ab3'
$nodePin='ff68c5930b9b205b25ba5351a17ad08aa68f51d7cf71779d0507b6269f4087f3'
$oldNodePin='19883c8b95fc059ec7838ff0556284b1ddd434b7a72bfbe93e3f3410f458a0fc'
$corePin='1b764a36c1f782425237d40446d4ea827f56a66fb341b870855d84c15acdce50'
$helperPin='659d2cdecf3e5ca0e58472a25c9f85b4f85a05095fca7b0cc0fc4277077f7b66'
$state="C:\ProgramData\NOVOVM\selfrun\91129655606029\$HostId"
$oldWork=Join-Path $state 'novonet-probe-ddd89e8e-20261108'
$work=Join-Path $state 'novonet-probe-5181da73-20261109'
$oldRelease='C:\ProgramData\NOVOVM\releases\novonet-probe-ddd89e8e-windows-x86_64'
$release='C:\ProgramData\NOVOVM\releases\novonet-probe-5181da73-windows-x86_64'
$oldTask="NOVOVM-$HostId-ddd89e8e-Existing-20261108"
$newTask="NOVOVM-$HostId-5181da73-Existing-20261109"
$rollbackTask="NOVOVM-$HostId-ddd89e8e-Existing-Rollback-20261109"
$oldExe=Join-Path $oldRelease 'bin\novovm-node.exe'
$newExe=Join-Path $release 'bin\novovm-node.exe'

function Write-NewJson([string]$Path,$Value) {
    $bytes=[Text.UTF8Encoding]::new($false).GetBytes(($Value|ConvertTo-Json -Depth 85))
    $file=[IO.File]::Open($Path,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
    try{$file.Write($bytes,0,$bytes.Length);$file.Flush($true)}finally{$file.Dispose()}
}
function Assert-NoRedirect([string]$Path) {
    $item=Get-Item -LiteralPath $Path -Force
    while($null -ne $item) {
        if($item.Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Redirected deployment path'}
        $item=if($item -is [IO.DirectoryInfo]){$item.Parent}else{$item.Directory}
    }
}
function Assert-Package([string]$Root) {
    $Root=[IO.Path]::GetFullPath($Root).TrimEnd('\')
    Assert-NoRedirect $Root
    if((Get-FileHash -LiteralPath "$Root/release-manifest.json").Hash.ToLowerInvariant() -cne $manifestPin){throw 'Fixed manifest mismatch'}
    $manifest=Get-Content -LiteralPath "$Root/release-manifest.json" -Raw|ConvertFrom-Json
    if($manifest.git_commit -cne $source -or $manifest.artifacts.Count -ne 77){throw 'Fixed source/count mismatch'}
    foreach($entry in $manifest.artifacts) {
        $path=[IO.Path]::GetFullPath((Join-Path $Root $entry.path))
        if(!$path.StartsWith($Root+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'Artifact escaped candidate'}
        Assert-NoRedirect $path
        $item=Get-Item -LiteralPath $path
        if($item.PSIsContainer -or $item.Length -ne $entry.bytes -or (Get-FileHash -LiteralPath $path).Hash.ToLowerInvariant() -cne $entry.sha256){throw 'Fixed artifact mismatch'}
    }
    if((Get-FileHash -LiteralPath "$Root/bin/novovm-node.exe").Hash.ToLowerInvariant() -cne $nodePin){throw 'Fixed node mismatch'}
}
function Set-OwnerAcl([string]$Path,[switch]$Writable) {
    Assert-NoRedirect $Path
    $item=Get-Item -LiteralPath $Path -Force
    $acl=if($item.PSIsContainer){[Security.AccessControl.DirectorySecurity]::new()}else{[Security.AccessControl.FileSecurity]::new()}
    $acl.SetAccessRuleProtection($true,$false)
    foreach($sid in @('S-1-5-18','S-1-5-32-544','S-1-5-19')) {
        $rights=if($sid -ceq 'S-1-5-19'){if($Writable){[Security.AccessControl.FileSystemRights]::Modify}else{[Security.AccessControl.FileSystemRights]::ReadAndExecute}}else{[Security.AccessControl.FileSystemRights]::FullControl}
        $inherit=if($item.PSIsContainer){[Security.AccessControl.InheritanceFlags]'ContainerInherit,ObjectInherit'}else{[Security.AccessControl.InheritanceFlags]::None}
        $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new([Security.Principal.SecurityIdentifier]::new($sid),$rights,$inherit,[Security.AccessControl.PropagationFlags]::None,[Security.AccessControl.AccessControlType]::Allow))
    }
    Set-Acl -LiteralPath $Path -AclObject $acl
}
function Get-CustodyHashes($Config,$Network) {
    $keys=@{}
    foreach($path in @($Config.signing_key_file,$Config.transport_key_file,$Network.carrier_identity_file)) {
        Assert-NoRedirect $path
        $keys[$path]=(Get-FileHash -LiteralPath $path).Hash.ToLowerInvariant()
    }
    return $keys
}
function Assert-FirewallFilterScope($OldFilter,$NewFilter,[string]$OldName,[string]$NewName) {
    if($OldFilter.CimClass.CimClassName -cne $NewFilter.CimClass.CimClassName){throw 'Copied firewall filter class differs'}
    # Windows embeds the associated rule name here; a clone MUST bind its new rule.
    $prefix='MSFT|FW|FirewallRule|'
    if($OldFilter.CreationClassName -cne ($prefix+$OldName) -or $NewFilter.CreationClassName -cne ($prefix+$NewName)){throw 'Copied firewall filter rule association differs'}
    $properties=@($OldFilter.CimClass.CimClassProperties.Name|Where-Object {$_ -notin @('InstanceID','Caption','Description','ElementName','CreationClassName')})
    if(($OldFilter|Select-Object $properties|ConvertTo-Json -Depth 10 -Compress) -cne ($NewFilter|Select-Object $properties|ConvertTo-Json -Depth 10 -Compress)){throw 'Copied firewall filter scope differs'}
}
function Assert-CrashedOwnerEvidence($Evidence,[string]$ExpectedPath) {
    if($Evidence.task_state -cne 'Ready' -or $Evidence.task_sid -cne 'S-1-5-19' -or $Evidence.restart_count -ne 0 -or $Evidence.trigger_count -ne 0 -or $Evidence.task_exit_code -ne 3221225477){throw 'Crashed task lifecycle not proven'}
    if($Evidence.application -cne 'novovm-node.exe' -or $Evidence.application_path -cne $ExpectedPath -or $Evidence.exception_code -cne 'c0000005' -or $Evidence.fault_pid -le 0){throw 'Matching original application crash not proven'}
    if($Evidence.created_utc -lt $Evidence.last_run_utc.AddSeconds(-5) -or $Evidence.created_utc -gt $Evidence.event_utc -or $Evidence.event_utc -lt $Evidence.last_run_utc -or $Evidence.event_utc -gt [DateTime]::UtcNow.AddSeconds(5)){throw 'Original crash timeline differs'}
}
function ConvertFrom-NovonetCrashEvent([xml]$EventXml,[DateTime]$TimeCreatedUtc,[long]$RecordId,[string]$ExpectedPath) {
    $fields=@{}
    foreach($field in $EventXml.SelectNodes("/*[local-name()='Event']/*[local-name()='EventData']/*[local-name()='Data']")) {
        # InnerText also represents empty XML elements; #text is absent in strict mode.
        $fields[$field.GetAttribute('Name')]=[string]$field.InnerText
    }
    if($fields['AppPath'] -cne $ExpectedPath -or $fields['ExceptionCode'] -cne 'c0000005'){return}
    foreach($required in @('AppName','ProcessId','ProcessCreationTime')) {
        if([string]::IsNullOrWhiteSpace($fields[$required])){throw "Matching crash event missing $required"}
    }
    [pscustomobject]@{event_id=1000;record_id=$RecordId;event_utc=$TimeCreatedUtc;application=$fields['AppName'];application_path=$fields['AppPath'];exception_code=$fields['ExceptionCode'];fault_pid=[Convert]::ToInt32($fields['ProcessId'],16);created_utc=[DateTime]::FromFileTimeUtc([Convert]::ToInt64($fields['ProcessCreationTime'],16));report_id=$fields['IntegratorReportId']}
}
function Start-ExistingTask([string]$Name,[switch]$Old) {
    if(Get-ScheduledTask -TaskName $Name -ErrorAction SilentlyContinue){throw 'Task exists; inspect, do not repeat startup'}
    $arguments='-NoProfile -NonInteractive -ExecutionPolicy RemoteSigned -File "'+(Join-Path $work 'ExistingOwner.ps1')+'" -HostId '+$HostId+' -Owner'
    if($Old){$arguments+=' -Rollback'}
    $action=New-ScheduledTaskAction -Execute 'C:\Program Files\PowerShell\7\pwsh.exe' -Argument $arguments
    $principal=New-ScheduledTaskPrincipal -UserId 'NT AUTHORITY\LOCAL SERVICE' -LogonType ServiceAccount
    $settings=New-ScheduledTaskSettingsSet -ExecutionTimeLimit (New-TimeSpan -Hours 24 -Minutes 3) -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
    Register-ScheduledTask -TaskName $Name -Action $action -Principal $principal -Settings $settings|Out-Null
    Start-ScheduledTask -TaskName $Name
}
if($Owner) {
    if([Security.Principal.WindowsIdentity]::GetCurrent().User.Value -cne 'S-1-5-19'){throw 'Actual LocalService required'}
    $root=if($Rollback){$oldRelease}else{$release}
    $configPath=if($Rollback){Join-Path $oldWork 'run.json'}else{Join-Path $work 'run.json'}
    $logs=Join-Path $work $(if($Rollback){'rollback-logs'}else{'logs'})
    $config=Get-Content -LiteralPath $configPath -Raw|ConvertFrom-Json
    if($config.database -cne (Join-Path $state 'chain-state') -or !(Test-Path -LiteralPath $config.database -PathType Container)){throw 'Original existing database required'}
    Assert-NoRedirect $config.database
    if((Get-FileHash -LiteralPath $config.library).Hash.ToLowerInvariant() -cne $corePin){throw 'Original AOEM differs'}
    if($Rollback){if((Get-FileHash -LiteralPath "$root/bin/novovm-node.exe").Hash.ToLowerInvariant() -cne $oldNodePin){throw 'Rollback program mismatch'}}else{Assert-Package $root}
    Write-NewJson "$logs/owner.json" ([ordered]@{host_id=$HostId;sid='S-1-5-19';start='existing';config=$configPath;database=$config.database;initialize_executed=$false;keys_generated=$false;rollback=[bool]$Rollback})
    $env:AOEM_DLL_MANIFEST=Join-Path $root 'aoem\manifest\aoem-manifest.json'
    $env:NOVOVM_AOEM_MANIFEST=$env:AOEM_DLL_MANIFEST
    $env:AOEM_DLL_MANIFEST_REQUIRED='1'
    $env:AOEM_SEMANTIC_GRAPH_V2_WORKERS='1'
    $env:NOVOVM_NODE_MODE='native_execution_tick'
    $env:NOVOVM_NATIVE_EXECUTION_CONFIG=$configPath
    $env:NOVOVM_NATIVE_EXECUTION_START='existing'
    $env:NOVOVM_NATIVE_EXECUTION_RUN_MS='86400000'
    $env:NOVOVM_NATIVE_EXECUTION_PIPELINE_PROGRESS_REPORT_PATH=Join-Path $logs 'progress.json'
    & "$root/bin/novovm-node.exe" 1> "$logs/node.stdout" 2> "$logs/node.stderr"
    $code=$LASTEXITCODE
    Write-NewJson "$logs/exit.json" ([ordered]@{exit_code=$code;start='existing';time=(Get-Date).ToUniversalTime().ToString('o')})
    exit $code
}
$principal=[Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())
if(!$principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)){throw 'Administrator PowerShell required'}
$helper=Join-Path $PSScriptRoot 'NovonetExistingProcess.ps1'
$helperText=(Get-Content -LiteralPath $helper -Raw).Replace("`r`n","`n")
$helperDigest=[BitConverter]::ToString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes($helperText))).Replace('-','').ToLowerInvariant()
if($helperDigest -cne $helperPin){throw 'Original process barrier helper differs'}
. $helper
if([string]::IsNullOrWhiteSpace($EvidenceRoot)){throw 'Explicit fresh evidence directory required'}
$out=[IO.Path]::GetFullPath($EvidenceRoot)
if(Test-Path -LiteralPath $out){throw 'Evidence directory exists; preserve it instead of repeating'}
New-Item -ItemType Directory -Path $out|Out-Null
Assert-NoRedirect $out
$stopped=$false
try {
    if($Rollback) {
        if((Get-FileHash -LiteralPath $oldExe).Hash.ToLowerInvariant() -cne $oldNodePin){throw 'Rollback program mismatch'}
        $nodes=@((Get-NovonetNodeExecutionState).active)
        if($nodes.Count -gt 1){throw 'Unknown concurrent nodes; preserve state'}
        if($nodes.Count -eq 1){Stop-NovonetExactProcess -ProcessId $nodes[0].ProcessId -ExpectedPath $newExe -ExpectedSha256 $nodePin}else{Wait-NovonetNodeExit}
        Start-ExistingTask $rollbackTask -Old
        Write-NewJson "$out/rollback-requested.json" ([ordered]@{host_id=$HostId;start='existing';task=$rollbackTask;database=(Join-Path $state 'chain-state');initialize_executed=$false})
        exit
    }
    if([string]::IsNullOrWhiteSpace($PackageRoot)){throw 'Fixed package directory required'}
    Assert-Package $PackageRoot
    Assert-NoRedirect $state
    $config=Get-Content -LiteralPath "$oldWork/run.json" -Raw|ConvertFrom-Json
    $network=Get-Content -LiteralPath $config.novonet_config_file -Raw|ConvertFrom-Json
    if($config.database -cne (Join-Path $state 'chain-state')){throw 'Original database path differs'}
    Assert-NoRedirect $config.database
    if((Get-FileHash -LiteralPath $oldExe).Hash.ToLowerInvariant() -cne $oldNodePin -or (Get-FileHash -LiteralPath $config.library).Hash.ToLowerInvariant() -cne $corePin){throw 'Original program/runtime differs'}
    $execution=Get-NovonetNodeExecutionState
    $nodes=@($execution.active)
    $original=$null;$crash=$null
    $task=Get-ScheduledTask -TaskName $oldTask
    $taskSid=if($task.Principal.UserId.StartsWith('S-1-')){$task.Principal.UserId}else{([Security.Principal.NTAccount]::new($task.Principal.UserId)).Translate([Security.Principal.SecurityIdentifier]).Value}
    if($RecoverCrashed) {
        if($nodes.Count -ne 0){throw 'Crashed recovery requires no executing node; do not kill unknown processes'}
        $info=Get-ScheduledTaskInfo -TaskName $oldTask
        $crashes=@(Get-WinEvent -FilterHashtable @{LogName='Application';Id=1000;StartTime=$info.LastRunTime}|ForEach-Object {
            ConvertFrom-NovonetCrashEvent ([xml]$_.ToXml()) $_.TimeCreated.ToUniversalTime() $_.RecordId $oldExe
        }|Sort-Object event_utc -Descending)
        if($crashes.Count -lt 1){throw 'Original crash event missing; preserve state'}
        $crash=$crashes[0]
        $crash|Add-Member -NotePropertyMembers @{task_state=$task.State.ToString();task_sid=$taskSid;restart_count=[int]$task.Settings.RestartCount;trigger_count=@($task.Triggers|Where-Object {$null -ne $_}).Count;task_exit_code=[uint32]$info.LastTaskResult;last_run_utc=$info.LastRunTime.ToUniversalTime()}
        Assert-CrashedOwnerEvidence $crash $oldExe
        $expectedPorts=if($HostId -ceq 'A'){@(41051,42051)}else{@(41052,42052)}
        if(@(Get-NetUDPEndpoint|Where-Object {$_.LocalPort -in $expectedPorts}).Count){throw 'Original ports still have an owner; preserve state'}
        Wait-NovonetNodeExit
        Write-NewJson "$out/crash-recovery-preflight.json" ([ordered]@{crash=$crash;execution=$execution;no_executing_nodes=$true;original_ports_unbound=$true;terminated_processes_not_killed=$true})
    } else {
        if($nodes.Count -ne 1 -or $nodes[0].ExecutablePath -cne $oldExe -or (Invoke-CimMethod $nodes[0] -MethodName GetOwnerSid).Sid -cne 'S-1-5-19'){throw 'Original process identity not confirmed'}
        $original=$nodes[0]
        if($task.State -ne 'Running' -or [int]$task.Settings.RestartCount -ne 0 -or $taskSid -cne 'S-1-5-19'){throw 'Original task lifecycle differs'}
    }
    $progress=Get-Content -LiteralPath "$oldWork/logs/progress.json" -Raw|ConvertFrom-Json
    $progressAge=((Get-Date)-(Get-Item -LiteralPath "$oldWork/logs/progress.json").LastWriteTime).TotalSeconds
    if($progress.chain_id -ne 91129655606029 -or $progress.execution_failures -ne 0 -or (!$RecoverCrashed -and $progressAge -gt 30)){throw 'Original progress not live and healthy'}
    $keys=Get-CustodyHashes $config $network
    $profiles=@(Get-NetFirewallProfile|Select-Object Name,Enabled)
    Write-NewJson "$out/before.json" ([ordered]@{host_id=$HostId;time=(Get-Date).ToUniversalTime().ToString('o');pid=$(if($null -ne $original){$original.ProcessId}else{$null});exe=$oldExe;node_sha256=$oldNodePin;sid='S-1-5-19';database=$config.database;custody_sha256=$keys;config=$config;network=$network;progress=$progress;progress_age_seconds=$progressAge;prior_owner_crashed=[bool]$RecoverCrashed;crash=$crash;firewall_profiles=$profiles})
    if((Test-Path -LiteralPath $work) -and !$ResumePrepared){throw 'Prepared work exists; inspect before explicit resume'}
    if(Test-Path -LiteralPath $release){Assert-Package $release}else{Copy-Item -LiteralPath $PackageRoot -Destination $release -Recurse;Assert-Package $release}
    foreach($item in @((Get-Item -LiteralPath $release))+@(Get-ChildItem -LiteralPath $release -Recurse -Force)){Set-OwnerAcl $item.FullName}
    if(!(Test-Path -LiteralPath $work)){New-Item -ItemType Directory -Path $work|Out-Null}
    Set-OwnerAcl $work
    foreach($dir in @('logs','rollback-logs','commands')) {
        if(!(Test-Path -LiteralPath "$work/$dir")){New-Item -ItemType Directory -Path "$work/$dir"|Out-Null}
        Set-OwnerAcl "$work/$dir" -Writable:($dir -ne 'commands')
    }
    if(Test-Path -LiteralPath "$work/logs/owner.json"){throw 'New owner already started; inspect instead of repeated update'}
    Copy-Item -LiteralPath $PSCommandPath -Destination "$work/ExistingOwner.ps1" -Force
    Set-OwnerAcl "$work/ExistingOwner.ps1"
    $config.library=Join-Path $release 'aoem\windows\core\bin\aoem_ffi.dll'
    $config.novonet_config_file=Join-Path $work 'network.json'
    $network.probe_control_file=Join-Path $work 'commands\control.nvprobe'
    if(Test-Path -LiteralPath $network.probe_control_file){throw 'New command path must be empty'}
    foreach($pair in @(@('run.json',$config),@('network.json',$network))) {
        $path=Join-Path $work $pair[0]
        if(Test-Path -LiteralPath $path) {
            if(!$ResumePrepared -or ((Get-Content -LiteralPath $path -Raw|ConvertFrom-Json|ConvertTo-Json -Depth 85 -Compress) -cne ($pair[1]|ConvertTo-Json -Depth 85 -Compress))){throw 'Prepared config differs'}
        }else{Write-NewJson $path $pair[1]}
        Set-OwnerAcl $path
    }
    $rule=Get-NetFirewallRule -Name "NOVOVM-ddd89e8e-$HostId-20261108"
    $port=$rule|Get-NetFirewallPortFilter
    $app=$rule|Get-NetFirewallApplicationFilter
    if($rule.Enabled -ne 'True' -or $rule.Action -ne 'Allow' -or $rule.Direction -ne 'Inbound' -or $port.Protocol -ne 'UDP' -or $app.Program -ine $oldExe){throw 'Original narrow firewall rule differs'}
    $newRule="NOVOVM-5181da73-$HostId-20261109"
    if(Get-NetFirewallRule -Name $newRule -ErrorAction SilentlyContinue) {
        if(!$ResumePrepared){throw 'New firewall rule exists; inspect before explicit resume'}
    }else{
        Copy-NetFirewallRule -Name $rule.Name -NewName $newRule|Out-Null
        Get-NetFirewallRule -Name $newRule|Get-NetFirewallApplicationFilter|Set-NetFirewallApplicationFilter -Program $newExe
    }
    $freshRule=Get-NetFirewallRule -Name $newRule
    foreach($command in @('Get-NetFirewallPortFilter','Get-NetFirewallAddressFilter','Get-NetFirewallInterfaceFilter','Get-NetFirewallInterfaceTypeFilter','Get-NetFirewallSecurityFilter','Get-NetFirewallServiceFilter')) {
        $oldFilter=& $command -AssociatedNetFirewallRule $rule
        $newFilter=& $command -AssociatedNetFirewallRule $freshRule
        Assert-FirewallFilterScope $oldFilter $newFilter $rule.Name $freshRule.Name
    }
    if($freshRule.Profile -ne $rule.Profile -or $freshRule.Enabled -ne $rule.Enabled -or $freshRule.Direction -ne $rule.Direction -or $freshRule.Action -ne $rule.Action -or ($freshRule|Get-NetFirewallApplicationFilter).Program -ine $newExe){throw 'Copied firewall policy differs'}
    if(($profiles|ConvertTo-Json -Compress) -cne (@(Get-NetFirewallProfile|Select-Object Name,Enabled)|ConvertTo-Json -Compress)){throw 'Firewall profiles changed'}
    Write-NewJson "$out/firewall.json" ([ordered]@{old_rule=$rule.Name;new_rule=$newRule;old_preserved=$true;filters_copied_exactly=$true;program=$newExe;profiles_unchanged=$true})
    if(Get-ScheduledTask -TaskName $newTask -ErrorAction SilentlyContinue){throw 'New task already exists'}
    if($RecoverCrashed) {
        if((Get-ScheduledTask -TaskName $oldTask).State -ne 'Ready'){throw 'Old task became active; preserve state'}
        Wait-NovonetNodeExit
    }else{Stop-NovonetExactProcess -ProcessId $original.ProcessId -ExpectedPath $oldExe -ExpectedSha256 $oldNodePin}
    $stopped=$true
    Write-NewJson "$out/old-exit.json" ([ordered]@{pid=$(if($null -ne $original){$original.ProcessId}else{$crash.fault_pid});exe=$oldExe;process_handle_waited=$true;no_executing_node=$true;terminated_by_updater=(!$RecoverCrashed);scope='authorized original-database interrupted recovery; not graceful drain';time=(Get-Date).ToUniversalTime().ToString('o')})
    Start-ExistingTask $newTask
    for($i=0;$i -lt 120 -and !(Test-Path -LiteralPath "$work/logs/progress.json");$i++){Start-Sleep -Milliseconds 500}
    if(!(Test-Path -LiteralPath "$work/logs/progress.json")){throw 'No new original-owner progress'}
    $newExecution=Get-NovonetNodeExecutionState
    $new=@($newExecution.active)
    if($new.Count -ne 1 -or $new[0].ExecutablePath -cne $newExe -or (Invoke-CimMethod $new[0] -MethodName GetOwnerSid).Sid -cne 'S-1-5-19'){throw 'New process identity differs'}
    $current=Get-Content -LiteralPath "$work/logs/progress.json" -Raw|ConvertFrom-Json
    if($current.chain_id -ne 91129655606029 -or $current.execution_failures -ne 0 -or $current.current_height -ne $progress.current_height -or ($current.head|ConvertTo-Json -Compress) -cne ($progress.head|ConvertTo-Json -Compress)){throw 'Original state progress differs'}
    if(!$current.native_network_probe.enabled -or $current.native_network_probe.transport.udp_recovery_event_names.Count -ne 22){throw 'Original recovery observation not enabled'}
    $afterKeys=Get-CustodyHashes $config $network
    foreach($path in $keys.Keys){if($keys[$path] -cne $afterKeys[$path]){throw 'Custody changed'}}
    $modules=@((Get-Process -Id $new[0].ProcessId).Modules|Where-Object ModuleName -EQ 'aoem_ffi.dll')
    if($modules.Count -ne 1 -or $modules[0].FileName -ine $config.library -or (Get-FileHash -LiteralPath $modules[0].FileName).Hash.ToLowerInvariant() -cne $corePin){throw 'Actually loaded AOEM differs'}
    Write-NewJson "$out/after.json" ([ordered]@{host_id=$HostId;time=(Get-Date).ToUniversalTime().ToString('o');source=$source;pid=$new[0].ProcessId;exe=$newExe;node_sha256=$nodePin;sid='S-1-5-19';database=$config.database;start='existing';initialize_executed=$false;keys_generated=$false;custody_unchanged=$true;aoem_module=$modules[0].FileName;aoem_sha256=$corePin;old_task=(Get-ScheduledTask -TaskName $oldTask).State.ToString();new_task=$newTask;prior_owner_crashed=[bool]$RecoverCrashed;terminated_process_observations=$newExecution.exited;progress=$current;listeners=@(Get-NetUDPEndpoint|Where-Object OwningProcess -EQ $new[0].ProcessId|Select-Object LocalAddress,LocalPort,OwningProcess);new_command_file_present=(Test-Path -LiteralPath $network.probe_control_file)})
    Write-NewJson "$out/result.json" ([ordered]@{host_id=$HostId;source=$source;update_completed=$true;start='existing';database=$config.database;pid=$new[0].ProcessId;custody_unchanged=$true;progress_state_unchanged=$true;authenticated_state_root_external_check_required=$true;recovery_observer_available=$true;new_object_sent=$false;root_cause_resolved=$false;initialize_executed=$false;business_receipt=$false})
    Write-Output "PASS $HostId 5181da73 existing update; external authenticated Status and UDP recovery observation remain required"
} catch {
    $problem=$_.ToString()
    Write-NewJson "$out/error.json" ([ordered]@{error=$problem;old_node_stopped=($stopped -and !$RecoverCrashed);existing_start_attempted=$stopped;prior_owner_crashed=[bool]$RecoverCrashed;time=(Get-Date).ToUniversalTime().ToString('o')})
    if($stopped) {
        $remaining=@((Get-NovonetNodeExecutionState).active)
        if($remaining.Count -gt 1){throw 'Unknown concurrent processes; preserve state, no automatic rollback'}
        if($remaining.Count -eq 1){Stop-NovonetExactProcess -ProcessId $remaining[0].ProcessId -ExpectedPath $newExe -ExpectedSha256 $nodePin}else{Wait-NovonetNodeExit}
        Start-ExistingTask $rollbackTask -Old
        Write-NewJson "$out/rollback-requested.json" ([ordered]@{task=$rollbackTask;start='existing';database=$config.database;initialize_executed=$false})
    }
    throw $problem
}
