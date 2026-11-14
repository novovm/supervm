#Requires -Version 7.0
param([Parameter(Mandatory)][string]$EvidenceRoot,[Parameter(Mandatory)][string]$ProbeExe)
$ErrorActionPreference='Stop'
$admin=[Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())
if(!$admin.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)){throw 'Administrator required'}
if(Test-Path -LiteralPath $EvidenceRoot){throw 'New evidence directory required'}
[IO.Directory]::CreateDirectory($EvidenceRoot)|Out-Null
$utf8=[Text.UTF8Encoding]::new($false)
function Save($name,$value){[IO.File]::WriteAllText((Join-Path $EvidenceRoot $name),($value|ConvertTo-Json -Depth 12)+"`n",$utf8)}
function Snapshot([string]$path){
 if(!(Test-Path -LiteralPath $path)){return @{exists=$false;path=$path;values=@()}}
 $k=Get-Item -LiteralPath $path
 return @{exists=$true;path=$path;values=@($k.GetValueNames()|Sort-Object|ForEach-Object {@{name=$_;kind=$k.GetValueKind($_).ToString();value=$k.GetValue($_,$null,[Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)}})}
}
function Same($a,$b){return (($a|ConvertTo-Json -Depth 12 -Compress) -ceq ($b|ConvertTo-Json -Depth 12 -Compress))}
function AssertPrivateInherited([string]$path){
 $acl=Get-Acl -LiteralPath $path
 $allowed=@('S-1-5-18','S-1-5-19','S-1-5-32-544','S-1-3-0')
 foreach($rule in $acl.Access){
  $sid=$rule.IdentityReference.Translate([Security.Principal.SecurityIdentifier]).Value
  if($rule.AccessControlType -eq 'Allow' -and $sid -notin $allowed){throw "Unexpected dump directory access: $sid"}
 }
 return @{owner=$acl.Owner;sddl=$acl.Sddl;acl_modified=$false}
}
$base='HKLM:\SOFTWARE\Microsoft\Windows\Windows Error Reporting\LocalDumps'
$nodeKey=Join-Path $base 'novovm-node.exe'
$probeName='novovm-wer-probe-C-20261114.exe'
$probeKey=Join-Path $base $probeName
$ae='HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\AeDebug'
$ae32='HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows NT\CurrentVersion\AeDebug'
$ifeo='HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options\novovm-node.exe'
$taskName='NOVOVM-C-WER-Qualification-'+[DateTime]::UtcNow.ToString('yyyyMMddTHHmmssZ')
$taskCreated=$false;$probeKeyCreated=$false;$nodeKeyCreated=$false;$committed=$false
$result=[ordered]@{start_utc=[DateTime]::UtcNow.ToString('o');node_started=$false;original_database_accessed=$false;acl_changed=$false;target_configured=$false;qualified=$false;error=$null;exit_code=1}
Save 'started.json' $result
try {
 $before=[ordered]@{global=Snapshot $base;node=Snapshot $nodeKey;ae=Snapshot $ae;ae32=Snapshot $ae32;ifeo=Snapshot $ifeo}
 Save 'registry-before.json' $before
 if($before.node.exists -or (Test-Path -LiteralPath $probeKey)){throw 'Existing per-image configuration: stop, do not overwrite'}
 foreach($a in @($before.ae,$before.ae32)){if(@($a.values|Where-Object {$_.name -eq 'Auto' -and [string]$_.value -eq '1'}).Count){throw 'Automatic debugging enabled; leave unchanged'}}
 if($before.ifeo.exists){throw 'Existing target IFEO: stop and review, do not change'}
 $app=Get-Content (Join-Path $PSScriptRoot 'applicability-public.json') -Raw|ConvertFrom-Json
 if($app.custom_crash_reporter_found_in_reviewed_source){throw 'Custom reporting incompatible'}
 $result.probe_sha256=(Get-FileHash -LiteralPath $ProbeExe).Hash.ToLowerInvariant()
 $profile='C:\Windows\ServiceProfiles\LocalService\AppData\Local'
 $parent=Join-Path $profile 'CrashDumps'
 if(!(Test-Path -LiteralPath $parent)){throw 'Expected service Profile dump directory absent'}
 if((Get-Item -LiteralPath $parent).Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Unexpected reparse point'}
 $parentAcl=AssertPrivateInherited $parent
 $disk=[IO.DriveInfo]::new('C:\');if($disk.AvailableFreeSpace -lt 8GB){throw 'Less than 8 GiB free; stop'}
 $stamp=[DateTime]::UtcNow.ToString('yyyyMMddTHHmmssZ')
 $probeRoot=Join-Path $parent ('WER-qualification-C-'+$stamp)
 $nodeFolder=Join-Path $parent ('NOVOVM-Full-C-'+$stamp)
 if((Test-Path $probeRoot) -or (Test-Path $nodeFolder)){throw 'Run directories already exist'}
 [IO.Directory]::CreateDirectory($probeRoot)|Out-Null
 $probeDumps=Join-Path $probeRoot 'dumps';[IO.Directory]::CreateDirectory($probeDumps)|Out-Null
 Save 'directory-before.json' @{parent=$parent;acl=$parentAcl;probe_root=$probeRoot;free_bytes=$disk.AvailableFreeSpace}
 AssertPrivateInherited $probeDumps|Out-Null
 Copy-Item -LiteralPath $ProbeExe -Destination (Join-Path $probeRoot $probeName)
 Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'ProbeOwner.ps1') -Destination (Join-Path $probeRoot 'ProbeOwner.ps1')
 New-Item -Path $probeKey|Out-Null;$probeKeyCreated=$true
 New-ItemProperty -LiteralPath $probeKey -Name DumpType -PropertyType DWord -Value 2|Out-Null
 New-ItemProperty -LiteralPath $probeKey -Name DumpCount -PropertyType DWord -Value 2|Out-Null
 New-ItemProperty -LiteralPath $probeKey -Name DumpFolder -PropertyType ExpandString -Value $probeDumps|Out-Null
 Save 'probe-config.json' (Snapshot $probeKey)
 $args='-NoProfile -NonInteractive -File "'+(Join-Path $probeRoot 'ProbeOwner.ps1')+'" -Root "'+$probeRoot+'"'
 $action=New-ScheduledTaskAction -Execute 'C:\Program Files\PowerShell\7\pwsh.exe' -Argument $args -WorkingDirectory $probeRoot
 $principal=New-ScheduledTaskPrincipal -UserId 'NT AUTHORITY\LOCAL SERVICE' -LogonType ServiceAccount
 $settings=New-ScheduledTaskSettingsSet -Hidden -ExecutionTimeLimit (New-TimeSpan -Minutes 3) -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
 Register-ScheduledTask -TaskName $taskName -Action $action -Principal $principal -Settings $settings|Out-Null;$taskCreated=$true
 Start-ScheduledTask -TaskName $taskName
 $limit=[DateTime]::UtcNow.AddSeconds(180)
 do {Start-Sleep -Seconds 2; $finished=Test-Path -LiteralPath (Join-Path $probeRoot 'probe-exit.json')} while(!$finished -and [DateTime]::UtcNow -lt $limit)
 if(!$finished){throw 'Bounded probe timed out; no target configuration'}
 $exit=Get-Content (Join-Path $probeRoot 'probe-exit.json') -Raw|ConvertFrom-Json
 Save 'probe-exit.json' $exit
 if($exit.sid -cne 'S-1-5-19' -or $exit.exit_code -notin @(-1073741819,3221225477)){throw 'Unexpected qualification identity/exit code'}
 & 'D:\Anaconda3\python.exe' (Join-Path $PSScriptRoot 'Verify-ProbeDump.py') $probeRoot (Join-Path $EvidenceRoot 'qualification-public.json') 1> (Join-Path $EvidenceRoot 'verification.stdout.txt') 2> (Join-Path $EvidenceRoot 'verification.stderr.txt')
 $result.verifier_exit=$LASTEXITCODE
 if($LASTEXITCODE -ne 0){throw 'Full dump verification failed; no target configuration'}
 $proof=Get-Content (Join-Path $EvidenceRoot 'qualification-public.json') -Raw|ConvertFrom-Json
 if(!$proof.qualified){throw 'Full dump proof absent'}
 $result.qualified=$true
 [IO.Directory]::CreateDirectory($nodeFolder)|Out-Null
 $targetAcl=AssertPrivateInherited $nodeFolder
 if(!(Same $before.node (Snapshot $nodeKey))){throw 'Target configuration changed concurrently'}
 New-Item -Path $nodeKey|Out-Null;$nodeKeyCreated=$true
 New-ItemProperty -LiteralPath $nodeKey -Name DumpType -PropertyType DWord -Value 2|Out-Null
 New-ItemProperty -LiteralPath $nodeKey -Name DumpCount -PropertyType DWord -Value 2|Out-Null
 New-ItemProperty -LiteralPath $nodeKey -Name DumpFolder -PropertyType ExpandString -Value $nodeFolder|Out-Null
 $after=[ordered]@{global=Snapshot $base;node=Snapshot $nodeKey;ae=Snapshot $ae;ae32=Snapshot $ae32;ifeo=Snapshot $ifeo}
 foreach($key in @('global','ae','ae32','ifeo')){if(!(Same $before[$key] $after[$key])){throw "Unrelated configuration changed: $key"}}
 $current=Get-ItemProperty -LiteralPath $nodeKey
 if($current.DumpType -ne 2 -or $current.DumpCount -ne 2 -or $current.DumpFolder -cne $nodeFolder){throw 'Target readback mismatch'}
 Save 'registry-after.json' $after
 Save 'target-directory.json' @{path=$nodeFolder;acl=$targetAcl;dump_count=2;retention_overwrites_oldest=$true}
 $result.target_configured=$true;$committed=$true;$result.exit_code=0
} catch {$result.error=$_.Exception.ToString()}
finally {
 try {
  if($taskCreated){
   $task=Get-ScheduledTask -TaskName $taskName
   if($task.State -eq 'Running'){
    # Stop only this unique qualification task; never touch a node task.
    Stop-ScheduledTask -TaskName $taskName
   }
   Save 'task-result.json' (Get-ScheduledTaskInfo -TaskName $taskName|Select-Object LastRunTime,LastTaskResult)
   Unregister-ScheduledTask -TaskName $taskName -Confirm:$false
  }
  if($probeKeyCreated){Remove-Item -LiteralPath $probeKey}
  if($nodeKeyCreated -and !$committed){Remove-Item -LiteralPath $nodeKey}
  $result.probe_key_removed=!(Test-Path -LiteralPath $probeKey)
  $result.task_removed=!(Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue)
 } catch {$result.cleanup_error=$_.Exception.ToString();$result.exit_code=2}
 $result.end_utc=[DateTime]::UtcNow.ToString('o');Save 'result-public.json' $result
}
exit $result.exit_code
