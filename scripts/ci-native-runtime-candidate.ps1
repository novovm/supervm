param(
  [Parameter(Mandatory)][ValidateSet('windows-x86_64', 'linux-x86_64')][string]$Platform,
  [string]$CandidateRoot = (Join-Path $PSScriptRoot '../artifacts/runtime-candidates/native-transport-mldsa65-20261103'),
  [switch]$GitHubEnvironment
)

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path -LiteralPath $CandidateRoot).Path
$manifest = Get-Content -LiteralPath (Join-Path $root 'manifest.json') -Raw | ConvertFrom-Json -AsHashtable
if ($manifest.schema_version -ne 1 -or
    $manifest.scope -cne 'isolated_native_transport_mldsa65_core_candidates_not_shipped_sdk') {
  throw 'Unsupported isolated native-runtime candidate manifest.'
}
$entry = $manifest.platforms[$Platform]
$libraryName = if ($Platform -eq 'windows-x86_64') { 'windows/aoem_ffi.dll' } else { 'linux/libaoem_ffi.so' }
$headerName = if ($Platform -eq 'windows-x86_64') { 'windows/aoem.h' } else { 'linux/aoem.h' }
if (!$entry -or $entry.library -cne $libraryName -or $entry.header -cne $headerName -or
    $entry.sha256 -cnotmatch '^[0-9a-f]{64}$' -or $entry.header_sha256 -cnotmatch '^[0-9a-f]{64}$' -or
    $entry.source.base_commit -cnotmatch '^[0-9a-f]{40}$' -or
    $entry.source.overlay_sha256 -cnotmatch '^[0-9a-f]{64}$' -or
    $entry.source.lock_sha256 -cnotmatch '^[0-9a-f]{64}$' -or
    !$entry.features -or $entry.features -notcontains 'mldsa') {
  throw 'Incomplete or mismatched platform candidate identity.'
}
$library = (Resolve-Path -LiteralPath (Join-Path $root $libraryName)).Path
$header = (Resolve-Path -LiteralPath (Join-Path $root $headerName)).Path
$runtimeEntries = @($manifest.entries | Where-Object { $_.name -ceq 'core' -and $_.platform -ceq $Platform })
if ($runtimeEntries.Count -ne 1 -or $runtimeEntries[0].dll -cne $libraryName -or
    $runtimeEntries[0].sha256 -cne $entry.sha256 -or $runtimeEntries[0].abi_expected -ne 1) {
  throw 'Runtime manifest does not describe the same selected candidate core.'
}
# Keep the existing platform SDK's required ABI/capability checks. The runtime
# loader still checks these against the real library; this is not qualification.
$requiredCapabilities = @('execute_ops_v2', 'semantic_graph_v3', 'storage_provider_wire_v1')
if ($Platform -eq 'windows-x86_64') {
  $requiredCapabilities += @('compute_checked_integer_resources_v7', 'compute_checked_integer_binary_result_v1',
    'compute_checked_integer_dataflow_v1', 'compute_checked_integer_composition_v1')
}
foreach ($capability in $requiredCapabilities) {
  if ($runtimeEntries[0].capabilities_required[$capability] -isnot [bool] -or
      !$runtimeEntries[0].capabilities_required[$capability]) {
    throw "Runtime manifest is missing the original required capability: $capability"
  }
}
foreach ($artifact in @(@($library, $entry.sha256), @($header, $entry.header_sha256))) {
  $actual = (Get-FileHash -LiteralPath $artifact[0] -Algorithm SHA256).Hash.ToLowerInvariant()
  if ($actual -cne $artifact[1]) { throw "Isolated runtime artifact SHA-256 mismatch: $($artifact[0])" }
}
# Hash/materialization is not runtime qualification. The original CLI must run
# the actual verification-owner KAT against this same selected library.
if ($GitHubEnvironment) {
  if (!$env:GITHUB_ENV) { throw 'GITHUB_ENV is required for CI environment output.' }
  @(
    "NOVOVM_AOEM_TEST_LIBRARY=$library"
    "NOVOVM_AOEM_TEST_MANIFEST=$(Join-Path $root 'manifest.json')"
    "NOVOVM_AOEM_MANIFEST=$(Join-Path $root 'manifest.json')"
    "NOVOVM_AOEM_DLL=$library"
    "NOVOVM_NATIVE_RUNTIME_SHA256=$($entry.sha256)"
  ) | Out-File -LiteralPath $env:GITHUB_ENV -Append -Encoding utf8
}
[PSCustomObject]@{
  platform = $Platform
  library = $library
  sha256 = $entry.sha256
  header = $header
  header_sha256 = $entry.header_sha256
  runtime_manifest = (Join-Path $root 'manifest.json')
  runtime_kat_executed = $false
} | ConvertTo-Json
