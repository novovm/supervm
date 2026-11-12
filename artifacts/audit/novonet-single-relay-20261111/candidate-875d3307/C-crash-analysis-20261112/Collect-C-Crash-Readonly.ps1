#Requires -Version 7.0
#Requires -RunAsAdministrator
$ErrorActionPreference='Stop'
$root=$PSScriptRoot
if(Test-Path -LiteralPath "$root/inventory-local.json"){throw 'Fresh evidence required'}
$data=[ordered]@{host='C';started_utc=[DateTime]::UtcNow.ToString('o');read_only=$true;node_restarted=$false;configuration_changed=$false;D_touched=$false;locations=@();reports=@();dumps=@();registry=@();errors=@()}
$locations=@('C:\ProgramData\Microsoft\Windows\WER\ReportArchive','C:\ProgramData\Microsoft\Windows\WER\ReportQueue','C:\ProgramData\Microsoft\Windows\WER\Temp','C:\Windows\ServiceProfiles\LocalService\AppData\Local\CrashDumps','C:\Users\leadb\AppData\Local\CrashDumps')
foreach($base in $locations){
 try{
  $exists=Test-Path -LiteralPath $base -ErrorAction Stop
  $data.locations+=@{path=$base;exists=$exists}
  if(!$exists){continue}
  if($base -match '\\CrashDumps$'){
   foreach($f in @(Get-ChildItem -LiteralPath $base -File -Filter '*novovm*' -ErrorAction Stop)){$data.dumps+=@{path=$f.FullName;bytes=$f.Length;modified_utc=$f.LastWriteTimeUtc.ToString('o')}}
  }else{
   $folders=@(Get-ChildItem -LiteralPath $base -Directory -ErrorAction Stop|Where-Object Name -like '*novovm*'|Sort-Object LastWriteTimeUtc -Descending|Select-Object -First 30)
   foreach($folder in $folders){
    if($folder.Attributes -band [IO.FileAttributes]::ReparsePoint){continue}
    $items=@(Get-ChildItem -LiteralPath $folder.FullName -File -Recurse -Depth 2 -ErrorAction Stop|Select-Object -First 100)
    foreach($f in $items){
     if($f.Extension -in @('.dmp','.mdmp','.hdmp')){$data.dumps+=@{path=$f.FullName;bytes=$f.Length;modified_utc=$f.LastWriteTimeUtc.ToString('o')}}
     if($f.Name -eq 'Report.wer'){
      $dest=Join-Path $root ('wer-local-'+$data.reports.Count+'.txt')
      Copy-Item -LiteralPath $f.FullName -Destination $dest
      $data.reports+=@{path=$f.FullName;local_copy=$dest;bytes=$f.Length;modified_utc=$f.LastWriteTimeUtc.ToString('o');sha256=(Get-FileHash -LiteralPath $f.FullName).Hash.ToLowerInvariant()}
     }
    }
   }
  }
 }catch{$data.errors+=@{location=$base;error=$_.Exception.Message}}
}
foreach($key in @('HKLM:\SOFTWARE\Microsoft\Windows\Windows Error Reporting\LocalDumps','HKLM:\SOFTWARE\Microsoft\Windows\Windows Error Reporting\LocalDumps\novovm-node.exe')){
 try{if(Test-Path -LiteralPath $key){$v=Get-ItemProperty -LiteralPath $key;$data.registry+=@{key=$key;DumpFolder=$v.DumpFolder;DumpType=$v.DumpType;DumpCount=$v.DumpCount}}else{$data.registry+=@{key=$key;exists=$false}}}catch{$data.errors+=@{location=$key;error=$_.Exception.Message}}
}
$data.finished_utc=[DateTime]::UtcNow.ToString('o')
$data|ConvertTo-Json -Depth 8|Set-Content -LiteralPath "$root/inventory-local.json" -Encoding utf8
[ordered]@{host='C';started_utc=$data.started_utc;finished_utc=$data.finished_utc;read_only=$true;node_restarted=$false;configuration_changed=$false;D_touched=$false;report_count=$data.reports.Count;dump_count=$data.dumps.Count;errors_count=$data.errors.Count;raw_reports_local_only=$true;dumps_not_uploaded=$true}|ConvertTo-Json|Set-Content -LiteralPath "$root/collection-public.json" -Encoding utf8
exit 0
