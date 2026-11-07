#Requires -Version 5.1
# Fixed 08236129 candidate; same approved domain, original owners and protected purposes.
param(
    [Parameter(Mandatory=$true)][ValidateSet('A','C','D')][string]$HostId,
    [Parameter(Mandatory=$true)][string]$PackageRoot,
    [Parameter(Mandatory=$true)][string]$InputsRoot,
    [switch]$Owner
)
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$PSNativeCommandUseErrorActionPreference=$false
$source='08236129320da2e71c5fa112d249fa7afc741e78'
$manifestPin='a1a4082dcff2af227738e7e37013ae9a41b2546ecf25474eccda22166cb2f506'
$corePin='1b764a36c1f782425237d40446d4ea827f56a66fb341b870855d84c15acdce50'
$state=Join-Path $env:ProgramData ('NOVOVM\selfrun\91129655606029\'+$HostId)
$work=Join-Path $state 'novonet-08236129-20261108'
$release=Join-Path $env:ProgramData 'NOVOVM\releases\novonet-08236129-windows-x86_64'
$taskName='NOVOVM-'+$HostId+'-08236129-Network-20261108'
function Assert-Frozen([string]$Root) {
    $Root=[IO.Path]::GetFullPath($Root).TrimEnd('\')
    $manifestFile=Join-Path $Root 'release-manifest.json'
    if((Get-FileHash -LiteralPath $manifestFile).Hash.ToLowerInvariant() -cne $manifestPin){throw 'Fixed manifest mismatch'}
    $manifest=Get-Content -LiteralPath $manifestFile -Raw -Encoding UTF8 | ConvertFrom-Json
    if($manifest.git_commit -cne $source -or $manifest.artifacts.Count -ne 77){throw 'Fixed package source/count mismatch'}
    foreach($entry in $manifest.artifacts){
        $path=[IO.Path]::GetFullPath((Join-Path $Root $entry.path))
        if(-not $path.StartsWith($Root+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'Artifact outside candidate'}
        $item=Get-Item -LiteralPath $path -Force
        if($item.PSIsContainer -or $item.Length -ne $entry.bytes -or (Get-FileHash -LiteralPath $path).Hash.ToLowerInvariant() -cne $entry.sha256){throw ('Artifact mismatch: '+$entry.path)}
    }
    foreach($item in @((Get-Item -LiteralPath $Root -Force))+@(Get-ChildItem -LiteralPath $Root -Recurse -Force)){
        if($item.Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Candidate contains redirected paths'}
    }
}
function Write-NewJson([string]$Path,$Value) {
    if(Test-Path -LiteralPath $Path){throw ('Refuse overwriting '+$Path)}
    $bytes=[Text.UTF8Encoding]::new($false).GetBytes(($Value|ConvertTo-Json -Depth 70))
    $file=[IO.File]::Open($Path,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
    try{$file.Write($bytes,0,$bytes.Length);$file.Flush($true)}finally{$file.Dispose()}
}
if($Owner){
    if([Security.Principal.WindowsIdentity]::GetCurrent().User.Value -cne 'S-1-5-19'){throw 'Actual LocalService account required'}
    Write-NewJson (Join-Path $work 'owner.json') ([ordered]@{host_id=$HostId;sid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value;account=[Security.Principal.WindowsIdentity]::GetCurrent().Name;purpose_keys_regenerated=$false;asset_keys_accessed=$false;service_registered=$false})
    Assert-Frozen $release
    $env:AOEM_DLL_MANIFEST=Join-Path $release 'aoem\manifest\aoem-manifest.json'
    $env:NOVOVM_AOEM_MANIFEST=$env:AOEM_DLL_MANIFEST
    $env:AOEM_DLL_MANIFEST_REQUIRED='1'
    $env:AOEM_SEMANTIC_GRAPH_V2_WORKERS='1'
    $ctl=Join-Path $release 'bin\novovmctl.exe'
    & $ctl native-genesis initialize --config (Join-Path $work 'initialize.json') --library-sha256 $corePin 1> (Join-Path $work 'initialize.stdout') 2> (Join-Path $work 'initialize.stderr')
    if($LASTEXITCODE -ne 0){throw 'Original initializer failed; preserve state and logs; never clean or retry create'}
    $initial=Get-Content -LiteralPath (Join-Path $work 'initialize.stdout') -Raw -Encoding UTF8 | ConvertFrom-Json
    if(-not $initial.ok -or $initial.data.initial_state_root -cne 'f6186d40260aa18e84b01101d56ca3e918aff799ef41d927aeab554495f56333'){throw 'Initialized root differs from approved root'}
    $env:NOVOVM_NODE_MODE='native_execution_tick'
    $env:NOVOVM_NATIVE_EXECUTION_CONFIG=Join-Path $work 'run.json'
    $env:NOVOVM_NATIVE_EXECUTION_START='existing'
    # Original soft run bound, NOT a production authentication/consensus TTL.
    $env:NOVOVM_NATIVE_EXECUTION_RUN_MS='86400000'
    $env:NOVOVM_NATIVE_EXECUTION_PIPELINE_PROGRESS_REPORT_PATH=Join-Path $work 'progress.json'
    & (Join-Path $release 'bin\novovm-node.exe') 1> (Join-Path $work 'node.stdout') 2> (Join-Path $work 'node.stderr')
    if($LASTEXITCODE -ne 0){throw 'Original node failed; preserve process output and original database'}
    exit 0
}
$identity=[Security.Principal.WindowsIdentity]::GetCurrent()
$principal=[Security.Principal.WindowsPrincipal]::new($identity)
if(-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)){throw 'Run this approved one-shot launcher in Administrator PowerShell'}
$PackageRoot=(Resolve-Path -LiteralPath $PackageRoot).Path
$InputsRoot=(Resolve-Path -LiteralPath $InputsRoot).Path
$commonFile=Join-Path $InputsRoot 'common-approved.json'
if((Get-FileHash -LiteralPath $commonFile).Hash.ToLowerInvariant() -cne 'a9b8a240fbdb2cb47281b2866b9c1cfa58ab078aefee271ad65b80aba4053eac'){throw 'Approved common input mismatch'}
if(-not (Test-Path -LiteralPath (Join-Path $InputsRoot 'bootstrap-B.json'))){throw 'Original signed B bootstrap missing'}
if((Test-Path -LiteralPath $work) -or (Test-Path -LiteralPath (Join-Path $state 'chain-state'))){throw 'Existing work/state: inspect, do not rerun initialization'}
if(Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue){throw 'Existing task: inspect, do not replace'}
foreach($purpose in @('validator','transport')){if(-not (Test-Path -LiteralPath (Join-Path $state ($purpose+'.protected')))){throw 'Original service purpose material missing; do not generate substitutes'}}
Assert-Frozen $PackageRoot
if(Test-Path -LiteralPath $release){Assert-Frozen $release}else{
    Copy-Item -LiteralPath $PackageRoot -Destination $release -Recurse
    Assert-Frozen $release
    # Candidate code is admin/SYSTEM-writable and LocalService-readable only.
    foreach($item in @((Get-Item -LiteralPath $release -Force))+@(Get-ChildItem -LiteralPath $release -Recurse -Force)){
        $acl=if($item.PSIsContainer){[Security.AccessControl.DirectorySecurity]::new()}else{[Security.AccessControl.FileSecurity]::new()}
        $acl.SetAccessRuleProtection($true,$false)
        foreach($sid in @('S-1-5-18','S-1-5-32-544','S-1-5-19')){
            $rights=if($sid -ceq 'S-1-5-19'){[Security.AccessControl.FileSystemRights]::ReadAndExecute}else{[Security.AccessControl.FileSystemRights]::FullControl}
            $rule=[Security.AccessControl.FileSystemAccessRule]::new([Security.Principal.SecurityIdentifier]::new($sid),$rights,[Security.AccessControl.AccessControlType]::Allow)
            $acl.AddAccessRule($rule)
        }
        Set-Acl -LiteralPath $item.FullName -AclObject $acl
    }
}
$common=Get-Content -LiteralPath $commonFile -Raw -Encoding UTF8|ConvertFrom-Json
$ids=@('A','B','C','D');$index=@{A=0;B=1;C=2;D=3};$ips=@{A='192.168.71.74';C='192.168.71.125';D='192.168.71.59'}
$ports=@{A=41051;B=41050;C=41052;D=41053}
$routes=@{A='192.168.71.74:41051';B='47.103.59.206:41050';C='192.168.71.125:41052';D='192.168.71.59:41053'}
if(-not (Get-NetIPAddress -AddressFamily IPv4 -IPAddress $ips[$HostId] -ErrorAction SilentlyContinue)){throw 'Current LAN coordinate differs: report actual address instead of changing identity or silently choosing a VPN interface'}
New-Item -ItemType Directory -Path $work|Out-Null
Copy-Item -LiteralPath (Join-Path $InputsRoot 'bootstrap-B.json') -Destination (Join-Path $work 'bootstrap-B.json')
$ownerScript=Join-Path $work 'Start-Novonet082Host.ps1'
Copy-Item -LiteralPath $PSCommandPath -Destination $ownerScript
$config=[ordered]@{profile='native_execution_tick';library=(Join-Path $release 'aoem\windows\core\bin\aoem_ffi.dll');database=(Join-Path $state 'chain-state');rpc_addr=$null;signing_key_file=(Join-Path $state 'validator.protected');transport_key_file=(Join-Path $state 'transport.protected');key_storage='protected_v1';udp_addr=($ips[$HostId]+':'+$ports[$HostId]);peers=@(foreach($peer in $ids){if($peer -cne $HostId){@{public_key=$common.genesis.validators[$index[$peer]].public_key;address=$routes[$peer]}}});native_client_limit=1;workers=1;batch_size=1024;genesis=$common.genesis;uca_initialization=$common.uca_initialization}
Write-NewJson (Join-Path $work 'initialize.json') $config
Write-NewJson (Join-Path $work 'network.json') ([ordered]@{relays=@('https://euc1-1.relay.n0.iroh.link/');nameserver='223.5.5.5';bind_address=($ips[$HostId]+':'+($ports[$HostId]+1000));public_record_file=(Join-Path $work 'locator.json');carrier_identity_file=(Join-Path $work 'carrier.protected');peer_records=@();discovery=$true;discovery_seeds=@((Join-Path $work 'bootstrap-B.json'));relay_forwarding=$true;fallback_after_ms=1000;retry_ms=1000;udp_recovery_ms=5000})
$config.learn_authenticated_peer_routes=$true
$config.novonet_config_file=Join-Path $work 'network.json'
Write-NewJson (Join-Path $work 'run.json') $config
$ruleName='NOVOVM-08236129-'+$HostId+'-20261108'
if(Get-NetFirewallRule -Name $ruleName -ErrorAction SilentlyContinue){throw 'Existing named firewall rule: inspect, do not replace'}
New-NetFirewallRule -Name $ruleName -DisplayName $ruleName -Direction Inbound -Action Allow -Protocol UDP -Program (Join-Path $release 'bin\novovm-node.exe') -LocalAddress $ips[$HostId] -LocalPort @($ports[$HostId],($ports[$HostId]+1000)) -RemoteAddress @('192.168.71.0/24','47.103.59.206') -Profile Any|Out-Null
$action=New-ScheduledTaskAction -Execute 'C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe' -Argument ('-NoProfile -NonInteractive -ExecutionPolicy RemoteSigned -File "'+$ownerScript+'" -HostId '+$HostId+' -PackageRoot "'+$release+'" -InputsRoot "'+$work+'" -Owner')
$account=New-ScheduledTaskPrincipal -UserId 'NT AUTHORITY\LOCAL SERVICE' -LogonType ServiceAccount
$settings=New-ScheduledTaskSettingsSet -ExecutionTimeLimit (New-TimeSpan -Hours 24 -Minutes 3) -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
Register-ScheduledTask -TaskName $taskName -Action $action -Principal $account -Settings $settings|Out-Null
Start-ScheduledTask -TaskName $taskName
Write-Output ('STARTED original-node acceptance task: '+$taskName)
Write-Output ('Preserve state/logs. Public locator: '+(Join-Path $work 'locator.json'))
Write-Output 'Do not submit carrier.protected or purpose keys. Task registration alone is NOT network acceptance.'
