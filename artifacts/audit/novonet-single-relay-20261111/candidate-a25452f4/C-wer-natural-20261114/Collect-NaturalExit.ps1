#Requires -Version 7.0
#Requires -RunAsAdministrator
param([Parameter(Mandatory)][string]$EvidenceRoot)
$ErrorActionPreference='Stop'
if(Test-Path -LiteralPath $EvidenceRoot){throw 'New private evidence directory required'}
[IO.Directory]::CreateDirectory($EvidenceRoot)|Out-Null
# Protect this newly created destination only. Original service ACLs remain unchanged.
$acl=[Security.AccessControl.DirectorySecurity]::new();$acl.SetAccessRuleProtection($true,$false)
$me=[Security.Principal.WindowsIdentity]::GetCurrent().User
foreach($sid in @($me,[Security.Principal.SecurityIdentifier]::new('S-1-5-18'),[Security.Principal.SecurityIdentifier]::new('S-1-5-32-544'))){
 $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new($sid,'FullControl','ContainerInherit,ObjectInherit','None','Allow'))
}
Set-Acl -LiteralPath $EvidenceRoot -AclObject $acl
$utf8=[Text.UTF8Encoding]::new($false)
function Save($n,$v){[IO.File]::WriteAllText((Join-Path $EvidenceRoot $n),($v|ConvertTo-Json -Depth 12)+"`n",$utf8)}
function Hash($p){(Get-FileHash -LiteralPath $p).Hash.ToLowerInvariant()}
$source='C:\Windows\ServiceProfiles\LocalService\AppData\Local\CrashDumps\NOVOVM-Full-C-20261114T074716Z\novovm-node.exe.34452.dmp'
$work='C:\Windows\ServiceProfiles\LocalService\AppData\Local\NOVOVM-WER-Natural-C-20261114T080720Z'
$result=[ordered]@{start_utc=[DateTime]::UtcNow.ToString('o');node_started=$false;source_acl_changed=$false;source_modified=$false;exit_code=1}
try{
 if(@(Get-CimInstance Win32_Process -Filter "Name='novovm-node.exe'").Count){throw 'Node present; verify before collecting'}
 $before=Get-Item -LiteralPath $source;$hash=Hash $source
 if($hash -cne '7d910fa6713e399c6bb394f2201fec622771dbc8e0665894ce9b8c115c9231aa' -or $before.Length -ne 233403217){throw 'Original dump identity differs'}
 $dest=Join-Path $EvidenceRoot 'novovm-node.exe.34452.dmp';[IO.File]::Copy($source,$dest,$false)
 if((Hash $dest) -cne $hash){throw 'Copy hash differs'}
 $after=Get-Item -LiteralPath $source
 if($before.Length -ne $after.Length -or $before.LastWriteTimeUtc -ne $after.LastWriteTimeUtc){throw 'Source changed'}
 [IO.File]::SetCreationTimeUtc($dest,$before.CreationTimeUtc);[IO.File]::SetLastWriteTimeUtc($dest,$before.LastWriteTimeUtc)
 foreach($n in @('node.stdout','node.stderr','progress.json','owner.json','exit.json','owner-error.json')){
  $p=Join-Path $work ('logs\'+$n);if(Test-Path -LiteralPath $p){Copy-Item -LiteralPath $p -Destination (Join-Path $EvidenceRoot $n)}
 }
 $events=@(Get-WinEvent -FilterHashtable @{LogName='Application';Id=1000,1001;StartTime=[DateTime]::Parse('2026-11-14T09:10:00Z').ToLocalTime()} -ErrorAction SilentlyContinue|Where-Object {$_.Message -like '*novovm-node.exe*'}|ForEach-Object {@{record_id=$_.RecordId;utc=$_.TimeCreated.ToUniversalTime().ToString('o');id=$_.Id;xml=$_.ToXml()}})
 Save 'events-private.json' $events
 $result.dump_sha256=$hash;$result.dump_bytes=$before.Length;$result.creation_utc=$before.CreationTimeUtc.ToString('o');$result.last_write_utc=$before.LastWriteTimeUtc.ToString('o');$result.private_copy=$dest;$result.exit_code=0
}catch{$result.error=$_.Exception.ToString()}
$result.end_utc=[DateTime]::UtcNow.ToString('o');Save 'collection-public.json' $result
exit $result.exit_code
