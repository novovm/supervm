#Requires -Version 7.0
#Requires -RunAsAdministrator
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$root='D:\WEB3_AI\SUPERVM\target\c-ttd-node-20261113\run-private-02\crash-39992-private'
if(Test-Path -LiteralPath $root){throw 'Evidence exists; do not overwrite'}
New-Item -ItemType Directory -Path $root | Out-Null
$result=[ordered]@{utc=[DateTime]::UtcNow.ToString('o');node_pid=39992;files=@();root_cause_fixed=$false;relay_ready=$false}
try {
 $sources=@('C:\Windows\ServiceProfiles\LocalService\AppData\Local\CrashDumps\novovm-node.exe.39992.dmp','C:\ProgramData\Microsoft\Windows\WER\ReportArchive\AppCrash_novovm-node.exe_e880c3d9fb9ec8d27cc0671e822f95191c741b8_55f1ec8c_dda8bfb2-5130-4cd7-815a-fe39253c7509\Report.wer')
 foreach($src in $sources){
  if(!(Test-Path -LiteralPath $src -PathType Leaf)){$result.files+=@{name=[IO.Path]::GetFileName($src);present=$false};continue}
  $info=Get-Item -LiteralPath $src
  if($info.LastWriteTimeUtc -lt [datetime]'2026-11-13T12:47:00Z' -or $info.LastWriteTimeUtc -gt [datetime]'2026-11-13T12:51:00Z'){throw 'Unexpected source time; preserve'}
  $hash=(Get-FileHash -LiteralPath $src -Algorithm SHA256).Hash.ToLowerInvariant()
  $dst=Join-Path $root $info.Name
  Copy-Item -LiteralPath $src -Destination $dst
  if((Get-FileHash -LiteralPath $dst).Hash.ToLowerInvariant() -cne $hash){throw 'Copy hash differs'}
  $result.files+=@{name=$info.Name;present=$true;bytes=$info.Length;sha256=$hash}
 }
 $result.exit_code=0
 $result|ConvertTo-Json -Depth 6|Set-Content -LiteralPath (Join-Path $root 'copy-public.json')
 exit 0
} catch {
 $result.exit_code=1
 $result.error=$_.Exception.Message
 $result|ConvertTo-Json -Depth 6|Set-Content -LiteralPath (Join-Path $root 'copy-public.json')
 exit 1
}
