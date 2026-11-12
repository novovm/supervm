#Requires -Version 7.0
#Requires -RunAsAdministrator
param([Parameter(Mandatory=$true)][string]$EvidenceRoot)
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$oldRelease='C:\ProgramData\NOVOVM\releases\novonet-observation-a25452f4-windows-x86_64'
$oldWork='C:\ProgramData\NOVOVM\selfrun\91129655606029\C\novonet-observation-a25452f4-20261112'
$oldTask='NOVOVM-C-a25452f4-Existing-20261112'
$newTask='NOVOVM-C-a254-AOEM966-Existing-20261113'
$newRoot='C:\ProgramData\NOVOVM\releases\aoem-9662470d-c-20261113'
$newAoem=Join-Path $newRoot 'aoem'
$newWork='C:\ProgramData\NOVOVM\selfrun\91129655606029\C\a254-aoem966-existing-20261113'
$bundle='D:\WEB3_AI\AOEM\target-c-9662470d-fullmax\bundle\windows\20261113-072438'
$zip='E:\区块链四机通讯\aoem-fullmax-9662470d-windows-candidate.zip'
$oldExe=Join-Path $oldRelease 'bin\novovm-node.exe'
$oldConfig=Join-Path $oldWork 'run.json'
$expectedNode='1758e4a79dd34d1688d8f7bf4b60f46ed9ced518186f42fdb16baf04c0d0c46d'
$expectedOldCore='1b764a36c1f782425237d40446d4ea827f56a66fb341b870855d84c15acdce50'
$expectedCore='5743f5875131330f8c8fec58cdc2a85aa39dc6e9689358609a5f0a09a01c0d52'
$expectedZip='7dadc5239503d969b44eb59d6d06ef8544e072b944d80d1f5549ef420497c8d2'
function Hash([string]$Path) { (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() }
function WriteJson([string]$Path,$Value) {
  if(Test-Path -LiteralPath $Path){throw "refusing overwrite: $Path"}
  [IO.File]::WriteAllText($Path,($Value|ConvertTo-Json -Depth 20)+[Environment]::NewLine,[Text.UTF8Encoding]::new($false))
}
function AssertNoRedirect([string]$Path) {
  $item=Get-Item -LiteralPath $Path -Force -ErrorAction Stop
  if(($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0){throw "reparse point: $Path"}
}
function CheckBundle([string]$Root) {
  $rows=Get-Content -LiteralPath (Join-Path $Root 'CHECKSUMS.sha256')
  $count=0
  foreach($row in $rows) {
    if($row -notmatch '^([a-fA-F0-9]{64})  ([^\\/].+)$'){throw 'invalid AOEM checksums row'}
    $expected=$Matches[1].ToLowerInvariant();$relative=$Matches[2]
    if($relative.Contains('..')){throw 'invalid AOEM relative path'}
    $path=Join-Path $Root $relative
    if(!(Test-Path -LiteralPath $path -PathType Leaf) -or (Hash $path) -cne $expected){throw "AOEM bundle mismatch: $relative"}
    $count++
  }
  if($count -ne 28){throw "AOEM component count changed: $count"}
}
$out=[IO.Path]::GetFullPath($EvidenceRoot)
if(!( $out.StartsWith('D:\WEB3_AI\SUPERVM\target\c-aoem-node-test-20261113\',[StringComparison]::OrdinalIgnoreCase))){throw 'EvidenceRoot outside intended workspace'}
if(Test-Path -LiteralPath $out){throw 'EvidenceRoot exists; do not overwrite evidence'}
if(Test-Path -LiteralPath $newRoot){throw 'New runtime already staged; inspect instead of retry'}
if(Test-Path -LiteralPath $newWork){throw 'New work already staged; inspect instead of retry'}
if(Get-ScheduledTask -TaskName $newTask -ErrorAction SilentlyContinue){throw 'New task already exists; inspect instead of retry'}
$oldTaskState=Get-ScheduledTask -TaskName $oldTask -ErrorAction Stop
$oldTaskInfo=Get-ScheduledTaskInfo -TaskName $oldTask -ErrorAction Stop
if($oldTaskState.State -ne 'Ready' -or [long]$oldTaskInfo.LastTaskResult -ne 3221225477 -or $oldTaskState.Principal.UserId -cne 'LOCAL SERVICE'){throw 'Old task state differs'}
if(@(Get-CimInstance Win32_Process -Filter "Name = 'novovm-node.exe'").Count -ne 0){throw 'Node process present'}
if(@(Get-NetUDPEndpoint|Where-Object LocalPort -in 41052,42052).Count -ne 0){throw 'C UDP listeners present'}
if((Hash $oldExe) -cne $expectedNode -or (Hash (Join-Path $oldRelease 'aoem\windows\core\bin\aoem_ffi.dll')) -cne $expectedOldCore){throw 'Original program/runtime differs'}
if((Hash $oldConfig) -cne 'e6bc8ff878740a706c5bf86c873bfab3724a449774355df085c9977db3242415'){throw 'Original config differs'}
$cfg=Get-Content -LiteralPath $oldConfig -Raw|ConvertFrom-Json
if($cfg.database -cne 'C:\ProgramData\NOVOVM\selfrun\91129655606029\C\chain-state' -or !(Test-Path -LiteralPath $cfg.database -PathType Container)){throw 'Original database differs'}
AssertNoRedirect $cfg.database
if((Hash $cfg.novonet_config_file) -cne 'a41b09211b2e45e4fbf5ab5e7b6a9d3b9e7f35180a34cfffcabe7d5d39369230'){throw 'Original network config differs'}
$network=Get-Content -LiteralPath $cfg.novonet_config_file -Raw|ConvertFrom-Json
$keyExpected=@{
 'validator'='8d9b8bb2150496e79e50fc7d3127bffce06f25dcf7ee7f956fb7b54cfc382b9e'
 'transport'='5894e458fcde653e3b0e97a0390d849edc2efa501295964ac77a0c50ab9dd956'
 'carrier'='fbbea2280acc1bc564dfa6f81727f09cbc583a5e79030534e1a9a86a349e6713'
}
if((Hash $cfg.signing_key_file) -cne $keyExpected.validator -or (Hash $cfg.transport_key_file) -cne $keyExpected.transport -or (Hash $network.carrier_identity_file) -cne $keyExpected.carrier){throw 'Protected identities changed'}
if((Hash $zip) -cne $expectedZip){throw 'AOEM ZIP changed'}
CheckBundle $bundle
if((Hash (Join-Path $bundle 'core\bin\aoem_ffi.dll')) -cne $expectedCore){throw 'AOEM core changed'}
New-Item -ItemType Directory -Path $out|Out-Null
WriteJson (Join-Path $out 'preflight.json') ([ordered]@{host='C';utc=[DateTime]::UtcNow.ToString('o');old_task=$oldTask;old_result=[long]$oldTaskInfo.LastTaskResult;node_sha256=$expectedNode;old_aoem_sha256=$expectedOldCore;new_aoem_sha256=$expectedCore;aoem_source='9662470d';database_existing=$true;no_node_before=$true;new_runtime=$newRoot;new_work=$newWork})
try {
  $windows=Join-Path $newAoem 'windows'
  New-Item -ItemType Directory -Path $windows -Force|Out-Null
  Copy-Item -Path (Join-Path $bundle '*') -Destination $windows -Recurse -Force
  CheckBundle $windows
  if((Hash (Join-Path $windows 'core\bin\aoem_ffi.dll')) -cne $expectedCore){throw 'Staged AOEM mismatch'}
  New-Item -ItemType Directory -Path (Join-Path $newAoem 'manifest') -Force|Out-Null
  WriteJson (Join-Path $newAoem 'manifest\aoem-manifest.json') ([ordered]@{
    package='aoem-fullmax-c-isolated-node-candidate';source_commit='9662470d';
    entries=@([ordered]@{name='core';platform='windows-x86_64';dll='windows/core/bin/aoem_ffi.dll';sha256=$expectedCore;abi_expected=1;capabilities_required=@{execute_ops_v2=$true}})
  })
  # Keep the original host runtime profile while changing the AOEM binaries.
  New-Item -ItemType Directory -Path (Join-Path $newAoem 'config') -Force|Out-Null
  $package=[IO.Compression.ZipFile]::OpenRead('E:\区块链四机通讯\novonet-observation-a25452f4-windows-x86_64.zip')
  try {
    $profileEntry=$package.GetEntry('novonet-observation-a25452f4-windows-x86_64/aoem/config/aoem-runtime-profile.json')
    if($null -eq $profileEntry){throw 'Original AOEM runtime profile missing'}
    $source=$profileEntry.Open()
    try {
      $target=[IO.File]::Create((Join-Path $newAoem 'config\aoem-runtime-profile.json'))
      try {$source.CopyTo($target)} finally {$target.Dispose()}
    } finally {$source.Dispose()}
  } finally {$package.Dispose()}
  New-Item -ItemType Directory -Path $newWork -Force|Out-Null
  New-Item -ItemType Directory -Path (Join-Path $newWork 'logs')|Out-Null
  $newCfg=Get-Content -LiteralPath $oldConfig -Raw|ConvertFrom-Json
  $newCfg.library=Join-Path $windows 'core\bin\aoem_ffi.dll'
  WriteJson (Join-Path $newWork 'run.json') $newCfg
  $ownerTemplate=Join-Path $PSScriptRoot 'C-Aoem966-Owner.ps1'
  Copy-Item -LiteralPath $ownerTemplate -Destination (Join-Path $newWork 'ExistingOwner.ps1') -ErrorAction Stop
  foreach($p in @($newRoot,$newWork)){
    & icacls.exe $p /grant '*S-1-5-19:(OI)(CI)M' /T /Q | Out-Null
    if($LASTEXITCODE -ne 0){throw "LocalService ACL failed: $p"}
  }
  $args='-NoProfile -NonInteractive -ExecutionPolicy RemoteSigned -File "'+(Join-Path $newWork 'ExistingOwner.ps1')+'"'
  $action=New-ScheduledTaskAction -Execute 'C:\Program Files\PowerShell\7\pwsh.exe' -Argument $args
  $principal=New-ScheduledTaskPrincipal -UserId 'NT AUTHORITY\LOCAL SERVICE' -LogonType ServiceAccount
  $settings=New-ScheduledTaskSettingsSet -ExecutionTimeLimit (New-TimeSpan -Hours 24 -Minutes 3) -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
  Register-ScheduledTask -TaskName $newTask -Action $action -Principal $principal -Settings $settings|Out-Null
  Start-ScheduledTask -TaskName $newTask
  WriteJson (Join-Path $out 'start-requested.json') ([ordered]@{host='C';utc=[DateTime]::UtcNow.ToString('o');task=$newTask;start='existing';node_sha256=$expectedNode;aoem_sha256=$expectedCore;identity_unchanged=$true;old_task_untouched=$true})
} catch {
  WriteJson (Join-Path $out 'stage-error.json') ([ordered]@{host='C';utc=[DateTime]::UtcNow.ToString('o');error=$_.Exception.Message;no_retry=$true})
  throw
}
