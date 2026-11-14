$ErrorActionPreference = 'Stop'
$result = [ordered]@{ start_utc = [DateTime]::UtcNow.ToString('o'); read_only = $true }
$names = @('DumpType','DumpCount','DumpFolder','Auto','Debugger','GlobalFlag','VerifierFlags')
$keys = @(
 'HKLM:\SOFTWARE\Microsoft\Windows\Windows Error Reporting\LocalDumps',
 'HKLM:\SOFTWARE\Microsoft\Windows\Windows Error Reporting\LocalDumps\novovm-node.exe',
 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\AeDebug',
 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options\novovm-node.exe'
)
$result.registry = @(foreach ($key in $keys) {
 try {
  if (Test-Path -LiteralPath $key) {
   $item = Get-Item -LiteralPath $key
   $values = [ordered]@{}
   foreach ($name in $names) { if ($item.GetValueNames() -contains $name) { $values[$name] = $item.GetValue($name) } }
   [ordered]@{ key=$key; exists=$true; values=$values; query_succeeded=$true }
  } else { [ordered]@{key=$key; exists=$false; query_succeeded=$true} }
 } catch { [ordered]@{key=$key;query_succeeded=$false;error=$_.Exception.Message} }
})
$result.dump_directories = @(foreach ($dir in @(
 'C:\Windows\ServiceProfiles\LocalService\AppData\Local\CrashDumps',
 'C:\Users\leadb\AppData\Local\CrashDumps'
)) {
 try {
  $files = @(Get-ChildItem -LiteralPath $dir -Filter 'novovm-node*.dmp' -File -ErrorAction Stop | ForEach-Object {
   [ordered]@{name=$_.Name;bytes=$_.Length;modified_utc=$_.LastWriteTimeUtc.ToString('o')}
  })
  [ordered]@{directory=$dir;listing_succeeded=$true;files=$files}
 } catch { [ordered]@{directory=$dir;listing_succeeded=$false;error=$_.Exception.Message} }
})
$result.end_utc = [DateTime]::UtcNow.ToString('o')
$result.original_node_started = $false
$result.registry_changed = $false
$result.original_database_accessed = $false
$result.all_queries_succeeded = (@($result.registry | Where-Object { !$_.query_succeeded }).Count -eq 0 -and @($result.dump_directories | Where-Object { !$_.listing_succeeded }).Count -eq 0)
[IO.File]::WriteAllText((Join-Path $PSScriptRoot 'dump-config-public.json'), ($result | ConvertTo-Json -Depth 8) + "`n", [Text.UTF8Encoding]::new($false))
if (!$result.all_queries_succeeded) { exit 2 }
exit 0
