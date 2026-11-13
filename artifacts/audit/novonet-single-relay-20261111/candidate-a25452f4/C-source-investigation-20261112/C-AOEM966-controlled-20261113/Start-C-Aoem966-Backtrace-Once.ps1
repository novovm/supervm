#Requires -Version 7.0
#Requires -RunAsAdministrator
param([Parameter(Mandatory=$true)][string]$EvidenceRoot)
$ErrorActionPreference='Stop';Set-StrictMode -Version Latest
$priorTask='NOVOVM-C-a254-AOEM966-Existing-20261113'
$newTask='NOVOVM-C-a254-AOEM966-BT-20261113'
$priorWork='C:\ProgramData\NOVOVM\selfrun\91129655606029\C\a254-aoem966-existing-20261113'
$newWork='C:\ProgramData\NOVOVM\selfrun\91129655606029\C\a254-aoem966-backtrace-20261113'
$node='C:\ProgramData\NOVOVM\releases\novonet-observation-a25452f4-windows-x86_64\bin\novovm-node.exe'
$core='C:\ProgramData\NOVOVM\releases\aoem-9662470d-c-20261113\aoem\windows\core\bin\aoem_ffi.dll'
$out=[IO.Path]::GetFullPath($EvidenceRoot)
function GetSha([string]$p){(Get-FileHash -LiteralPath $p -Algorithm SHA256).Hash.ToLowerInvariant()}
function WriteJson([string]$p,$v){if(Test-Path -LiteralPath $p){throw "refuse overwrite: $p"};[IO.File]::WriteAllText($p,($v|ConvertTo-Json -Depth 8)+[Environment]::NewLine,[Text.UTF8Encoding]::new($false))}
if(!($out.StartsWith('D:\WEB3_AI\SUPERVM\target\c-aoem-node-test-20261113\',[StringComparison]::OrdinalIgnoreCase))){throw 'Evidence path outside workspace'}
if(Test-Path -LiteralPath $out){throw 'Evidence root exists'}
if(Test-Path -LiteralPath $newWork){throw 'Backtrace work exists; do not retry'}
if(Get-ScheduledTask -TaskName $newTask -ErrorAction SilentlyContinue){throw 'Backtrace task exists; do not retry'}
$t=Get-ScheduledTask -TaskName $priorTask -ErrorAction Stop
$ti=Get-ScheduledTaskInfo -TaskName $priorTask -ErrorAction Stop
if($t.State -ne 'Ready' -or [long]$ti.LastTaskResult -ne 1 -or $t.Principal.UserId -cne 'LOCAL SERVICE'){throw 'Previous task state changed'}
if(@(Get-CimInstance Win32_Process -Filter "Name='novovm-node.exe'").Count -ne 0 -or @(Get-NetUDPEndpoint|Where-Object LocalPort -in 41052,42052).Count -ne 0){throw 'Node or C listeners present'}
if((GetSha $node) -cne '1758e4a79dd34d1688d8f7bf4b60f46ed9ced518186f42fdb16baf04c0d0c46d' -or (GetSha $core) -cne '5743f5875131330f8c8fec58cdc2a85aa39dc6e9689358609a5f0a09a01c0d52'){throw 'Program/runtime changed'}
$oldExit=Get-Content -LiteralPath (Join-Path $priorWork 'logs\exit.json') -Raw|ConvertFrom-Json
if($oldExit.exit_code -ne 1 -or (GetSha (Join-Path $priorWork 'logs\node.stderr')) -cne '809bd7dccf8aeb7cf2f83860a3c9a6217a3b378fa2e2133989fed912a7d83885'){throw 'Previous failure evidence changed'}
$cfg=Get-Content -LiteralPath (Join-Path $priorWork 'run.json') -Raw|ConvertFrom-Json
if($cfg.database -cne 'C:\ProgramData\NOVOVM\selfrun\91129655606029\C\chain-state' -or $cfg.library -cne $core -or !(Test-Path -LiteralPath $cfg.database -PathType Container)){throw 'Existing config/database changed'}
if((GetSha $cfg.novonet_config_file) -cne 'a41b09211b2e45e4fbf5ab5e7b6a9d3b9e7f35180a34cfffcabe7d5d39369230'){throw 'Network config changed'}
$net=Get-Content -LiteralPath $cfg.novonet_config_file -Raw|ConvertFrom-Json
if((GetSha $cfg.signing_key_file) -cne '8d9b8bb2150496e79e50fc7d3127bffce06f25dcf7ee7f956fb7b54cfc382b9e' -or (GetSha $cfg.transport_key_file) -cne '5894e458fcde653e3b0e97a0390d849edc2efa501295964ac77a0c50ab9dd956' -or (GetSha $net.carrier_identity_file) -cne 'fbbea2280acc1bc564dfa6f81727f09cbc583a5e79030534e1a9a86a349e6713'){throw 'Protected identity changed'}
New-Item -ItemType Directory -Path $out|Out-Null
WriteJson (Join-Path $out 'preflight.json') ([ordered]@{host='C';utc=[DateTime]::UtcNow.ToString('o');prior_exit=1;prior_stderr_sha256='809bd7dccf8aeb7cf2f83860a3c9a6217a3b378fa2e2133989fed912a7d83885';node_sha256=(GetSha $node);aoem_sha256=(GetSha $core);database_existing=$true;identity_unchanged=$true;node_absent=$true})
try {
  New-Item -ItemType Directory -Path $newWork|Out-Null
  New-Item -ItemType Directory -Path (Join-Path $newWork 'logs')|Out-Null
  Copy-Item -LiteralPath (Join-Path $priorWork 'run.json') -Destination (Join-Path $newWork 'run.json')
  $owner=Get-Content -LiteralPath (Join-Path $priorWork 'ExistingOwner.ps1') -Raw
  $owner=$owner.Replace($priorWork,$newWork)
  if($owner -notmatch [regex]::Escape($newWork)){throw 'Owner path replacement failed'}
  $owner=$owner.Replace("  & `$exe 1>","  `$env:RUST_BACKTRACE='full'`n  & `$exe 1>")
  if($owner -notmatch "RUST_BACKTRACE='full'"){throw 'Backtrace setting missing'}
  [IO.File]::WriteAllText((Join-Path $newWork 'ExistingOwner.ps1'),$owner,[Text.UTF8Encoding]::new($false))
  & icacls.exe $newWork /grant '*S-1-5-19:(OI)(CI)M' /T /Q | Out-Null
  if($LASTEXITCODE -ne 0){throw 'LocalService ACL failed'}
  $args='-NoProfile -NonInteractive -ExecutionPolicy RemoteSigned -File "'+(Join-Path $newWork 'ExistingOwner.ps1')+'"'
  $action=New-ScheduledTaskAction -Execute 'C:\Program Files\PowerShell\7\pwsh.exe' -Argument $args
  $principal=New-ScheduledTaskPrincipal -UserId 'NT AUTHORITY\LOCAL SERVICE' -LogonType ServiceAccount
  $settings=New-ScheduledTaskSettingsSet -ExecutionTimeLimit (New-TimeSpan -Hours 24 -Minutes 3) -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
  Register-ScheduledTask -TaskName $newTask -Action $action -Principal $principal -Settings $settings|Out-Null
  Start-ScheduledTask -TaskName $newTask
  WriteJson (Join-Path $out 'start-requested.json') ([ordered]@{host='C';utc=[DateTime]::UtcNow.ToString('o');task=$newTask;start='existing';diagnostic='RUST_BACKTRACE=full';same_node=$true;same_aoem=$true;same_database=$true;previous_failure_preserved=$true})
} catch {
  WriteJson (Join-Path $out 'stage-error.json') ([ordered]@{host='C';utc=[DateTime]::UtcNow.ToString('o');error=$_.Exception.Message;no_retry=$true})
  throw
}
