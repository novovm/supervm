#Requires -Version 7.0
#Requires -RunAsAdministrator
param()
$ErrorActionPreference='Stop';Set-StrictMode -Version Latest
$base='D:\WEB3_AI\SUPERVM\target\c-first-fault-20261113'
$work=Join-Path $base 'run-private-01'
$tool=Join-Path $base 'sdk\extracted\Windows Kits\10\Debuggers\x64'
$taskName='NOVOVM-C-FirstFault-20261113-01'
$oldWork='C:\ProgramData\NOVOVM\selfrun\91129655606029\C\a254-aoem966-backtrace-20261113'
$ifeo='HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options'
$ownedFlags=[Collections.Generic.List[string]]::new()
$out=Join-Path $base 'launch-public-01'
function Save($name,$value){$p=Join-Path $out $name;if(Test-Path -LiteralPath $p){throw 'Evidence exists'};$value|ConvertTo-Json -Depth 8|Set-Content -LiteralPath $p}
function ClearOwnFlag([string]$image){
 if(!$ownedFlags.Contains($image)){return}
 & (Join-Path $tool 'gflags.exe') /p /disable $image >> (Join-Path $out 'gflags-public.log') 2>&1
 if($LASTEXITCODE -ne 0){throw "GFlags cleanup failed: $image"}
 $key=Join-Path $ifeo $image
 if(Test-Path -LiteralPath $key){
  $item=Get-Item -LiteralPath $key
  if($item.ValueCount -ne 0 -or $item.SubKeyCount -ne 0){throw "Residual/unexpected IFEO data: $image"}
  Remove-Item -LiteralPath $key
 }
 [void]$ownedFlags.Remove($image)
}
if(Test-Path -LiteralPath $out){throw 'No retry / launch evidence already exists'}
New-Item -ItemType Directory -Path $out|Out-Null
try{
 if(Test-Path -LiteralPath $work){throw 'No retry / private evidence already exists'}
 if(Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue){throw 'Task already exists'}
 if(@(Get-CimInstance Win32_Process -Filter "Name='novovm-node.exe'").Count -or @(Get-NetUDPEndpoint|Where-Object LocalPort -in 41052,42052).Count){throw 'Node or C listeners present'}
 $oldTask=Get-ScheduledTask -TaskName 'NOVOVM-C-a254-AOEM966-BT-20261113'
 $oldExit=Get-Content (Join-Path $oldWork 'logs\exit.json') -Raw|ConvertFrom-Json
 if($oldTask.State -ne 'Ready' -or $oldTask.Principal.UserId -cne 'LOCAL SERVICE' -or $oldExit.start -cne 'existing' -or $oldExit.exit_code -ne -1073741819){throw 'Previous owner/exit changed'}
 foreach($image in 'novovm-node.exe','c-pageheap-control.exe'){if(Test-Path -LiteralPath (Join-Path $ifeo $image)){throw "Existing IFEO settings: $image"}}
 foreach($binary in 'cdb.exe','gflags.exe','dbgeng.dll','dbghelp.dll'){
  $sig=Get-AuthenticodeSignature -LiteralPath (Join-Path $tool $binary)
  if($sig.Status -ne 'Valid' -or $sig.SignerCertificate.Subject -notmatch 'Microsoft Corporation'){throw "Debugger signature mismatch: $binary"}
 }
 if((Get-FileHash (Join-Path $base 'c-pageheap-control.exe')).Hash -cne '3CDE64EB56DB67A2A7465A019C970B9F90F2F2EE5C3E068C40811237282C81D4'){throw 'Control changed'}
 $cfg=Get-Content (Join-Path $oldWork 'run.json') -Raw|ConvertFrom-Json
 if($cfg.database -cne 'C:\ProgramData\NOVOVM\selfrun\91129655606029\C\chain-state' -or !(Test-Path -LiteralPath $cfg.database -PathType Container)){throw 'Original DB mismatch'}
 $net=Get-Content -LiteralPath $cfg.novonet_config_file -Raw|ConvertFrom-Json
 $protected=@($cfg.signing_key_file,$cfg.transport_key_file,$net.carrier_identity_file,$cfg.novonet_config_file)
 $before=@{};foreach($p in $protected){$before[$p]=(Get-FileHash -LiteralPath $p).Hash}
 New-Item -ItemType Directory -Path $work|Out-Null
 # Dumps and original configuration remain local, readable only by these owners.
 $sid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
 & icacls.exe $work /inheritance:r /grant:r "*$($sid):(OI)(CI)F" '*S-1-5-18:(OI)(CI)F' '*S-1-5-32-544:(OI)(CI)F' '*S-1-5-19:(OI)(CI)M' | Out-Null
 if($LASTEXITCODE -ne 0){throw 'Private output ACL failed'}
 Copy-Item -LiteralPath (Join-Path $oldWork 'run.json') -Destination (Join-Path $work 'run.json')
 Copy-Item -LiteralPath (Join-Path $base 'FirstFaultOwner.ps1') -Destination (Join-Path $work 'FirstFaultOwner.ps1')
 foreach($name in 'c-pageheap-control.exe','c-pageheap-control.pdb'){Copy-Item -LiteralPath (Join-Path $base $name) -Destination (Join-Path $tool $name)}
 & icacls.exe $tool /grant '*S-1-5-19:(OI)(CI)RX' /Q|Out-Null
 if($LASTEXITCODE -ne 0){throw 'Debugger read ACL failed'}
 foreach($image in 'c-pageheap-control.exe','novovm-node.exe'){
  $ownedFlags.Add($image)
  & (Join-Path $tool 'gflags.exe') /p /enable $image /full >> (Join-Path $out 'gflags-public.log') 2>&1
  if($LASTEXITCODE -ne 0){throw 'Enable full page heap failed'}
 }
 Save 'preflight-public.json' @{utc=[DateTime]::UtcNow.ToString('o');original_owner_exited=$true;database='existing';protected_identity_verified=$true;initial_ifeo_absent=$true;full_pageheap=$true;node_runtime_ms=1800000;hard_debugger_bound_seconds=1920;memory_bound_bytes=6GB;minimum_disk_free_bytes=10GB;no_relay_ready=$true}
 $args='-NoProfile -NonInteractive -File "'+(Join-Path $work 'FirstFaultOwner.ps1')+'" -WorkRoot "'+$work+'" -ToolRoot "'+$tool+'"'
 $action=New-ScheduledTaskAction -Execute 'C:\Program Files\PowerShell\7\pwsh.exe' -Argument $args
 $principal=New-ScheduledTaskPrincipal -UserId 'NT AUTHORITY\LOCAL SERVICE' -LogonType ServiceAccount
 $settings=New-ScheduledTaskSettingsSet -ExecutionTimeLimit (New-TimeSpan -Minutes 35) -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
 Register-ScheduledTask -TaskName $taskName -Action $action -Principal $principal -Settings $settings|Out-Null
 Start-ScheduledTask -TaskName $taskName
 $start=[DateTime]::UtcNow;$ready=$false
 while(([DateTime]::UtcNow-$start).TotalSeconds -lt 150){
  if(Test-Path (Join-Path $work 'error-public.json')){throw 'Diagnostic owner failed; preserve its error'}
  $log=Join-Path $work 'node-private.stdout'
  if(Test-Path $log){
   $text=Get-Content -LiteralPath $log -Raw
   if($text -match '(?m)^C_DIAG_READY\r?$' -and $text -match 'hpa - Enable page heap'){$ready=$true;break}
  }
  if(Test-Path (Join-Path $work 'result-public.json')){break}
  Start-Sleep -Seconds 1
 }
 # Per-image settings only affect creation; remove immediately after this launch.
 foreach($image in @($ownedFlags.ToArray())){ClearOwnFlag $image}
 Copy-Item -LiteralPath (Join-Path $work 'controls-public.json') -Destination (Join-Path $out 'controls-public.json')
 if(!$ready){throw 'Node pageheap startup not verified'}
 $debug=Get-Content (Join-Path $work 'node-debugger.json') -Raw|ConvertFrom-Json
 $nodes=@(Get-CimInstance Win32_Process -Filter "Name='novovm-node.exe'" | Where-Object ParentProcessId -eq $debug.pid | Select-Object ProcessId,ParentProcessId,CreationDate,ExecutablePath)
 Save 'diagnostic-ready-public.json' @{utc=[DateTime]::UtcNow.ToString('o');debugger=$debug;nodes=$nodes;pageheap_confirmed=$true;temporary_ifeo_removed=$true;relay_ready=$false;diagnostic_only=$true}
 $until=$start.AddMinutes(34)
 while([DateTime]::UtcNow -lt $until){
  if(Test-Path (Join-Path $work 'result-public.json')){Copy-Item (Join-Path $work 'result-public.json') (Join-Path $out 'result-public.json');break}
  if(Test-Path (Join-Path $work 'error-public.json')){Copy-Item (Join-Path $work 'error-public.json') (Join-Path $out 'owner-error-public.json');break}
  Start-Sleep -Seconds 5
 }
 $unchanged=$true;foreach($p in $protected){if((Get-FileHash -LiteralPath $p).Hash -cne $before[$p]){$unchanged=$false}}
 $nodes=@(Get-CimInstance Win32_Process -Filter "Name='novovm-node.exe'"|Select-Object ProcessId,ParentProcessId,CreationDate)
 Save 'end-public.json' @{utc=[DateTime]::UtcNow.ToString('o');identity_and_network_unchanged=$unchanged;nodes=$nodes;task_state=(Get-ScheduledTask -TaskName $taskName).State.ToString();root_cause_fixed=$false;relay_ready=$false}
}catch{
 Save 'launch-error-public.json' @{utc=[DateTime]::UtcNow.ToString('o');error=$_.Exception.Message;no_retry=$true}
 # Only this newly registered diagnostic task/debugger may be stopped on failure.
 if(Test-Path (Join-Path $work 'node-debugger.json')){
  $d=Get-Content (Join-Path $work 'node-debugger.json') -Raw|ConvertFrom-Json
  $p=Get-Process -Id $d.pid -ErrorAction SilentlyContinue
  if($p -and $p.Path -ceq $d.path -and $p.StartTime.ToUniversalTime().ToString('o') -ceq $d.created_utc){Stop-Process -Id $p.Id -Force}
 }
 exit 1
}finally{
 foreach($image in @($ownedFlags.ToArray())){ClearOwnFlag $image}
 Save 'registry-cleanup-public.json' @{utc=[DateTime]::UtcNow.ToString('o');node_ifeo_present=(Test-Path -LiteralPath (Join-Path $ifeo 'novovm-node.exe'));control_ifeo_present=(Test-Path -LiteralPath (Join-Path $ifeo 'c-pageheap-control.exe'))}
}
