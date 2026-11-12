#Requires -Version 7.0
#Requires -RunAsAdministrator
$ErrorActionPreference='Stop'
$r=$PSScriptRoot
$start=[DateTime]::UtcNow.ToString('o')
try {
 New-Item -ItemType Directory -Path "$r/dumps-local" -ErrorAction Stop|Out-Null
 $rows=@()
 foreach($id in @(27928,13020)) {
  $src="C:/Windows/ServiceProfiles/LocalService/AppData/Local/CrashDumps/novovm-node.exe.$id.dmp"
  if(!(Test-Path -LiteralPath $src)){$rows+=@{pid=$id;exists=$false};continue}
  $a=Get-Item -LiteralPath $src
  $h=(Get-FileHash -LiteralPath $src).Hash.ToLowerInvariant()
  $dst=Join-Path "$r/dumps-local" $a.Name
  Copy-Item -LiteralPath $src -Destination $dst -ErrorAction Stop
  if((Get-FileHash -LiteralPath $dst).Hash.ToLowerInvariant() -cne $h){throw 'Copy hash mismatch'}
  $rows+=@{pid=$id;exists=$true;bytes=$a.Length;sha256=$h;modified_utc=$a.LastWriteTimeUtc.ToString('o');copied_local_only=$true}
 }
 @{start_utc=$start;end_utc=[DateTime]::UtcNow.ToString('o');dumps=$rows;node_changed=$false;exit_code=0}|ConvertTo-Json -Depth 8|Set-Content "$r/copy-public.json"
 exit 0
} catch {
 @{start_utc=$start;end_utc=[DateTime]::UtcNow.ToString('o');error=$_.Exception.Message;exit_code=1}|ConvertTo-Json|Set-Content "$r/copy-error-public.json"
 exit 1
}
