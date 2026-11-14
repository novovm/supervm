param([Parameter(Mandatory)][string]$EvidenceRoot)
$ErrorActionPreference = 'Stop'
if (Test-Path -LiteralPath $EvidenceRoot) { throw 'Use a new EvidenceRoot; no overwrite permitted.' }
$admin = [Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())
if (!$admin.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { throw 'Administrator read access required; ACLs will not be changed.' }
[IO.Directory]::CreateDirectory($EvidenceRoot) | Out-Null
$private = Join-Path $EvidenceRoot 'files-private'
[IO.Directory]::CreateDirectory($private) | Out-Null
$utf8 = [Text.UTF8Encoding]::new($false)
function Save-Json($Name, $Object) {
 [IO.File]::WriteAllText((Join-Path $EvidenceRoot $Name), ($Object | ConvertTo-Json -Depth 14) + "`n", $utf8)
}
function Read-DumpHeader([string]$Path) {
 $stream = [IO.File]::Open($Path,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::ReadWrite)
 try {
  $b = [IO.BinaryReader]::new($stream)
  if ($b.ReadUInt32() -ne 0x504d444d) { throw 'Not MDMP signature' }
  $version = $b.ReadUInt32(); $count = $b.ReadUInt32(); $directory = $b.ReadUInt32()
  $checksum = $b.ReadUInt32(); $stamp = $b.ReadUInt32(); $flags = $b.ReadUInt64()
  if ($count -gt 1024 -or ([uint64]$directory + 12*[uint64]$count) -gt $stream.Length) { throw 'Invalid directory bounds' }
  $streams = @()
  for ($i=0; $i -lt $count; $i++) {
   $stream.Position = $directory+12*$i
   $kind=$b.ReadUInt32(); $size=$b.ReadUInt32(); $rva=$b.ReadUInt32()
   if (([uint64]$rva+$size) -gt $stream.Length) { throw 'Truncated stream' }
   $streams += [ordered]@{type=$kind;bytes=$size;rva=$rva}
  }
  $threadCount=$null; $exception=$null; $memory64Count=0; [uint64]$memory64Bytes=0
  foreach ($s in $streams) {
   $stream.Position=$s.rva
   if ($s.type -eq 3) { $threadCount=$b.ReadUInt32() }
   if ($s.type -eq 6) {
    $tid=$b.ReadUInt32(); $pad=$b.ReadUInt32(); $code=$b.ReadUInt32()
    $exception=[ordered]@{thread_id=$tid;code=('0x{0:x8}' -f $code)}
   }
   if ($s.type -eq 9) {
    $memory64Count=$b.ReadUInt64(); $base=$b.ReadUInt64()
    if ($memory64Count -gt 1000000 -or (16+16*$memory64Count) -gt $s.bytes) { throw 'Invalid Memory64 list bounds' }
    for ($j=0; $j -lt $memory64Count; $j++) { $address=$b.ReadUInt64(); $memory64Bytes += $b.ReadUInt64() }
    if (($base+$memory64Bytes) -gt $stream.Length) { throw 'Truncated Memory64 payload' }
   }
  }
  return [ordered]@{flags=('0x{0:x}' -f $flags);full_memory_flag=(($flags -band 2) -ne 0);timestamp_utc=[DateTimeOffset]::FromUnixTimeSeconds($stamp).UtcDateTime.ToString('o');thread_count=$threadCount;exception=$exception;memory64_count=$memory64Count;memory64_bytes=$memory64Bytes;stream_types=@($streams | ForEach-Object {$_.type})}
 } finally { $stream.Dispose() }
}
$result=[ordered]@{start_utc=[DateTime]::UtcNow.ToString('o');admin=$true;localservice_sid='S-1-5-19';read_only_sources=$true;acl_changed=$false;node_started=$false;original_database_accessed=$false;roots=@();files=@();errors=@();exit_code=$null}
Save-Json 'started.json' $result
try {
 $roots=@(
  @{label='LocalService-CrashDumps';path='C:\Windows\ServiceProfiles\LocalService\AppData\Local\CrashDumps';direct=$true},
  @{label='LocalService-WER-Archive';path='C:\Windows\ServiceProfiles\LocalService\AppData\Local\Microsoft\Windows\WER\ReportArchive';direct=$false},
  @{label='LocalService-WER-Queue';path='C:\Windows\ServiceProfiles\LocalService\AppData\Local\Microsoft\Windows\WER\ReportQueue';direct=$false},
  @{label='System-WER-Archive';path='C:\ProgramData\Microsoft\Windows\WER\ReportArchive';direct=$false},
  @{label='System-WER-Queue';path='C:\ProgramData\Microsoft\Windows\WER\ReportQueue';direct=$false}
 )
 $selected=@()
 foreach ($root in $roots) {
  $entry=[ordered]@{label=$root.label;path=$root.path;exists=$false;acl=$null;completed=$false;error=$null}
  try {
   if (Test-Path -LiteralPath $root.path) {
    $entry.exists=$true
    $acl=Get-Acl -LiteralPath $root.path
    $entry.acl=[ordered]@{owner=$acl.Owner;sddl=$acl.Sddl}
    if ($root.direct) {
     $files=@(Get-ChildItem -LiteralPath $root.path -File | Where-Object {$_.Name -like 'novovm-node*' -and !($_.Attributes -band [IO.FileAttributes]::ReparsePoint)})
    } else {
     $files=@(Get-ChildItem -LiteralPath $root.path -Directory -Filter '*novovm-node*' | Where-Object {!($_.Attributes -band [IO.FileAttributes]::ReparsePoint)} | ForEach-Object {
      # WER report artifacts are immediate children; never follow links or unrelated trees.
      Get-ChildItem -LiteralPath $_.FullName -File | Where-Object {!($_.Attributes -band [IO.FileAttributes]::ReparsePoint)}
     })
    }
    foreach ($file in $files) { $selected += @{file=$file;root=$root} }
   }
   $entry.completed=$true
  } catch { $entry.error=$_.Exception.Message; $result.errors += $entry.error }
  $result.roots += $entry
 }
 $total=($selected | ForEach-Object {$_.file.Length} | Measure-Object -Sum).Sum
 $drive=[IO.DriveInfo]::new([IO.Path]::GetPathRoot([IO.Path]::GetFullPath($EvidenceRoot)))
 if ($drive.AvailableFreeSpace -lt ($total+2GB)) { throw 'Insufficient space for private copies plus 2 GiB margin; sources untouched.' }
 $index=0
 foreach ($item in $selected) {
  $file=$item.file; $index++
  $before=Get-Item -LiteralPath $file.FullName
  $row=[ordered]@{root=$item.root.label;relative_path=[IO.Path]::GetRelativePath($item.root.path,$file.FullName);bytes=$before.Length;creation_utc=$before.CreationTimeUtc.ToString('o');last_write_utc=$before.LastWriteTimeUtc.ToString('o');sha256=$null;private_copy=$null;source_stable=$false;dump=$null;error=$null}
  try {
   $row.sha256=(Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
   $copyName=('{0:d4}-' -f $index)+$file.Name
   $copy=Join-Path $private $copyName
   [IO.File]::Copy($file.FullName,$copy,$false)
   if ((Get-FileHash -LiteralPath $copy -Algorithm SHA256).Hash.ToLowerInvariant() -ne $row.sha256) { throw 'Private copy hash mismatch' }
   $after=Get-Item -LiteralPath $file.FullName
   $row.source_stable=($before.Length -eq $after.Length -and $before.LastWriteTimeUtc -eq $after.LastWriteTimeUtc -and $before.CreationTimeUtc -eq $after.CreationTimeUtc)
   if (!$row.source_stable) { throw 'Source changed during read; do not use as stable evidence' }
   $row.private_copy='files-private/'+$copyName
   if ($file.Extension -ieq '.dmp') { $row.dump=Read-DumpHeader $copy }
  } catch { $row.error=$_.Exception.Message; $result.errors += $row.error }
  $result.files += $row
  Save-Json 'inventory-public.json' $result
 }
 $result.exit_code=if ($result.errors.Count) {2} else {0}
} catch { $result.errors += $_.Exception.Message; $result.exit_code=3 }
finally {
 $result.end_utc=[DateTime]::UtcNow.ToString('o')
 Save-Json 'inventory-public.json' $result
 Save-Json 'exit.json' ([ordered]@{exit_code=$result.exit_code;end_utc=$result.end_utc})
}
exit $result.exit_code
