param(
  [ValidateSet('A','C','D')][string]$HostId,
  [string]$RuntimeRoot = '',
  [string]$OutFile = ''
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Hash-File([string]$Path) {
  if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return $null }
  return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

$started = (Get-Date).ToUniversalTime().ToString('o')
$nodes = @(Get-CimInstance Win32_Process -Filter "Name = 'novovm-node.exe'")
$processes = @()
foreach ($node in $nodes) {
  $loaded = @()
  $enumerationError = $null
  try {
    $modules = @(Get-Process -Id $node.ProcessId -Module -ErrorAction Stop)
    foreach ($module in $modules) {
      if ($module.ModuleName -ieq 'aoem_ffi.dll') {
        $loaded += [ordered]@{
          path = $module.FileName
          sha256 = Hash-File $module.FileName
        }
      }
    }
  } catch {
    $enumerationError = $_.Exception.Message
  }
  $processes += [ordered]@{
    pid = [int]$node.ProcessId
    executable = $node.ExecutablePath
    executable_sha256 = Hash-File $node.ExecutablePath
    loaded_aoem = $loaded
    module_enumeration_error = $enumerationError
  }
}

$installed = $null
if ($RuntimeRoot) {
  $root = (Resolve-Path -LiteralPath $RuntimeRoot).Path
  $dll = Join-Path $root 'windows\core\bin\aoem_ffi.dll'
  $manifest = Join-Path $root 'manifest\aoem-manifest.json'
  $identity = $null
  $declaredHash = $null
  if (Test-Path -LiteralPath $manifest -PathType Leaf) {
    $json = Get-Content -LiteralPath $manifest -Raw | ConvertFrom-Json
    $entry = @($json.entries | Where-Object { $_.platform -eq 'windows-x86_64' -and $_.name -eq 'core' }) | Select-Object -First 1
    if ($entry) {
      $identity = $entry.core_source_identity
      $declaredHash = $entry.sha256
    }
  }
  $installed = [ordered]@{
    root = $root
    core_path = $dll
    core_sha256 = Hash-File $dll
    manifest_path = $manifest
    manifest_sha256 = Hash-File $manifest
    manifest_declared_core_sha256 = $declaredHash
    core_source_identity = $identity
  }
}

$report = [ordered]@{
  schema = 'novonet_aoem_host_version_v1'
  host_id = $HostId
  started_utc = $started
  ended_utc = (Get-Date).ToUniversalTime().ToString('o')
  processes = $processes
  installed_runtime = $installed
  interpretation = if ($processes.Count -eq 0) { 'node_not_running; installed artifact is not loaded runtime' } else { 'compare loaded module and installed artifact separately' }
}
$result = $report | ConvertTo-Json -Depth 12
if ($OutFile) {
  $parent = Split-Path -Parent $OutFile
  if (-not (Test-Path -LiteralPath $parent)) { throw "EvidenceRoot must already exist: $parent" }
  if (Test-Path -LiteralPath $OutFile) { throw "refuse overwrite: $OutFile" }
  [IO.File]::WriteAllText($OutFile, $result + [Environment]::NewLine, [Text.UTF8Encoding]::new($false))
}
$result
