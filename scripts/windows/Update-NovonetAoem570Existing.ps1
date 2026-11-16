#Requires -Version 7
# Runtime-only upgrade of the original node; never initializes or changes keys.
param(
    [Parameter(Mandatory)][ValidateSet('C','D')][string]$HostId,
    [Parameter(Mandatory)][string]$RuntimeRoot,
    [Parameter(Mandatory)][string]$EvidenceRoot,
    [switch]$Owner
)
$ErrorActionPreference='Stop'
$PSNativeCommandUseErrorActionPreference=$false
$source='57090b6b430c3e8f3389186ec2e97cf370a6963a'
$corePin='f9e8ded21753dfd32573de7bf66da8a785120444e810ba9fa865bfa4d639fcaa'
$nodePin='1758e4a79dd34d1688d8f7bf4b60f46ed9ced518186f42fdb16baf04c0d0c46d'
$manifestPin='d85e8a9577c1525e5665be4b74e9f0af7f8c26221002734feca7ff4b98745c75'
$state="C:\ProgramData\NOVOVM\selfrun\91129655606029\$HostId"
$oldWork=Join-Path $state 'novonet-observation-a25452f4-20261112'
$nodeRoot='C:\ProgramData\NOVOVM\releases\novonet-observation-a25452f4-windows-x86_64'
$exe=Join-Path $nodeRoot 'bin\novovm-node.exe'
$task="NOVOVM-$HostId-AOEM57090b6b-Existing-20261117"
$destination='C:\ProgramData\NOVOVM\releases\aoem-57090b6b-windows-x86_64'
$out=[IO.Path]::GetFullPath($EvidenceRoot)
function Save($name,$value){$value|ConvertTo-Json -Depth 16|Set-Content -LiteralPath (Join-Path $out $name) -Encoding utf8}
function NoRedirect([string]$path){
    for($item=Get-Item -LiteralPath $path -Force;$null-ne $item;$item=if($item -is [IO.DirectoryInfo]){$item.Parent}else{$item.Directory}){
        if($item.Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Redirected target'}
    }
}
function Hash([string]$path){(Get-FileHash -LiteralPath $path).Hash.ToLowerInvariant()}
function AllowService([string]$path,[string]$rights){
    & icacls.exe $path /grant "*S-1-5-19:(OI)(CI)$rights"|Out-Null
    if($LASTEXITCODE -ne 0){throw 'LocalService ACL preparation failed'}
}
if($Owner){
    try{
        if([Security.Principal.WindowsIdentity]::GetCurrent().User.Value -ne 'S-1-5-19'){throw 'LocalService required'}
        if((Hash $exe) -ne $nodePin -or (Hash "$destination\core\bin\aoem_ffi.dll") -ne $corePin){throw 'Owner hash mismatch'}
        $env:AOEM_DLL_MANIFEST="$out\node-runtime-manifest.json"
        $env:NOVOVM_AOEM_MANIFEST=$env:AOEM_DLL_MANIFEST
        $env:AOEM_DLL_MANIFEST_REQUIRED='1'
        $env:NOVOVM_AOEM_PLUGIN_DIR="$destination\core\plugins"
        $env:AOEM_FFI_PLUGIN_DIR=$env:NOVOVM_AOEM_PLUGIN_DIR
        $env:AOEM_SEMANTIC_GRAPH_V2_WORKERS='1'
        $env:NOVOVM_NODE_MODE='native_execution_tick'
        $env:NOVOVM_NATIVE_EXECUTION_CONFIG="$out\run.json"
        $env:NOVOVM_NATIVE_EXECUTION_START='existing'
        Remove-Item Env:NOVOVM_NATIVE_EXECUTION_RUN_MS -ErrorAction SilentlyContinue
        $env:NOVOVM_PROFILE_NATIVE_DELIVERY='1'
        $env:NOVOVM_NATIVE_EXECUTION_PIPELINE_PROGRESS_REPORT_PATH="$out\logs\progress.json"
        & $exe 1> "$out\logs\node.stdout" 2> "$out\logs\node.stderr"
        Save 'owner-exit.json' @{exit_code=$LASTEXITCODE;utc=[datetime]::UtcNow.ToString('o')}
        exit $LASTEXITCODE
    }catch{Save 'owner-error.json' @{error=$_.Exception.Message;utc=[datetime]::UtcNow.ToString('o')};exit 1}
}
$admin=[Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())
if(!$admin.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)){throw 'Administrator PowerShell7 required'}
if(Test-Path -LiteralPath $out){throw 'Fresh EvidenceRoot required'}
New-Item -ItemType Directory -Path $out|Out-Null
NoRedirect $out
$startedNew=$false
try{
    $RuntimeRoot=(Resolve-Path -LiteralPath $RuntimeRoot).Path
    NoRedirect $RuntimeRoot
    if((Hash "$RuntimeRoot\manifest.json") -ne $manifestPin){throw 'Fixed bundle manifest mismatch'}
    $manifest=Get-Content "$RuntimeRoot\manifest.json" -Raw|ConvertFrom-Json
    if($manifest.source_commit -ne $source -or $manifest.validation.unified_compute_smoke -ne 'passed'){throw 'Unqualified source/package'}
    $seen=@{}
    foreach($line in Get-Content "$RuntimeRoot\CHECKSUMS.sha256"){
        $parts=$line -split '  ',2
        if($parts.Count -ne 2){throw 'Invalid checksum row'}
        $path=[IO.Path]::GetFullPath((Join-Path $RuntimeRoot $parts[1]))
        if(!$path.StartsWith($RuntimeRoot+'\',[StringComparison]::OrdinalIgnoreCase) -or $seen.ContainsKey($path)){throw 'Escaping/duplicate archive member'}
        NoRedirect $path
        if((Hash $path) -cne $parts[0]){throw 'Bundle member hash mismatch'}
        $seen[$path]=$true
    }
    $files=@(Get-ChildItem -LiteralPath $RuntimeRoot -File -Recurse)
    if($files.Count -ne $seen.Count+1){throw 'Unexpected bundle members'}
    if((Hash "$RuntimeRoot\core\bin\aoem_ffi.dll") -ne $corePin -or (Hash $exe) -ne $nodePin){throw 'Fixed DLL/node mismatch'}
    if(Get-ScheduledTask -TaskName $task -ErrorAction SilentlyContinue){throw 'New owner already exists; inspect rather than repeat'}
    if(Test-Path $destination){throw 'Destination already exists; inspect rather than overwrite'}
    $config=Get-Content "$oldWork\run.json" -Raw|ConvertFrom-Json
    if($config.database -cne (Join-Path $state 'chain-state')){throw 'Unexpected original database'}
    NoRedirect $config.database
    $network=Get-Content $config.novonet_config_file -Raw|ConvertFrom-Json
    $keyPaths=@($config.signing_key_file,$config.transport_key_file,$network.carrier_identity_file)
    $keyHashes=@($keyPaths|ForEach-Object {NoRedirect $_;Hash $_})
    $configBeforeHash=Hash "$oldWork\run.json"
    $oldProgressPath=Join-Path $oldWork 'logs\progress.json'
    $before=if(Test-Path $oldProgressPath){try{Get-Content $oldProgressPath -Raw|ConvertFrom-Json}catch{$null}}else{$null}
    Copy-Item -LiteralPath $RuntimeRoot -Destination $destination -Recurse
    AllowService $destination 'RX'
    $prior=Get-Content "$nodeRoot\aoem\manifest\aoem-manifest.json" -Raw|ConvertFrom-Json
    $required=@($prior.entries|Where-Object {$_.name -eq 'core' -and $_.platform -eq 'windows-x86_64'})
    if($required.Count -ne 1){throw 'Original capability contract ambiguous'}
    @{source_commit=$source;entries=@(@{name='core';platform='windows-x86_64';dll='core/bin/aoem_ffi.dll';sha256=$corePin;abi_expected=$required[0].abi_expected;capabilities_required=$required[0].capabilities_required})}|ConvertTo-Json -Depth 16|Set-Content "$out\node-runtime-manifest.json" -Encoding utf8
    $env:AOEM_DLL_MANIFEST="$out\node-runtime-manifest.json"
    $env:NOVOVM_AOEM_MANIFEST=$env:AOEM_DLL_MANIFEST
    $env:AOEM_DLL_MANIFEST_REQUIRED='1'
    & "$nodeRoot\bin\novovmctl.exe" native-identity qualify-runtime --library "$destination\core\bin\aoem_ffi.dll" --library-sha256 $corePin 1> "$out\runtime-qualification.json" 2> "$out\runtime-qualification.stderr"
    if($LASTEXITCODE -ne 0){throw 'New library qualification failed; old owner not stopped'}
    $qualified=Get-Content "$out\runtime-qualification.json" -Raw|ConvertFrom-Json
    if(!$qualified.ok -or !$qualified.data.verification_qualified){throw 'New library not qualified'}
    $nodes=@(Get-CimInstance Win32_Process -Filter "Name='novovm-node.exe'")
    if($nodes.Count -gt 1){throw 'Multiple node owners; preserve and inspect'}
    $stopped=@()
    foreach($n in $nodes){
        if($n.ExecutablePath -ine $exe -or (Invoke-CimMethod $n -MethodName GetOwnerSid).Sid -ne 'S-1-5-19'){throw 'Ambiguous existing node owner'}
        Save 'old-process.json' @{pid=$n.ProcessId;created=$n.CreationDate;exe_sha256=$nodePin}
        Stop-Process -Id $n.ProcessId -Force
        if(!(Wait-Process -Id $n.ProcessId -Timeout 20 -PassThru -ErrorAction SilentlyContinue).HasExited -and (Get-Process -Id $n.ProcessId -ErrorAction SilentlyContinue)){throw 'Old node still running'}
        $stopped+=@($n.ProcessId)
    }
    if(Get-CimInstance Win32_Process -Filter "Name='novovm-node.exe'"){throw 'Node remains; no database copy/start'}
    # Never deletes LOCK or initializes. Cold copy is made only after stop.
    Copy-Item -LiteralPath $config.database -Destination "$out\database-cold-copy" -Recurse
    $config.library="$destination\core\bin\aoem_ffi.dll"
    $config|ConvertTo-Json -Depth 100|Set-Content "$out\run.json" -Encoding utf8
    New-Item -ItemType Directory -Path "$out\logs"|Out-Null
    AllowService $out 'RX'
    AllowService "$out\logs" 'M'
    # Owner exit/error records go into logs as well as the original stream files.
    & icacls.exe $out /grant '*S-1-5-19:(OI)(CI)M'|Out-Null
    if($LASTEXITCODE -ne 0){throw 'Owner result ACL failed'}
    $arguments="-NoProfile -ExecutionPolicy RemoteSigned -File `"$PSCommandPath`" -Owner -HostId $HostId -RuntimeRoot `"$destination`" -EvidenceRoot `"$out`""
    $action=New-ScheduledTaskAction -Execute 'C:\Program Files\PowerShell\7\pwsh.exe' -Argument $arguments -WorkingDirectory $out
    $principal=New-ScheduledTaskPrincipal -UserId 'NT AUTHORITY\LOCAL SERVICE' -LogonType ServiceAccount
    $settings=New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([timespan]::Zero) -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
    Register-ScheduledTask -TaskName $task -Action $action -Principal $principal -Settings $settings|Out-Null
    Start-ScheduledTask -TaskName $task
    $startedNew=$true
    $deadline=[datetime]::UtcNow.AddMinutes(2)
    do{
        if(Test-Path "$out\owner-error.json"){throw (Get-Content "$out\owner-error.json" -Raw)}
        if(Test-Path "$out\owner-exit.json"){throw 'New node exited'}
        $progress=if(Test-Path "$out\logs\progress.json"){try{Get-Content "$out\logs\progress.json" -Raw|ConvertFrom-Json}catch{$null}}else{$null}
        if($progress -and $progress.entry -eq 'original_native_execution_tick' -and $progress.recovery_in_progress -eq $false){break}
        if([datetime]::UtcNow -gt $deadline){throw 'New owner health deadline'}
        Start-Sleep -Seconds 2
    }while($true)
    if($progress.execution_failures -ne 0){throw 'New execution failure'}
    if($before -and ($progress.current_height -ne $before.current_height -or ($progress.head|ConvertTo-Json -Compress) -cne ($before.head|ConvertTo-Json -Compress))){throw 'Original state projection differs'}
    if(($keyHashes -join ',') -cne (@($keyPaths|ForEach-Object {Hash $_}) -join ',') -or (Hash "$oldWork\run.json") -ne $configBeforeHash){throw 'Original config/identity changed'}
    $new=@(Get-CimInstance Win32_Process -Filter "Name='novovm-node.exe'"|Where-Object ExecutablePath -IEQ $exe)
    if($new.Count -ne 1){throw 'Ambiguous new owner'}
    $loaded=(Get-Process -Id $new[0].ProcessId).Modules|Where-Object ModuleName -eq 'aoem_ffi.dll'
    if(!$loaded -or (Hash $loaded.FileName) -ne $corePin){throw 'Actual loaded library differs'}
    Save 'result-public.json' @{status="$($HostId)_AOEM570_EXISTING_READY_FOR_A_QUERY";host=$HostId;utc=[datetime]::UtcNow.ToString('o');pid=$new[0].ProcessId;source=$source;node_sha256=$nodePin;loaded_aoem_sha256=$corePin;stopped_pids=$stopped;start='existing';height=$progress.current_height;execution_failures=$progress.execution_failures;identity_hashes_unchanged=$true;original_config_unchanged=$true;initialize=$false;four_host_acceptance=$false}
    Write-Output "$($HostId)_AOEM570_EXISTING_READY_FOR_A_QUERY"
}catch{
    if($startedNew){Stop-ScheduledTask -TaskName $task -ErrorAction SilentlyContinue}
    Save 'error-public.json' @{host=$HostId;utc=[datetime]::UtcNow.ToString('o');error=$_.Exception.Message;new_task_started=$startedNew;retry=$false}
    throw
}
