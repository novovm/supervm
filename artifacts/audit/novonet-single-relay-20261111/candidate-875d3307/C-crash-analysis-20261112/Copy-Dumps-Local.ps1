#Requires -Version 7.0
#Requires -RunAsAdministrator
$ErrorActionPreference='Stop'
$r=$PSScriptRoot
if(Test-Path -LiteralPath "$r/dumps-local"){throw 'Already copied'}
New-Item -ItemType Directory -Path "$r/dumps-local"|Out-Null
$rows=@()
foreach($id in @(31296,3228)){
 $src="C:\Windows\ServiceProfiles\LocalService\AppData\Local\CrashDumps\novovm-node.exe.$id.dmp"
 $dst=Join-Path "$r/dumps-local" ([IO.Path]::GetFileName($src))
 $before=(Get-FileHash -LiteralPath $src).Hash.ToLowerInvariant()
 Copy-Item -LiteralPath $src -Destination $dst
 $after=(Get-FileHash -LiteralPath $dst).Hash.ToLowerInvariant()
 if($before -cne $after){throw 'Dump copy mismatch'}
 $rows+=@{file=[IO.Path]::GetFileName($src);sha256=$after;bytes=(Get-Item -LiteralPath $src).Length;modified_utc=(Get-Item -LiteralPath $src).LastWriteTimeUtc.ToString('o');copy_verified=$true;uploaded=$false}
}
$rows|ConvertTo-Json|Set-Content -LiteralPath "$r/dump-copy-public.json" -Encoding utf8
