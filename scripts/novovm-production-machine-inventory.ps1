[CmdletBinding()]
param(
  [ValidatePattern('^[A-Za-z0-9._-]+$')]
  [string]$MachineLabel = [System.Net.Dns]::GetHostName(),
  [string]$ReportPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$windowsHost = [Environment]::OSVersion.Platform -eq [PlatformID]::Win32NT
$head = (& git -C $repo rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0) { throw 'Unable to read repository HEAD.' }
$dirty = @(& git -C $repo status --porcelain)
if ($LASTEXITCODE -ne 0) { throw 'Unable to read repository status.' }

$addresses = @()
$memoryBytes = $null
$systemName = [Environment]::OSVersion.VersionString
if ($windowsHost) {
  $operatingSystem = Get-CimInstance Win32_OperatingSystem
  $systemName = $operatingSystem.Caption + ' ' + $operatingSystem.Version
  $memoryBytes = [uint64](Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory
  $addresses = @(Get-NetIPAddress -AddressFamily IPv4 | ForEach-Object {
    [ordered]@{
      interface = $_.InterfaceAlias
      ip = $_.IPAddress
      prefix_length = $_.PrefixLength
      state = [string]$_.AddressState
    }
  })
} elseif (Get-Command ip -ErrorAction SilentlyContinue) {
  $interfaces = & ip -j -4 addr
  if ($LASTEXITCODE -ne 0) { throw 'Unable to read local interface inventory.' }
  $addresses = @(($interfaces | ConvertFrom-Json) | ForEach-Object {
    $interface = $_
    foreach ($address in $interface.addr_info) {
      [ordered]@{
        interface = $interface.ifname
        ip = $address.local
        prefix_length = $address.prefixlen
        state = $interface.operstate
      }
    }
  })
}

$runtimeRelative = if ($windowsHost) { 'aoem/windows/core/bin/aoem_ffi.dll' } else { 'aoem/linux/core/bin/libaoem_ffi.so' }
$runtimePath = Join-Path $repo $runtimeRelative
$runtime = [ordered]@{ path = $runtimeRelative; present = (Test-Path -LiteralPath $runtimePath); bytes = $null; sha256 = $null; matches_sdk_manifest = $false }
if ($runtime.present) {
  $runtime.bytes = (Get-Item -LiteralPath $runtimePath).Length
  $runtime.sha256 = (Get-FileHash -LiteralPath $runtimePath -Algorithm SHA256).Hash.ToLowerInvariant()
  $manifest = Get-Content -Raw -LiteralPath (Join-Path $repo 'aoem/aoem-sdk-manifest.json') | ConvertFrom-Json
  $platformKey = if ($windowsHost) { 'windows-x86_64' } else { 'linux-x86_64' }
  $runtime.matches_sdk_manifest = $runtime.sha256 -eq $manifest.platforms.$platformKey.library_sha256
}
$disks = @(Get-PSDrive -PSProvider FileSystem | ForEach-Object {
  [ordered]@{ root = $_.Root; used_bytes = $_.Used; free_bytes = $_.Free }
})
$report = [ordered]@{
  schema = 'novovm-production-machine-inventory/v1'
  generated_at_utc = [DateTime]::UtcNow.ToString('o')
  machine_label = $MachineLabel
  hostname = [System.Net.Dns]::GetHostName()
  os = $systemName
  process_64_bit = [Environment]::Is64BitProcess
  logical_processors = [Environment]::ProcessorCount
  memory_bytes = $memoryBytes
  git_commit = $head
  worktree_clean = $dirty.Count -eq 0
  ipv4_candidates = $addresses
  disks = $disks
  aoem_runtime = $runtime
  connectivity_tested = $false
  production_ready = $false
}
if ([string]::IsNullOrWhiteSpace($ReportPath)) {
  $stamp = [DateTime]::UtcNow.ToString('yyyyMMdd-HHmmss-fff')
  $ReportPath = Join-Path $repo "artifacts/production-inventory/$MachineLabel-$stamp.json"
} elseif (![IO.Path]::IsPathRooted($ReportPath)) {
  $ReportPath = Join-Path $repo $ReportPath
}
if (Test-Path -LiteralPath $ReportPath) { throw 'Inventory report already exists; choose a new path.' }
New-Item -ItemType Directory -Force -Path (Split-Path -Parent $ReportPath) | Out-Null
$report | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $ReportPath -Encoding UTF8
Write-Output "Inventory saved: $ReportPath"
Write-Output "HEAD=$head AOEM manifest match=$($runtime.matches_sdk_manifest)"
