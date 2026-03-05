param(
    [string]$RepoRoot = "",
    [string]$OutputPath = "",
    [string]$FunctionalJson = "",
    [string]$PerformanceJson = "",
    [string]$CapabilityJson = "",
    [string]$BaselineJson = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not $RepoRoot) {
    $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
} else {
    $RepoRoot = (Resolve-Path $RepoRoot).Path
}
if (-not $OutputPath) {
    $OutputPath = Join-Path $RepoRoot "docs_CN\SVM2026-MIGRATION\NOVOVM-CAPABILITY-MIGRATION-LEDGER-AUTO-2026-03-03.md"
}
if (-not $FunctionalJson) {
    $FunctionalJson = Join-Path $RepoRoot "artifacts\migration\functional\functional-consistency.json"
}
if (-not $PerformanceJson) {
    $PerformanceJson = Join-Path $RepoRoot "artifacts\migration\performance\performance-compare.json"
}
if (-not $CapabilityJson) {
    $CapabilityJson = Join-Path $RepoRoot "artifacts\migration\capabilities\capability-contract-core.json"
}
if (-not $BaselineJson) {
    $BaselineJson = Join-Path $RepoRoot "artifacts\migration\baseline\svm2026-baseline-core.json"
}

function Read-JsonOrNull {
    param([string]$Path)
    if (-not (Test-Path $Path)) {
        return $null
    }
    try {
        return (Get-Content -Path $Path -Raw | ConvertFrom-Json)
    } catch {
        return $null
    }
}

$functional = Read-JsonOrNull -Path $FunctionalJson
$performance = Read-JsonOrNull -Path $PerformanceJson
$capability = Read-JsonOrNull -Path $CapabilityJson
$baseline = Read-JsonOrNull -Path $BaselineJson

$generatedAt = [DateTime]::UtcNow.ToString("o")
$today = (Get-Date).ToString("yyyy-MM-dd")

$functionalPass = if ($functional) { [bool]$functional.overall_pass } else { $false }
$performancePass = if ($performance -and $null -ne $performance.compare_pass) { [bool]$performance.compare_pass } else { $null }
$stateRootAvailable = if ($functional -and $functional.state_root_consistency) { [bool]$functional.state_root_consistency.available } else { $false }
$stateRootPass = if ($functional -and $functional.state_root_consistency) { [bool]$functional.state_root_consistency.pass } else { $false }
$txCodecAvailable = if ($functional -and $functional.tx_codec_signal) { [bool]$functional.tx_codec_signal.available } else { $false }
$txCodecPass = if ($functional -and $functional.tx_codec_signal) { [bool]$functional.tx_codec_signal.pass } else { $false }
$txCodecBytes = if ($functional -and $functional.tx_codec_signal -and $functional.tx_codec_signal.ffi_v2 -and $null -ne $functional.tx_codec_signal.ffi_v2.bytes) { [int]$functional.tx_codec_signal.ffi_v2.bytes } else { 0 }
$mempoolAvailable = if ($functional -and $functional.mempool_admission_signal) { [bool]$functional.mempool_admission_signal.available } else { $false }
$mempoolPass = if ($functional -and $functional.mempool_admission_signal) { [bool]$functional.mempool_admission_signal.pass } else { $false }
$mempoolAccepted = if ($functional -and $functional.mempool_admission_signal -and $functional.mempool_admission_signal.ffi_v2 -and $null -ne $functional.mempool_admission_signal.ffi_v2.accepted) { [int]$functional.mempool_admission_signal.ffi_v2.accepted } else { 0 }
$mempoolRejected = if ($functional -and $functional.mempool_admission_signal -and $functional.mempool_admission_signal.ffi_v2 -and $null -ne $functional.mempool_admission_signal.ffi_v2.rejected) { [int]$functional.mempool_admission_signal.ffi_v2.rejected } else { 0 }
$mempoolFeeFloor = if ($functional -and $functional.mempool_admission_signal -and $functional.mempool_admission_signal.ffi_v2 -and $null -ne $functional.mempool_admission_signal.ffi_v2.fee_floor) { [int64]$functional.mempool_admission_signal.ffi_v2.fee_floor } else { 0 }
$txMetaAvailable = if ($functional -and $functional.tx_metadata_signal) { [bool]$functional.tx_metadata_signal.available } else { $false }
$txMetaPass = if ($functional -and $functional.tx_metadata_signal) { [bool]$functional.tx_metadata_signal.pass } else { $false }
$txMetaAccounts = if ($functional -and $functional.tx_metadata_signal -and $functional.tx_metadata_signal.ffi_v2 -and $null -ne $functional.tx_metadata_signal.ffi_v2.accounts) { [int]$functional.tx_metadata_signal.ffi_v2.accounts } else { 0 }
$txMetaMinFee = if ($functional -and $functional.tx_metadata_signal -and $functional.tx_metadata_signal.ffi_v2 -and $null -ne $functional.tx_metadata_signal.ffi_v2.min_fee) { [int64]$functional.tx_metadata_signal.ffi_v2.min_fee } else { 0 }
$txMetaMaxFee = if ($functional -and $functional.tx_metadata_signal -and $functional.tx_metadata_signal.ffi_v2 -and $null -ne $functional.tx_metadata_signal.ffi_v2.max_fee) { [int64]$functional.tx_metadata_signal.ffi_v2.max_fee } else { 0 }
$adapterAvailable = if ($functional -and $functional.adapter_signal) { [bool]$functional.adapter_signal.available } else { $false }
$adapterPass = if ($functional -and $functional.adapter_signal) { [bool]$functional.adapter_signal.pass } else { $false }
$adapterBackend = if ($functional -and $functional.adapter_signal -and $functional.adapter_signal.ffi_v2 -and $null -ne $functional.adapter_signal.ffi_v2.backend) { [string]$functional.adapter_signal.ffi_v2.backend } else { "" }
$adapterChain = if ($functional -and $functional.adapter_signal -and $functional.adapter_signal.ffi_v2 -and $null -ne $functional.adapter_signal.ffi_v2.chain) { [string]$functional.adapter_signal.ffi_v2.chain } else { "" }
$adapterTxs = if ($functional -and $functional.adapter_signal -and $functional.adapter_signal.ffi_v2 -and $null -ne $functional.adapter_signal.ffi_v2.txs) { [int]$functional.adapter_signal.ffi_v2.txs } else { 0 }
$adapterAccounts = if ($functional -and $functional.adapter_signal -and $functional.adapter_signal.ffi_v2 -and $null -ne $functional.adapter_signal.ffi_v2.accounts) { [int]$functional.adapter_signal.ffi_v2.accounts } else { 0 }
$adapterPluginAbiAvailable = if ($functional -and $functional.adapter_plugin_abi_signal) { [bool]$functional.adapter_plugin_abi_signal.available } else { $false }
$adapterPluginAbiPass = if ($functional -and $functional.adapter_plugin_abi_signal) { [bool]$functional.adapter_plugin_abi_signal.pass } else { $false }
$adapterPluginAbiExpected = if ($functional -and $functional.adapter_plugin_abi_signal -and $null -ne $functional.adapter_plugin_abi_signal.expected_abi) { [int]$functional.adapter_plugin_abi_signal.expected_abi } else { 0 }
$adapterPluginAbiRequired = if ($functional -and $functional.adapter_plugin_abi_signal -and $null -ne $functional.adapter_plugin_abi_signal.required_caps) { [string]$functional.adapter_plugin_abi_signal.required_caps } else { "" }
$adapterPluginAbiEnabled = if ($functional -and $functional.adapter_plugin_abi_signal -and $functional.adapter_plugin_abi_signal.ffi_v2 -and $null -ne $functional.adapter_plugin_abi_signal.ffi_v2.enabled) { [bool]$functional.adapter_plugin_abi_signal.ffi_v2.enabled } else { $false }
$adapterPluginAbiCompatible = if ($functional -and $functional.adapter_plugin_abi_signal -and $functional.adapter_plugin_abi_signal.ffi_v2 -and $null -ne $functional.adapter_plugin_abi_signal.ffi_v2.compatible) { [bool]$functional.adapter_plugin_abi_signal.ffi_v2.compatible } else { $false }
$adapterPluginRegistryAvailable = if ($functional -and $functional.adapter_plugin_registry_signal) { [bool]$functional.adapter_plugin_registry_signal.available } else { $false }
$adapterPluginRegistryPass = if ($functional -and $functional.adapter_plugin_registry_signal) { [bool]$functional.adapter_plugin_registry_signal.pass } else { $false }
$adapterPluginRegistryEnabled = if ($functional -and $functional.adapter_plugin_registry_signal -and $functional.adapter_plugin_registry_signal.ffi_v2 -and $null -ne $functional.adapter_plugin_registry_signal.ffi_v2.enabled) { [bool]$functional.adapter_plugin_registry_signal.ffi_v2.enabled } else { $false }
$adapterPluginRegistryMatched = if ($functional -and $functional.adapter_plugin_registry_signal -and $functional.adapter_plugin_registry_signal.ffi_v2 -and $null -ne $functional.adapter_plugin_registry_signal.ffi_v2.matched) { [bool]$functional.adapter_plugin_registry_signal.ffi_v2.matched } else { $false }
$adapterPluginRegistryStrict = if ($functional -and $functional.adapter_plugin_registry_signal -and $functional.adapter_plugin_registry_signal.ffi_v2 -and $null -ne $functional.adapter_plugin_registry_signal.ffi_v2.strict) { [bool]$functional.adapter_plugin_registry_signal.ffi_v2.strict } else { $false }
$adapterPluginRegistryChainAllowed = if ($functional -and $functional.adapter_plugin_registry_signal -and $functional.adapter_plugin_registry_signal.ffi_v2 -and $null -ne $functional.adapter_plugin_registry_signal.ffi_v2.chain_allowed) { [bool]$functional.adapter_plugin_registry_signal.ffi_v2.chain_allowed } else { $false }
$adapterPluginRegistryHashCheck = if ($functional -and $functional.adapter_plugin_registry_signal -and $functional.adapter_plugin_registry_signal.ffi_v2 -and $null -ne $functional.adapter_plugin_registry_signal.ffi_v2.hash_check) { [bool]$functional.adapter_plugin_registry_signal.ffi_v2.hash_check } else { $false }
$adapterPluginRegistryHashMatch = if ($functional -and $functional.adapter_plugin_registry_signal -and $functional.adapter_plugin_registry_signal.ffi_v2 -and $null -ne $functional.adapter_plugin_registry_signal.ffi_v2.hash_match) { [bool]$functional.adapter_plugin_registry_signal.ffi_v2.hash_match } else { $false }
$adapterPluginRegistryAbiWhitelist = if ($functional -and $functional.adapter_plugin_registry_signal -and $functional.adapter_plugin_registry_signal.ffi_v2 -and $null -ne $functional.adapter_plugin_registry_signal.ffi_v2.abi_whitelist) { [bool]$functional.adapter_plugin_registry_signal.ffi_v2.abi_whitelist } else { $false }
$adapterPluginRegistryAbiAllowed = if ($functional -and $functional.adapter_plugin_registry_signal -and $functional.adapter_plugin_registry_signal.ffi_v2 -and $null -ne $functional.adapter_plugin_registry_signal.ffi_v2.abi_allowed) { [bool]$functional.adapter_plugin_registry_signal.ffi_v2.abi_allowed } else { $false }
$adapterPluginRegistryExpectedHashCheck = if ($functional -and $functional.adapter_plugin_registry_signal -and $null -ne $functional.adapter_plugin_registry_signal.expected_hash_check) { [bool]$functional.adapter_plugin_registry_signal.expected_hash_check } else { $false }
$adapterPluginRegistryExpectedSha256 = if ($functional -and $functional.adapter_plugin_registry_signal -and $null -ne $functional.adapter_plugin_registry_signal.expected_registry_sha256) { [string]$functional.adapter_plugin_registry_signal.expected_registry_sha256 } else { "" }
$adapterPluginRegistrySource = if ($functional -and $functional.adapter_plugin_registry_signal -and $null -ne $functional.adapter_plugin_registry_signal.source_path) { [string]$functional.adapter_plugin_registry_signal.source_path } else { "" }
$adapterConsensusAvailable = if ($functional -and $functional.adapter_consensus_binding_signal) { [bool]$functional.adapter_consensus_binding_signal.available } else { $false }
$adapterConsensusPass = if ($functional -and $functional.adapter_consensus_binding_signal) { [bool]$functional.adapter_consensus_binding_signal.pass } else { $false }
$adapterConsensusClass = if ($functional -and $functional.adapter_consensus_binding_signal -and $functional.adapter_consensus_binding_signal.ffi_v2 -and $null -ne $functional.adapter_consensus_binding_signal.ffi_v2.plugin_class) { [string]$functional.adapter_consensus_binding_signal.ffi_v2.plugin_class } else { "" }
$adapterConsensusClassCode = if ($functional -and $functional.adapter_consensus_binding_signal -and $functional.adapter_consensus_binding_signal.ffi_v2 -and $null -ne $functional.adapter_consensus_binding_signal.ffi_v2.plugin_class_code) { [int]$functional.adapter_consensus_binding_signal.ffi_v2.plugin_class_code } else { 0 }
$adapterConsensusHash = if ($functional -and $functional.adapter_consensus_binding_signal -and $functional.adapter_consensus_binding_signal.ffi_v2 -and $null -ne $functional.adapter_consensus_binding_signal.ffi_v2.consensus_adapter_hash) { [string]$functional.adapter_consensus_binding_signal.ffi_v2.consensus_adapter_hash } else { "" }
$adapterPluginAbiNegativeEnabled = if ($functional -and $functional.adapter_plugin_abi_negative_signal -and $null -ne $functional.adapter_plugin_abi_negative_signal.enabled) { [bool]$functional.adapter_plugin_abi_negative_signal.enabled } else { $false }
$adapterPluginAbiNegativeAvailable = if ($functional -and $functional.adapter_plugin_abi_negative_signal -and $null -ne $functional.adapter_plugin_abi_negative_signal.available) { [bool]$functional.adapter_plugin_abi_negative_signal.available } else { $false }
$adapterPluginAbiNegativePass = if ($functional -and $functional.adapter_plugin_abi_negative_signal -and $null -ne $functional.adapter_plugin_abi_negative_signal.pass) { [bool]$functional.adapter_plugin_abi_negative_signal.pass } else { $false }
$adapterPluginAbiNegativeReason = if ($functional -and $functional.adapter_plugin_abi_negative_signal -and $null -ne $functional.adapter_plugin_abi_negative_signal.reason) { [string]$functional.adapter_plugin_abi_negative_signal.reason } else { "" }
$adapterPluginAbiNegativeAbiFail = if ($functional -and $functional.adapter_plugin_abi_negative_signal -and $functional.adapter_plugin_abi_negative_signal.abi_mismatch -and $null -ne $functional.adapter_plugin_abi_negative_signal.abi_mismatch.failed_as_expected) { [bool]$functional.adapter_plugin_abi_negative_signal.abi_mismatch.failed_as_expected } else { $false }
$adapterPluginAbiNegativeAbiReason = if ($functional -and $functional.adapter_plugin_abi_negative_signal -and $functional.adapter_plugin_abi_negative_signal.abi_mismatch -and $null -ne $functional.adapter_plugin_abi_negative_signal.abi_mismatch.reason_match) { [bool]$functional.adapter_plugin_abi_negative_signal.abi_mismatch.reason_match } else { $false }
$adapterPluginAbiNegativeCapFail = if ($functional -and $functional.adapter_plugin_abi_negative_signal -and $functional.adapter_plugin_abi_negative_signal.capability_mismatch -and $null -ne $functional.adapter_plugin_abi_negative_signal.capability_mismatch.failed_as_expected) { [bool]$functional.adapter_plugin_abi_negative_signal.capability_mismatch.failed_as_expected } else { $false }
$adapterPluginAbiNegativeCapReason = if ($functional -and $functional.adapter_plugin_abi_negative_signal -and $functional.adapter_plugin_abi_negative_signal.capability_mismatch -and $null -ne $functional.adapter_plugin_abi_negative_signal.capability_mismatch.reason_match) { [bool]$functional.adapter_plugin_abi_negative_signal.capability_mismatch.reason_match } else { $false }
$adapterPluginSymbolNegativeEnabled = if ($functional -and $functional.adapter_plugin_symbol_negative_signal -and $null -ne $functional.adapter_plugin_symbol_negative_signal.enabled) { [bool]$functional.adapter_plugin_symbol_negative_signal.enabled } else { $false }
$adapterPluginSymbolNegativeAvailable = if ($functional -and $functional.adapter_plugin_symbol_negative_signal -and $null -ne $functional.adapter_plugin_symbol_negative_signal.available) { [bool]$functional.adapter_plugin_symbol_negative_signal.available } else { $false }
$adapterPluginSymbolNegativePass = if ($functional -and $functional.adapter_plugin_symbol_negative_signal -and $null -ne $functional.adapter_plugin_symbol_negative_signal.pass) { [bool]$functional.adapter_plugin_symbol_negative_signal.pass } else { $false }
$adapterPluginSymbolNegativeFail = if ($functional -and $functional.adapter_plugin_symbol_negative_signal -and $null -ne $functional.adapter_plugin_symbol_negative_signal.failed_as_expected) { [bool]$functional.adapter_plugin_symbol_negative_signal.failed_as_expected } else { $false }
$adapterPluginSymbolNegativeReasonMatch = if ($functional -and $functional.adapter_plugin_symbol_negative_signal -and $null -ne $functional.adapter_plugin_symbol_negative_signal.reason_match) { [bool]$functional.adapter_plugin_symbol_negative_signal.reason_match } else { $false }
$adapterPluginSymbolNegativeReason = if ($functional -and $functional.adapter_plugin_symbol_negative_signal -and $null -ne $functional.adapter_plugin_symbol_negative_signal.reason) { [string]$functional.adapter_plugin_symbol_negative_signal.reason } else { "" }
$adapterPluginRegistryNegativeEnabled = if ($functional -and $functional.adapter_plugin_registry_negative_signal -and $null -ne $functional.adapter_plugin_registry_negative_signal.enabled) { [bool]$functional.adapter_plugin_registry_negative_signal.enabled } else { $false }
$adapterPluginRegistryNegativeAvailable = if ($functional -and $functional.adapter_plugin_registry_negative_signal -and $null -ne $functional.adapter_plugin_registry_negative_signal.available) { [bool]$functional.adapter_plugin_registry_negative_signal.available } else { $false }
$adapterPluginRegistryNegativePass = if ($functional -and $functional.adapter_plugin_registry_negative_signal -and $null -ne $functional.adapter_plugin_registry_negative_signal.pass) { [bool]$functional.adapter_plugin_registry_negative_signal.pass } else { $false }
$adapterPluginRegistryNegativeReason = if ($functional -and $functional.adapter_plugin_registry_negative_signal -and $null -ne $functional.adapter_plugin_registry_negative_signal.reason) { [string]$functional.adapter_plugin_registry_negative_signal.reason } else { "" }
$adapterPluginRegistryNegativeHashFail = if ($functional -and $functional.adapter_plugin_registry_negative_signal -and $functional.adapter_plugin_registry_negative_signal.hash_mismatch -and $null -ne $functional.adapter_plugin_registry_negative_signal.hash_mismatch.failed_as_expected) { [bool]$functional.adapter_plugin_registry_negative_signal.hash_mismatch.failed_as_expected } else { $false }
$adapterPluginRegistryNegativeHashReason = if ($functional -and $functional.adapter_plugin_registry_negative_signal -and $functional.adapter_plugin_registry_negative_signal.hash_mismatch -and $null -ne $functional.adapter_plugin_registry_negative_signal.hash_mismatch.reason_match) { [bool]$functional.adapter_plugin_registry_negative_signal.hash_mismatch.reason_match } else { $false }
$adapterPluginRegistryNegativeWhitelistFail = if ($functional -and $functional.adapter_plugin_registry_negative_signal -and $functional.adapter_plugin_registry_negative_signal.whitelist_mismatch -and $null -ne $functional.adapter_plugin_registry_negative_signal.whitelist_mismatch.failed_as_expected) { [bool]$functional.adapter_plugin_registry_negative_signal.whitelist_mismatch.failed_as_expected } else { $false }
$adapterPluginRegistryNegativeWhitelistReason = if ($functional -and $functional.adapter_plugin_registry_negative_signal -and $functional.adapter_plugin_registry_negative_signal.whitelist_mismatch -and $null -ne $functional.adapter_plugin_registry_negative_signal.whitelist_mismatch.reason_match) { [bool]$functional.adapter_plugin_registry_negative_signal.whitelist_mismatch.reason_match } else { $false }
$networkBlockWireNegativeEnabled = if ($functional -and $functional.network_block_wire_negative_signal -and $null -ne $functional.network_block_wire_negative_signal.enabled) { [bool]$functional.network_block_wire_negative_signal.enabled } else { $false }
$networkBlockWireNegativeAvailable = if ($functional -and $functional.network_block_wire_negative_signal -and $null -ne $functional.network_block_wire_negative_signal.available) { [bool]$functional.network_block_wire_negative_signal.available } else { $false }
$networkBlockWireNegativePass = if ($functional -and $functional.network_block_wire_negative_signal -and $null -ne $functional.network_block_wire_negative_signal.pass) { [bool]$functional.network_block_wire_negative_signal.pass } else { $false }
$networkBlockWireNegativeExpectedFail = if ($functional -and $functional.network_block_wire_negative_signal -and $null -ne $functional.network_block_wire_negative_signal.expected_fail) { [bool]$functional.network_block_wire_negative_signal.expected_fail } else { $false }
$networkBlockWireNegativeReasonMatch = if ($functional -and $functional.network_block_wire_negative_signal -and $null -ne $functional.network_block_wire_negative_signal.reason_match) { [bool]$functional.network_block_wire_negative_signal.reason_match } else { $false }
$networkBlockWireNegativeTamperMode = if ($functional -and $functional.network_block_wire_negative_signal -and $null -ne $functional.network_block_wire_negative_signal.tamper_mode) { [string]$functional.network_block_wire_negative_signal.tamper_mode } else { "" }
$networkBlockWireNegativeVerified = if ($functional -and $functional.network_block_wire_negative_signal -and $null -ne $functional.network_block_wire_negative_signal.block_wire_verified) { [int]$functional.network_block_wire_negative_signal.block_wire_verified } else { 0 }
$networkBlockWireNegativeTotal = if ($functional -and $functional.network_block_wire_negative_signal -and $null -ne $functional.network_block_wire_negative_signal.block_wire_total) { [int]$functional.network_block_wire_negative_signal.block_wire_total } else { 0 }
$adapterCompareEnabled = if ($functional -and $functional.adapter_backend_compare_signal -and $null -ne $functional.adapter_backend_compare_signal.enabled) { [bool]$functional.adapter_backend_compare_signal.enabled } else { $false }
$adapterCompareAvailable = if ($functional -and $functional.adapter_backend_compare_signal -and $null -ne $functional.adapter_backend_compare_signal.available) { [bool]$functional.adapter_backend_compare_signal.available } else { $false }
$adapterComparePass = if ($functional -and $functional.adapter_backend_compare_signal -and $null -ne $functional.adapter_backend_compare_signal.pass) { [bool]$functional.adapter_backend_compare_signal.pass } else { $false }
$adapterCompareStateRootEqual = if ($functional -and $functional.adapter_backend_compare_signal -and $null -ne $functional.adapter_backend_compare_signal.state_root_equal) { [bool]$functional.adapter_backend_compare_signal.state_root_equal } else { $false }
$adapterComparePluginPath = if ($functional -and $functional.adapter_backend_compare_signal -and $null -ne $functional.adapter_backend_compare_signal.plugin_path) { [string]$functional.adapter_backend_compare_signal.plugin_path } else { "" }
$adapterCompareReason = if ($functional -and $functional.adapter_backend_compare_signal -and $null -ne $functional.adapter_backend_compare_signal.reason) { [string]$functional.adapter_backend_compare_signal.reason } else { "" }
$adapterCompareNativeBackend = if ($functional -and $functional.adapter_backend_compare_signal -and $functional.adapter_backend_compare_signal.native -and $functional.adapter_backend_compare_signal.native.adapter -and $null -ne $functional.adapter_backend_compare_signal.native.adapter.backend) { [string]$functional.adapter_backend_compare_signal.native.adapter.backend } else { "" }
$adapterComparePluginBackend = if ($functional -and $functional.adapter_backend_compare_signal -and $functional.adapter_backend_compare_signal.plugin -and $functional.adapter_backend_compare_signal.plugin.adapter -and $null -ne $functional.adapter_backend_compare_signal.plugin.adapter.backend) { [string]$functional.adapter_backend_compare_signal.plugin.adapter.backend } else { "" }
$adapterCompareNativeRoot = if ($functional -and $functional.adapter_backend_compare_signal -and $functional.adapter_backend_compare_signal.native -and $functional.adapter_backend_compare_signal.native.adapter -and $null -ne $functional.adapter_backend_compare_signal.native.adapter.state_root) { [string]$functional.adapter_backend_compare_signal.native.adapter.state_root } else { "" }
$adapterComparePluginRoot = if ($functional -and $functional.adapter_backend_compare_signal -and $functional.adapter_backend_compare_signal.plugin -and $functional.adapter_backend_compare_signal.plugin.adapter -and $null -ne $functional.adapter_backend_compare_signal.plugin.adapter.state_root) { [string]$functional.adapter_backend_compare_signal.plugin.adapter.state_root } else { "" }
$batchAAvailable = if ($functional -and $functional.batch_a_closure) { [bool]$functional.batch_a_closure.available } else { $false }
$batchAPass = if ($functional -and $functional.batch_a_closure) { [bool]$functional.batch_a_closure.pass } else { $false }
$batchADemoTxs = if ($functional -and $functional.batch_a_input_profile -and $null -ne $functional.batch_a_input_profile.demo_txs) { [int]$functional.batch_a_input_profile.demo_txs } else { 0 }
$batchATargetBatches = if ($functional -and $functional.batch_a_input_profile -and $null -ne $functional.batch_a_input_profile.target_batches) { [int]$functional.batch_a_input_profile.target_batches } else { 0 }
$batchAExpectedMinBatches = if ($functional -and $functional.batch_a_input_profile -and $null -ne $functional.batch_a_input_profile.expected_min_batches) { [int]$functional.batch_a_input_profile.expected_min_batches } else { 0 }
$blockWireAvailable = if ($functional -and $functional.block_wire_signal) { [bool]$functional.block_wire_signal.available } else { $false }
$blockWirePass = if ($functional -and $functional.block_wire_signal) { [bool]$functional.block_wire_signal.pass } else { $false }
$blockWireCodec = if ($functional -and $functional.block_wire_signal -and $functional.block_wire_signal.ffi_v2 -and $null -ne $functional.block_wire_signal.ffi_v2.codec) { [string]$functional.block_wire_signal.ffi_v2.codec } else { "" }
$blockWireBytes = if ($functional -and $functional.block_wire_signal -and $functional.block_wire_signal.ffi_v2 -and $null -ne $functional.block_wire_signal.ffi_v2.bytes) { [int]$functional.block_wire_signal.ffi_v2.bytes } else { 0 }
$blockOutAvailable = if ($functional -and $functional.block_output_signal) { [bool]$functional.block_output_signal.available } else { $false }
$blockOutPass = if ($functional -and $functional.block_output_signal) { [bool]$functional.block_output_signal.pass } else { $false }
$blockOutBatches = if ($functional -and $functional.block_output_signal -and $functional.block_output_signal.ffi_v2 -and $null -ne $functional.block_output_signal.ffi_v2.batches) { [int]$functional.block_output_signal.ffi_v2.batches } else { 0 }
$blockOutTxs = if ($functional -and $functional.block_output_signal -and $functional.block_output_signal.ffi_v2 -and $null -ne $functional.block_output_signal.ffi_v2.txs) { [int]$functional.block_output_signal.ffi_v2.txs } else { 0 }
$commitOutAvailable = if ($functional -and $functional.commit_output_signal) { [bool]$functional.commit_output_signal.available } else { $false }
$commitOutPass = if ($functional -and $functional.commit_output_signal) { [bool]$functional.commit_output_signal.pass } else { $false }
$networkOutAvailable = if ($functional -and $functional.network_output_signal) { [bool]$functional.network_output_signal.available } else { $false }
$networkOutPass = if ($functional -and $functional.network_output_signal) { [bool]$functional.network_output_signal.pass } else { $false }
$networkClosureAvailable = if ($functional -and $functional.network_closure_signal) { [bool]$functional.network_closure_signal.available } else { $false }
$networkClosurePass = if ($functional -and $functional.network_closure_signal) { [bool]$functional.network_closure_signal.pass } else { $false }
$networkPacemakerAvailable = if ($functional -and ($functional.PSObject.Properties.Name -contains "network_pacemaker_signal") -and $functional.network_pacemaker_signal -and $null -ne $functional.network_pacemaker_signal.available) { [bool]$functional.network_pacemaker_signal.available } else { $false }
$networkPacemakerPass = if ($functional -and ($functional.PSObject.Properties.Name -contains "network_pacemaker_signal") -and $functional.network_pacemaker_signal -and $null -ne $functional.network_pacemaker_signal.pass) { [bool]$functional.network_pacemaker_signal.pass } else { $false }
$networkProcessAvailable = if ($functional -and $functional.network_process_signal) { [bool]$functional.network_process_signal.available } else { $false }
$networkProcessPass = if ($functional -and $functional.network_process_signal) { [bool]$functional.network_process_signal.pass } else { $false }
$networkProcessRounds = if ($functional -and $functional.network_process_signal -and $null -ne $functional.network_process_signal.rounds) { [int]$functional.network_process_signal.rounds } else { 1 }
$networkProcessRoundsPassed = if ($functional -and $functional.network_process_signal -and $null -ne $functional.network_process_signal.rounds_passed) { [int]$functional.network_process_signal.rounds_passed } else { if ($networkProcessPass) { 1 } else { 0 } }
$networkProcessRoundPassRatio = if ($functional -and $functional.network_process_signal -and $null -ne $functional.network_process_signal.round_pass_ratio) { [double]$functional.network_process_signal.round_pass_ratio } else { if ($networkProcessPass) { 1.0 } else { 0.0 } }
$networkProcessNodeCount = if ($functional -and $functional.network_process_signal -and $null -ne $functional.network_process_signal.node_count) { [int]$functional.network_process_signal.node_count } else { 0 }
$networkProcessTotalPairs = if ($functional -and $functional.network_process_signal -and $null -ne $functional.network_process_signal.total_pairs) { [int]$functional.network_process_signal.total_pairs } else { 0 }
$networkProcessPassedPairs = if ($functional -and $functional.network_process_signal -and $null -ne $functional.network_process_signal.passed_pairs) { [int]$functional.network_process_signal.passed_pairs } else { 0 }
$networkProcessPassRatio = if ($functional -and $functional.network_process_signal -and $null -ne $functional.network_process_signal.pair_pass_ratio) { [double]$functional.network_process_signal.pair_pass_ratio } else { 0.0 }
$networkProcessMode = if ($functional -and $functional.network_process_signal -and $null -ne $functional.network_process_signal.mode) { [string]$functional.network_process_signal.mode } else { "" }
$networkDirectedTotal = if ($functional -and $functional.network_process_signal -and $null -ne $functional.network_process_signal.directed_edges_total) { [int]$functional.network_process_signal.directed_edges_total } else { 0 }
$networkDirectedUp = if ($functional -and $functional.network_process_signal -and $null -ne $functional.network_process_signal.directed_edges_up) { [int]$functional.network_process_signal.directed_edges_up } else { 0 }
$networkDirectedRatio = if ($functional -and $functional.network_process_signal -and $null -ne $functional.network_process_signal.directed_edge_ratio) { [double]$functional.network_process_signal.directed_edge_ratio } else { 0.0 }
$networkBlockWireAvailable = if ($functional -and $functional.network_process_signal -and $null -ne $functional.network_process_signal.block_wire_available) { [bool]$functional.network_process_signal.block_wire_available } else { $false }
$networkBlockWirePass = if ($functional -and $functional.network_process_signal -and $null -ne $functional.network_process_signal.block_wire_pass) { [bool]$functional.network_process_signal.block_wire_pass } else { $false }
$networkBlockWirePassRatio = if ($functional -and $functional.network_process_signal -and $null -ne $functional.network_process_signal.block_wire_pass_ratio) { [double]$functional.network_process_signal.block_wire_pass_ratio } else { 0.0 }
$networkBlockWireVerified = if ($functional -and $functional.network_process_signal -and $null -ne $functional.network_process_signal.block_wire_verified) { [int]$functional.network_process_signal.block_wire_verified } else { 0 }
$networkBlockWireTotal = if ($functional -and $functional.network_process_signal -and $null -ne $functional.network_process_signal.block_wire_total) { [int]$functional.network_process_signal.block_wire_total } else { 0 }
$networkBlockWireVerifiedRatio = if ($functional -and $functional.network_process_signal -and $null -ne $functional.network_process_signal.block_wire_verified_ratio) { [double]$functional.network_process_signal.block_wire_verified_ratio } else { 0.0 }
$networkViewSyncAvailable = if ($functional -and $functional.network_process_signal -and (($functional.network_process_signal.PSObject.Properties.Name -contains "view_sync_available") -and $null -ne $functional.network_process_signal.view_sync_available)) { [bool]$functional.network_process_signal.view_sync_available } else { $false }
$networkViewSyncPass = if ($functional -and $functional.network_process_signal -and (($functional.network_process_signal.PSObject.Properties.Name -contains "view_sync_pass") -and $null -ne $functional.network_process_signal.view_sync_pass)) { [bool]$functional.network_process_signal.view_sync_pass } else { $false }
$networkViewSyncPassRatio = if ($functional -and $functional.network_process_signal -and (($functional.network_process_signal.PSObject.Properties.Name -contains "view_sync_pass_ratio") -and $null -ne $functional.network_process_signal.view_sync_pass_ratio)) { [double]$functional.network_process_signal.view_sync_pass_ratio } else { 0.0 }
$networkNewViewAvailable = if ($functional -and $functional.network_process_signal -and (($functional.network_process_signal.PSObject.Properties.Name -contains "new_view_available") -and $null -ne $functional.network_process_signal.new_view_available)) { [bool]$functional.network_process_signal.new_view_available } else { $false }
$networkNewViewPass = if ($functional -and $functional.network_process_signal -and (($functional.network_process_signal.PSObject.Properties.Name -contains "new_view_pass") -and $null -ne $functional.network_process_signal.new_view_pass)) { [bool]$functional.network_process_signal.new_view_pass } else { $false }
$networkNewViewPassRatio = if ($functional -and $functional.network_process_signal -and (($functional.network_process_signal.PSObject.Properties.Name -contains "new_view_pass_ratio") -and $null -ne $functional.network_process_signal.new_view_pass_ratio)) { [double]$functional.network_process_signal.new_view_pass_ratio } else { 0.0 }
$coordinatorSignalEnabled = if ($functional -and $functional.coordinator_signal -and $null -ne $functional.coordinator_signal.enabled) { [bool]$functional.coordinator_signal.enabled } else { $false }
$coordinatorSignalAvailable = if ($functional -and $functional.coordinator_signal -and $null -ne $functional.coordinator_signal.available) { [bool]$functional.coordinator_signal.available } else { $false }
$coordinatorSignalPass = if ($functional -and $functional.coordinator_signal -and $null -ne $functional.coordinator_signal.pass) { [bool]$functional.coordinator_signal.pass } else { $false }
$coordinatorSignalReason = if ($functional -and $functional.coordinator_signal -and $null -ne $functional.coordinator_signal.reason) { [string]$functional.coordinator_signal.reason } else { "" }
$coordinatorNegativeEnabled = if ($functional -and $functional.coordinator_negative_signal -and $null -ne $functional.coordinator_negative_signal.enabled) { [bool]$functional.coordinator_negative_signal.enabled } else { $false }
$coordinatorNegativeAvailable = if ($functional -and $functional.coordinator_negative_signal -and $null -ne $functional.coordinator_negative_signal.available) { [bool]$functional.coordinator_negative_signal.available } else { $false }
$coordinatorNegativePass = if ($functional -and $functional.coordinator_negative_signal -and $null -ne $functional.coordinator_negative_signal.pass) { [bool]$functional.coordinator_negative_signal.pass } else { $false }
$coordinatorNegativeUnknownPrepare = if ($functional -and $functional.coordinator_negative_signal -and $null -ne $functional.coordinator_negative_signal.unknown_prepare) { [bool]$functional.coordinator_negative_signal.unknown_prepare } else { $false }
$coordinatorNegativeNonParticipant = if ($functional -and $functional.coordinator_negative_signal -and $null -ne $functional.coordinator_negative_signal.non_participant_vote) { [bool]$functional.coordinator_negative_signal.non_participant_vote } else { $false }
$coordinatorNegativeVoteAfterDecide = if ($functional -and $functional.coordinator_negative_signal -and $null -ne $functional.coordinator_negative_signal.vote_after_decide) { [bool]$functional.coordinator_negative_signal.vote_after_decide } else { $false }
$coordinatorNegativeDuplicateTx = if ($functional -and $functional.coordinator_negative_signal -and $null -ne $functional.coordinator_negative_signal.duplicate_tx) { [bool]$functional.coordinator_negative_signal.duplicate_tx } else { $false }
$coordinatorNegativeReason = if ($functional -and $functional.coordinator_negative_signal -and $null -ne $functional.coordinator_negative_signal.reason) { [string]$functional.coordinator_negative_signal.reason } else { "" }
$proverContractSignalEnabled = if ($functional -and $functional.prover_contract_signal -and $null -ne $functional.prover_contract_signal.enabled) { [bool]$functional.prover_contract_signal.enabled } else { $false }
$proverContractSignalAvailable = if ($functional -and $functional.prover_contract_signal -and $null -ne $functional.prover_contract_signal.available) { [bool]$functional.prover_contract_signal.available } else { $false }
$proverContractSignalPass = if ($functional -and $functional.prover_contract_signal -and $null -ne $functional.prover_contract_signal.pass) { [bool]$functional.prover_contract_signal.pass } else { $false }
$proverContractSchemaOk = if ($functional -and $functional.prover_contract_signal -and $null -ne $functional.prover_contract_signal.schema_ok) { [bool]$functional.prover_contract_signal.schema_ok } else { $false }
$proverContractReasonNorm = if ($functional -and $functional.prover_contract_signal -and $null -ne $functional.prover_contract_signal.normalized_reason_codes) { [bool]$functional.prover_contract_signal.normalized_reason_codes } else { $false }
$proverContractFallbackCodes = if ($functional -and $functional.prover_contract_signal -and $null -ne $functional.prover_contract_signal.fallback_codes) { [int]$functional.prover_contract_signal.fallback_codes } else { 0 }
$proverContractReason = if ($functional -and $functional.prover_contract_signal -and $null -ne $functional.prover_contract_signal.reason) { [string]$functional.prover_contract_signal.reason } else { "" }
$proverContractNegativeEnabled = if ($functional -and $functional.prover_contract_negative_signal -and $null -ne $functional.prover_contract_negative_signal.enabled) { [bool]$functional.prover_contract_negative_signal.enabled } else { $false }
$proverContractNegativeAvailable = if ($functional -and $functional.prover_contract_negative_signal -and $null -ne $functional.prover_contract_negative_signal.available) { [bool]$functional.prover_contract_negative_signal.available } else { $false }
$proverContractNegativePass = if ($functional -and $functional.prover_contract_negative_signal -and $null -ne $functional.prover_contract_negative_signal.pass) { [bool]$functional.prover_contract_negative_signal.pass } else { $false }
$proverContractNegativeMissingFormal = if ($functional -and $functional.prover_contract_negative_signal -and $null -ne $functional.prover_contract_negative_signal.missing_formal_fields) { [bool]$functional.prover_contract_negative_signal.missing_formal_fields } else { $false }
$proverContractNegativeEmptyReasons = if ($functional -and $functional.prover_contract_negative_signal -and $null -ne $functional.prover_contract_negative_signal.empty_reason_codes) { [bool]$functional.prover_contract_negative_signal.empty_reason_codes } else { $false }
$proverContractNegativeNormStable = if ($functional -and $functional.prover_contract_negative_signal -and $null -ne $functional.prover_contract_negative_signal.reason_normalization_stable) { [bool]$functional.prover_contract_negative_signal.reason_normalization_stable } else { $false }
$proverContractNegativeReason = if ($functional -and $functional.prover_contract_negative_signal -and $null -ne $functional.prover_contract_negative_signal.reason) { [string]$functional.prover_contract_negative_signal.reason } else { "" }
$consensusNegativeEnabled = if ($functional -and $functional.consensus_negative_signal -and $null -ne $functional.consensus_negative_signal.enabled) { [bool]$functional.consensus_negative_signal.enabled } else { $false }
$consensusNegativeAvailable = if ($functional -and $functional.consensus_negative_signal -and $null -ne $functional.consensus_negative_signal.available) { [bool]$functional.consensus_negative_signal.available } else { $false }
$consensusNegativePass = if ($functional -and $functional.consensus_negative_signal -and $null -ne $functional.consensus_negative_signal.pass) { [bool]$functional.consensus_negative_signal.pass } else { $false }
$consensusNegativeInvalidSignature = if ($functional -and $functional.consensus_negative_signal -and $null -ne $functional.consensus_negative_signal.invalid_signature) { [bool]$functional.consensus_negative_signal.invalid_signature } else { $false }
$consensusNegativeDuplicateVote = if ($functional -and $functional.consensus_negative_signal -and $null -ne $functional.consensus_negative_signal.duplicate_vote) { [bool]$functional.consensus_negative_signal.duplicate_vote } else { $false }
$consensusNegativeWrongEpoch = if ($functional -and $functional.consensus_negative_signal -and $null -ne $functional.consensus_negative_signal.wrong_epoch) { [bool]$functional.consensus_negative_signal.wrong_epoch } else { $false }
$consensusNegativeWeightedQuorum = if ($functional -and $functional.consensus_negative_signal -and (($functional.consensus_negative_signal.PSObject.Properties.Name -contains "weighted_quorum") -and $null -ne $functional.consensus_negative_signal.weighted_quorum)) { [bool]$functional.consensus_negative_signal.weighted_quorum } else { $false }
$consensusNegativeEquivocation = if ($functional -and $functional.consensus_negative_signal -and (($functional.consensus_negative_signal.PSObject.Properties.Name -contains "equivocation") -and $null -ne $functional.consensus_negative_signal.equivocation)) { [bool]$functional.consensus_negative_signal.equivocation } else { $false }
$consensusNegativeSlashExecution = if ($functional -and $functional.consensus_negative_signal -and (($functional.consensus_negative_signal.PSObject.Properties.Name -contains "slash_execution") -and $null -ne $functional.consensus_negative_signal.slash_execution)) { [bool]$functional.consensus_negative_signal.slash_execution } else { $false }
$consensusNegativeSlashThreshold = if ($functional -and $functional.consensus_negative_signal -and (($functional.consensus_negative_signal.PSObject.Properties.Name -contains "slash_threshold") -and $null -ne $functional.consensus_negative_signal.slash_threshold)) { [bool]$functional.consensus_negative_signal.slash_threshold } else { $false }
$consensusNegativeSlashObserveOnly = if ($functional -and $functional.consensus_negative_signal -and (($functional.consensus_negative_signal.PSObject.Properties.Name -contains "slash_observe_only") -and $null -ne $functional.consensus_negative_signal.slash_observe_only)) { [bool]$functional.consensus_negative_signal.slash_observe_only } else { $false }
$consensusNegativeUnjailCooldown = if ($functional -and $functional.consensus_negative_signal -and (($functional.consensus_negative_signal.PSObject.Properties.Name -contains "unjail_cooldown") -and $null -ne $functional.consensus_negative_signal.unjail_cooldown)) { [bool]$functional.consensus_negative_signal.unjail_cooldown } else { $false }
$consensusNegativeViewChange = if ($functional -and $functional.consensus_negative_signal -and (($functional.consensus_negative_signal.PSObject.Properties.Name -contains "view_change") -and $null -ne $functional.consensus_negative_signal.view_change)) { [bool]$functional.consensus_negative_signal.view_change } else { $false }
$consensusNegativeForkChoice = if ($functional -and $functional.consensus_negative_signal -and (($functional.consensus_negative_signal.PSObject.Properties.Name -contains "fork_choice") -and $null -ne $functional.consensus_negative_signal.fork_choice)) { [bool]$functional.consensus_negative_signal.fork_choice } else { $false }
$consensusNegativeReason = if ($functional -and $functional.consensus_negative_signal -and $null -ne $functional.consensus_negative_signal.reason) { [string]$functional.consensus_negative_signal.reason } else { "" }
$capContract = if ($capability) { $capability.contract } else { $null }
$zkProve = if ($capContract) { [bool]$capContract.zkvm_prove } else { $false }
$zkVerify = if ($capContract) { [bool]$capContract.zkvm_verify } else { $false }
$zkFormalFieldsPresent = if ($capContract -and $null -ne $capContract.zk_formal_fields_present) { [bool]$capContract.zk_formal_fields_present } else { $false }
$msmAccel = if ($capContract) { [bool]$capContract.msm_accel } else { $false }
$msmBackend = if ($capContract) { [string]$capContract.msm_backend } else { "" }
$fallbackReason = if ($capContract -and $null -ne $capContract.fallback_reason) { [string]$capContract.fallback_reason } else { "" }
$fallbackReasonCodes = if ($capContract -and $null -ne $capContract.fallback_reason_codes) { @($capContract.fallback_reason_codes) } else { @() }
$capPropNames = @()
if ($capContract) { $capPropNames = @($capContract.PSObject.Properties.Name) }
$capHasFallbackReason = $capPropNames -contains "fallback_reason"
$capHasFallbackReasonCodes = $capPropNames -contains "fallback_reason_codes"
$capHasZkFormalFlag = $capPropNames -contains "zk_formal_fields_present"
$zkContractSchemaReady = ($capHasFallbackReason -and $capHasFallbackReasonCodes -and $capHasZkFormalFlag)
$proverCap = if ($capability -and $capability.prover_contract) { $capability.prover_contract } else { $null }
$proverReady = if ($proverCap -and $null -ne $proverCap.prover_ready) { [bool]$proverCap.prover_ready } else { $false }
$zkReady = ($zkProve -or $zkVerify)
$msmReady = $msmAccel
$baselineReady = $null -ne $baseline
$execSkeletonReady = Test-Path (Join-Path $RepoRoot "crates\novovm-exec\Cargo.toml")
$bindingsSkeletonReady = Test-Path (Join-Path $RepoRoot "crates\aoem-bindings\Cargo.toml")
$protocolSkeletonReady = Test-Path (Join-Path $RepoRoot "crates\novovm-protocol\Cargo.toml")
$consensusSkeletonReady = Test-Path (Join-Path $RepoRoot "crates\novovm-consensus\Cargo.toml")
$networkSkeletonReady = Test-Path (Join-Path $RepoRoot "crates\novovm-network\Cargo.toml")
$adapterSkeletonReady = Test-Path (Join-Path $RepoRoot "crates\novovm-adapter-api\Cargo.toml")
$adapterNativeReady = Test-Path (Join-Path $RepoRoot "crates\novovm-adapter-novovm\Cargo.toml")
$adapterPluginReady = Test-Path (Join-Path $RepoRoot "crates\novovm-adapter-sample-plugin\Cargo.toml")
$adapterCompatMatrixPath = Join-Path $RepoRoot "config\novovm-adapter-compatibility-matrix.json"
$adapterCompatMatrixReady = Test-Path $adapterCompatMatrixPath
$adapterCompatHasEvm = $false
$adapterCompatHasBnb = $false
if ($adapterCompatMatrixReady) {
    try {
        $adapterCompat = Get-Content -Path $adapterCompatMatrixPath -Raw | ConvertFrom-Json
        $compatChains = @()
        if ($adapterCompat.chains) {
            $compatChains = @($adapterCompat.chains | ForEach-Object { [string]$_ })
        }
        $adapterCompatHasEvm = $compatChains -contains "evm"
        $adapterCompatHasBnb = $compatChains -contains "bnb"
    } catch {
        $adapterCompatMatrixReady = $false
    }
}
$adapterRegistryConfigPath = Join-Path $RepoRoot "config\novovm-adapter-plugin-registry.json"
$adapterRegistryHasEvm = $false
$adapterRegistryHasBnb = $false
if (Test-Path $adapterRegistryConfigPath) {
    try {
        $registryJson = Get-Content -Path $adapterRegistryConfigPath -Raw | ConvertFrom-Json
        $allChains = @()
        if ($registryJson.plugins) {
            foreach ($plugin in $registryJson.plugins) {
                if ($plugin.chains) {
                    $allChains += @($plugin.chains | ForEach-Object { [string]$_ })
                }
            }
        }
        $adapterRegistryHasEvm = $allChains -contains "evm"
        $adapterRegistryHasBnb = $allChains -contains "bnb"
    } catch {
        $adapterRegistryHasEvm = $false
        $adapterRegistryHasBnb = $false
    }
}
$adapterNonNovoSampleReady = (
    $adapterCompatMatrixReady -and
    $adapterCompatHasEvm -and
    $adapterCompatHasBnb -and
    $adapterRegistryHasEvm -and
    $adapterRegistryHasBnb
)
$coordinatorSkeletonReady = Test-Path (Join-Path $RepoRoot "crates\novovm-coordinator\Cargo.toml")
$proverSkeletonReady = Test-Path (Join-Path $RepoRoot "crates\novovm-prover\Cargo.toml")
$appStorageSkeletonReady = Test-Path (Join-Path $RepoRoot "crates\novovm-storage-service\Cargo.toml")
$appDomainSkeletonReady = Test-Path (Join-Path $RepoRoot "crates\novovm-app-domain\Cargo.toml")
$appDefiSkeletonReady = Test-Path (Join-Path $RepoRoot "crates\novovm-app-defi\Cargo.toml")
$adaptersMultiSkeletonReady = Test-Path (Join-Path $RepoRoot "crates\novovm-adapters")
$legacyVmRuntimePresent = Test-Path (Join-Path $RepoRoot "src\vm-runtime")

$f01Status = if ($execSkeletonReady -and $bindingsSkeletonReady -and $adapterPass) { "ReadyForMerge" } elseif ($execSkeletonReady -and $bindingsSkeletonReady) { "InProgress" } else { "NotStarted" }
$variantDigestPass = if ($functional -and $functional.variant_digest_consistency -and $null -ne $functional.variant_digest_consistency.pass) { [bool]$functional.variant_digest_consistency.pass } else { $false }
$f02Status = if ($execSkeletonReady -and $variantDigestPass) { "ReadyForMerge" } elseif ($execSkeletonReady) { "InProgress" } else { "NotStarted" }
$f03Status = if ($protocolSkeletonReady -and $txCodecPass -and $blockWirePass -and $blockOutPass -and $commitOutPass) { "ReadyForMerge" } elseif ($protocolSkeletonReady) { "InProgress" } else { "NotStarted" }
$f04Status = if ($stateRootAvailable -and $stateRootPass) { "ReadyForMerge" } elseif ($protocolSkeletonReady -and $stateRootPass) { "InProgress" } else { "NotStarted" }
$f05ReadyForMerge = (
    $consensusSkeletonReady -and
    $batchAPass -and
    $functionalPass -and
    (
        (-not $consensusNegativeEnabled) -or
        ($consensusNegativeAvailable -and $consensusNegativePass)
    )
)
$f05Status = if ($f05ReadyForMerge) { "ReadyForMerge" } elseif ($consensusSkeletonReady) { "InProgress" } else { "NotStarted" }
$f06ReadyForMerge = (
    $coordinatorSkeletonReady -and
    $coordinatorSignalAvailable -and
    $coordinatorSignalPass -and
    $functionalPass -and
    (
        (-not $coordinatorNegativeEnabled) -or
        ($coordinatorNegativeAvailable -and $coordinatorNegativePass)
    )
)
$f06Status = if ($f06ReadyForMerge) { "ReadyForMerge" } elseif ($coordinatorSkeletonReady) { "InProgress" } else { "NotStarted" }
$f07ReadyForMerge = (
    $networkSkeletonReady -and
    $networkOutPass -and
    $networkClosurePass -and
    $networkPacemakerPass -and
    $networkProcessPass -and
    $networkBlockWirePass -and
    (
        (-not $networkBlockWireNegativeEnabled) -or
        ($networkBlockWireNegativeAvailable -and $networkBlockWireNegativePass)
    )
)
$f07Status = if ($f07ReadyForMerge) { "ReadyForMerge" } elseif ($networkSkeletonReady) { "InProgress" } else { "NotStarted" }
$f08ReadyForMerge = (
    $adapterSkeletonReady -and
    $adapterNativeReady -and
    $adapterPluginReady -and
    $adapterPluginAbiPass -and
    $adapterPluginRegistryPass -and
    $adapterConsensusPass -and
    $adapterComparePass -and
    $adapterNonNovoSampleReady -and
    (
        (-not $adapterPluginAbiNegativeEnabled) -or
        ($adapterPluginAbiNegativeAvailable -and $adapterPluginAbiNegativePass)
    ) -and
    (
        (-not $adapterPluginSymbolNegativeEnabled) -or
        ($adapterPluginSymbolNegativeAvailable -and $adapterPluginSymbolNegativePass)
    ) -and
    (
        (-not $adapterPluginRegistryNegativeEnabled) -or
        ($adapterPluginRegistryNegativeAvailable -and $adapterPluginRegistryNegativePass)
    )
)
$f08Status = if ($f08ReadyForMerge) { "ReadyForMerge" } elseif ($adapterSkeletonReady) { "InProgress" } else { "NotStarted" }
$f09ReadyForMerge = (
    $proverSkeletonReady -and
    $proverContractSignalAvailable -and
    $proverContractSignalPass -and
    $functionalPass -and
    (
        (-not $proverContractNegativeEnabled) -or
        ($proverContractNegativeAvailable -and $proverContractNegativePass)
    )
)
$f09Status = if ($f09ReadyForMerge) { "ReadyForMerge" } elseif ($proverSkeletonReady -or $capContract) { "InProgress" } else { "NotStarted" }
$f10Status = if ($appStorageSkeletonReady) { "InProgress" } else { "NotStarted" }
$f11Status = if ($appDomainSkeletonReady) { "InProgress" } else { "NotStarted" }
$f12Status = if ($appDefiSkeletonReady) { "InProgress" } else { "NotStarted" }
$f13Status = if ($adaptersMultiSkeletonReady) { "InProgress" } else { "NotStarted" }
$f14Status = if ($protocolSkeletonReady -and $consensusSkeletonReady -and $networkSkeletonReady -and $adapterSkeletonReady) { "InProgress" } elseif ($legacyVmRuntimePresent) { "NotStarted" } else { "NotStarted" }
$f15Status = if ($zkContractSchemaReady -and $proverContractSignalPass -and $functionalPass) { "ReadyForMerge" } elseif ($capContract) { "InProgress" } else { "NotStarted" }
$f16Status = if ($msmReady -and $functionalPass) { "ReadyForMerge" } elseif ($capContract) { "InProgress" } else { "NotStarted" }

$domainD0Done = ($f01Status -eq "ReadyForMerge" -and $f02Status -eq "ReadyForMerge")
$domainD1Done = ($f01Status -eq "ReadyForMerge" -and $f02Status -eq "ReadyForMerge" -and $functionalPass)
$domainD2Done = ($f03Status -eq "ReadyForMerge" -and $f04Status -eq "ReadyForMerge")
$domainD3Done = ($f05Status -eq "ReadyForMerge" -and $f06Status -eq "ReadyForMerge" -and $f07Status -eq "ReadyForMerge" -and $f08Status -eq "ReadyForMerge")

$domainD0Status = if ($domainD0Done) { "Done" } elseif ($f01Status -ne "NotStarted" -or $f02Status -ne "NotStarted") { "InProgress" } else { "NotStarted" }
$domainD1Status = if ($domainD1Done) { "Done" } elseif ($f01Status -ne "NotStarted" -or $f02Status -ne "NotStarted") { "InProgress" } else { "NotStarted" }
$domainD2Status = if ($domainD2Done) { "Done" } elseif ($f03Status -ne "NotStarted" -or $f04Status -ne "NotStarted") { "InProgress" } else { "NotStarted" }
$domainD3Status = if ($domainD3Done) { "Done" } elseif ($f05Status -ne "NotStarted" -or $f06Status -ne "NotStarted" -or $f07Status -ne "NotStarted" -or $f08Status -ne "NotStarted") { "InProgress" } else { "NotStarted" }
$adapterEvidence = if ($adapterAvailable) { $FunctionalJson } else { Join-Path $RepoRoot "crates\novovm-adapter-api" }
$networkDirectedSummary = "{0}/{1}:{2}" -f $networkDirectedUp, $networkDirectedTotal, $networkDirectedRatio
$networkBlockWireSummary = "{0}/{1}:{2}" -f $networkBlockWireVerified, $networkBlockWireTotal, $networkBlockWireVerifiedRatio
$adapterCompareSummary = if ($adapterCompareEnabled) {
    "compare(enabled=$adapterCompareEnabled, available=$adapterCompareAvailable, pass=$adapterComparePass, state_root_equal=$adapterCompareStateRootEqual, native_backend=$adapterCompareNativeBackend, plugin_backend=$adapterComparePluginBackend)"
} else {
    "compare(enabled=false)"
}
$adapterPluginAbiSummary = "plugin_abi(pass=$adapterPluginAbiPass, enabled=$adapterPluginAbiEnabled, compatible=$adapterPluginAbiCompatible, expected=$adapterPluginAbiExpected, required=$adapterPluginAbiRequired)"
$adapterPluginRegistrySummary = "plugin_registry(pass=$adapterPluginRegistryPass, enabled=$adapterPluginRegistryEnabled, matched=$adapterPluginRegistryMatched, strict=$adapterPluginRegistryStrict, chain_allowed=$adapterPluginRegistryChainAllowed, hash_check=$adapterPluginRegistryHashCheck/$adapterPluginRegistryHashMatch, abi_whitelist=$adapterPluginRegistryAbiWhitelist/$adapterPluginRegistryAbiAllowed)"
$adapterConsensusSummary = "consensus_binding(pass=$adapterConsensusPass, available=$adapterConsensusAvailable, class=$adapterConsensusClass/$adapterConsensusClassCode)"
$adapterMatrixSummary = "compat_matrix(ready=$adapterCompatMatrixReady, evm=$adapterCompatHasEvm, bnb=$adapterCompatHasBnb, registry_evm=$adapterRegistryHasEvm, registry_bnb=$adapterRegistryHasBnb, non_novovm_sample=$adapterNonNovoSampleReady)"
$coordinatorSummary = "coordinator_signal(enabled=$coordinatorSignalEnabled, available=$coordinatorSignalAvailable, pass=$coordinatorSignalPass, reason=$coordinatorSignalReason)"
$coordinatorNegativeSummary = "coordinator_negative_signal(enabled=$coordinatorNegativeEnabled, available=$coordinatorNegativeAvailable, pass=$coordinatorNegativePass, unknown_prepare=$coordinatorNegativeUnknownPrepare, non_participant_vote=$coordinatorNegativeNonParticipant, vote_after_decide=$coordinatorNegativeVoteAfterDecide, duplicate_tx=$coordinatorNegativeDuplicateTx, reason=$coordinatorNegativeReason)"
$proverContractSummary = "prover_contract_signal(enabled=$proverContractSignalEnabled, available=$proverContractSignalAvailable, pass=$proverContractSignalPass, schema_ok=$proverContractSchemaOk, reason_norm=$proverContractReasonNorm, fallback_codes=$proverContractFallbackCodes, reason=$proverContractReason)"
$proverContractNegativeSummary = "prover_contract_negative_signal(enabled=$proverContractNegativeEnabled, available=$proverContractNegativeAvailable, pass=$proverContractNegativePass, missing_formal_fields=$proverContractNegativeMissingFormal, empty_reason_codes=$proverContractNegativeEmptyReasons, normalization_stable=$proverContractNegativeNormStable, reason=$proverContractNegativeReason)"
$consensusNegativeSummary = "consensus_negative_signal(enabled=$consensusNegativeEnabled, available=$consensusNegativeAvailable, pass=$consensusNegativePass, invalid_signature=$consensusNegativeInvalidSignature, duplicate_vote=$consensusNegativeDuplicateVote, wrong_epoch=$consensusNegativeWrongEpoch, weighted_quorum=$consensusNegativeWeightedQuorum, equivocation=$consensusNegativeEquivocation, slash_execution=$consensusNegativeSlashExecution, slash_threshold=$consensusNegativeSlashThreshold, slash_observe_only=$consensusNegativeSlashObserveOnly, unjail_cooldown=$consensusNegativeUnjailCooldown, view_change=$consensusNegativeViewChange, fork_choice=$consensusNegativeForkChoice, reason=$consensusNegativeReason)"
$adapterPluginAbiNegativeSummary = if ($adapterPluginAbiNegativeEnabled) {
    "plugin_abi_negative(pass=$adapterPluginAbiNegativePass, available=$adapterPluginAbiNegativeAvailable, abi_fail=$adapterPluginAbiNegativeAbiFail/$adapterPluginAbiNegativeAbiReason, cap_fail=$adapterPluginAbiNegativeCapFail/$adapterPluginAbiNegativeCapReason)"
} else {
    "plugin_abi_negative(enabled=false)"
}
$adapterPluginSymbolNegativeSummary = if ($adapterPluginSymbolNegativeEnabled) {
    "plugin_symbol_negative(pass=$adapterPluginSymbolNegativePass, available=$adapterPluginSymbolNegativeAvailable, fail=$adapterPluginSymbolNegativeFail/$adapterPluginSymbolNegativeReasonMatch)"
} else {
    "plugin_symbol_negative(enabled=false)"
}
$adapterPluginRegistryNegativeSummary = if ($adapterPluginRegistryNegativeEnabled) {
    "plugin_registry_negative(pass=$adapterPluginRegistryNegativePass, available=$adapterPluginRegistryNegativeAvailable, hash_fail=$adapterPluginRegistryNegativeHashFail/$adapterPluginRegistryNegativeHashReason, whitelist_fail=$adapterPluginRegistryNegativeWhitelistFail/$adapterPluginRegistryNegativeWhitelistReason)"
} else {
    "plugin_registry_negative(enabled=false)"
}
$networkBlockWireNegativeSummary = if ($networkBlockWireNegativeEnabled) {
    "network_block_wire_negative(pass=$networkBlockWireNegativePass, available=$networkBlockWireNegativeAvailable, expected_fail=$networkBlockWireNegativeExpectedFail, reason_match=$networkBlockWireNegativeReasonMatch, tamper=$networkBlockWireNegativeTamperMode, verified=$networkBlockWireNegativeVerified/$networkBlockWireNegativeTotal)"
} else {
    "network_block_wire_negative(enabled=false)"
}

$md = @(
    "# NOVOVM Capability Migration Ledger (Auto Snapshot) - $today"
    ""
    "- generated_at_utc: $generatedAt"
    "- functional_report: $FunctionalJson"
    "- performance_report: $PerformanceJson"
    "- capability_snapshot: $CapabilityJson"
    "- svm2026_baseline: $BaselineJson"
    ""
    "## Auto Summary"
    ""
    "- functional_overall_pass: $functionalPass"
    "- performance_compare_pass: $performancePass"
    "- state_root_available: $stateRootAvailable"
    "- state_root_pass: $stateRootPass"
    "- tx_codec_signal_available: $txCodecAvailable"
    "- tx_codec_signal_pass: $txCodecPass"
    "- tx_codec_bytes: $txCodecBytes"
    "- mempool_admission_signal_available: $mempoolAvailable"
    "- mempool_admission_signal_pass: $mempoolPass"
    "- mempool_admission_accepted: $mempoolAccepted"
    "- mempool_admission_rejected: $mempoolRejected"
    "- mempool_admission_fee_floor: $mempoolFeeFloor"
    "- tx_metadata_signal_available: $txMetaAvailable"
    "- tx_metadata_signal_pass: $txMetaPass"
    "- tx_metadata_accounts: $txMetaAccounts"
    "- tx_metadata_min_fee: $txMetaMinFee"
    "- tx_metadata_max_fee: $txMetaMaxFee"
    "- adapter_signal_available: $adapterAvailable"
    "- adapter_signal_pass: $adapterPass"
    "- adapter_signal_backend: $adapterBackend"
    "- adapter_signal_chain: $adapterChain"
    "- adapter_signal_txs: $adapterTxs"
    "- adapter_signal_accounts: $adapterAccounts"
    "- adapter_plugin_abi_available: $adapterPluginAbiAvailable"
    "- adapter_plugin_abi_pass: $adapterPluginAbiPass"
    "- adapter_plugin_abi_enabled: $adapterPluginAbiEnabled"
    "- adapter_plugin_abi_compatible: $adapterPluginAbiCompatible"
    "- adapter_plugin_abi_expected: $adapterPluginAbiExpected"
    "- adapter_plugin_abi_required: $adapterPluginAbiRequired"
    "- adapter_plugin_registry_available: $adapterPluginRegistryAvailable"
    "- adapter_plugin_registry_pass: $adapterPluginRegistryPass"
    "- adapter_plugin_registry_enabled: $adapterPluginRegistryEnabled"
    "- adapter_plugin_registry_matched: $adapterPluginRegistryMatched"
    "- adapter_plugin_registry_strict: $adapterPluginRegistryStrict"
    "- adapter_plugin_registry_chain_allowed: $adapterPluginRegistryChainAllowed"
    "- adapter_plugin_registry_hash_check: $adapterPluginRegistryHashCheck"
    "- adapter_plugin_registry_hash_match: $adapterPluginRegistryHashMatch"
    "- adapter_plugin_registry_abi_whitelist: $adapterPluginRegistryAbiWhitelist"
    "- adapter_plugin_registry_abi_allowed: $adapterPluginRegistryAbiAllowed"
    "- adapter_plugin_registry_expected_hash_check: $adapterPluginRegistryExpectedHashCheck"
    "- adapter_plugin_registry_expected_sha256: $adapterPluginRegistryExpectedSha256"
    "- adapter_plugin_registry_source: $adapterPluginRegistrySource"
    "- adapter_compat_matrix_ready: $adapterCompatMatrixReady"
    "- adapter_compat_matrix_has_evm: $adapterCompatHasEvm"
    "- adapter_compat_matrix_has_bnb: $adapterCompatHasBnb"
    "- adapter_registry_has_evm: $adapterRegistryHasEvm"
    "- adapter_registry_has_bnb: $adapterRegistryHasBnb"
    "- adapter_non_novovm_sample_ready: $adapterNonNovoSampleReady"
    "- adapter_consensus_binding_available: $adapterConsensusAvailable"
    "- adapter_consensus_binding_pass: $adapterConsensusPass"
    "- adapter_consensus_binding_class: $adapterConsensusClass"
    "- adapter_consensus_binding_class_code: $adapterConsensusClassCode"
    "- adapter_consensus_binding_hash: $adapterConsensusHash"
    "- adapter_plugin_abi_negative_enabled: $adapterPluginAbiNegativeEnabled"
    "- adapter_plugin_abi_negative_available: $adapterPluginAbiNegativeAvailable"
    "- adapter_plugin_abi_negative_pass: $adapterPluginAbiNegativePass"
    "- adapter_plugin_abi_negative_abi_fail: $adapterPluginAbiNegativeAbiFail"
    "- adapter_plugin_abi_negative_abi_reason_match: $adapterPluginAbiNegativeAbiReason"
    "- adapter_plugin_abi_negative_cap_fail: $adapterPluginAbiNegativeCapFail"
    "- adapter_plugin_abi_negative_cap_reason_match: $adapterPluginAbiNegativeCapReason"
    "- adapter_plugin_abi_negative_reason: $adapterPluginAbiNegativeReason"
    "- adapter_plugin_symbol_negative_enabled: $adapterPluginSymbolNegativeEnabled"
    "- adapter_plugin_symbol_negative_available: $adapterPluginSymbolNegativeAvailable"
    "- adapter_plugin_symbol_negative_pass: $adapterPluginSymbolNegativePass"
    "- adapter_plugin_symbol_negative_fail: $adapterPluginSymbolNegativeFail"
    "- adapter_plugin_symbol_negative_reason_match: $adapterPluginSymbolNegativeReasonMatch"
    "- adapter_plugin_symbol_negative_reason: $adapterPluginSymbolNegativeReason"
    "- adapter_plugin_registry_negative_enabled: $adapterPluginRegistryNegativeEnabled"
    "- adapter_plugin_registry_negative_available: $adapterPluginRegistryNegativeAvailable"
    "- adapter_plugin_registry_negative_pass: $adapterPluginRegistryNegativePass"
    "- adapter_plugin_registry_negative_hash_fail: $adapterPluginRegistryNegativeHashFail"
    "- adapter_plugin_registry_negative_hash_reason_match: $adapterPluginRegistryNegativeHashReason"
    "- adapter_plugin_registry_negative_whitelist_fail: $adapterPluginRegistryNegativeWhitelistFail"
    "- adapter_plugin_registry_negative_whitelist_reason_match: $adapterPluginRegistryNegativeWhitelistReason"
    "- adapter_plugin_registry_negative_reason: $adapterPluginRegistryNegativeReason"
    "- network_block_wire_negative_signal_enabled: $networkBlockWireNegativeEnabled"
    "- network_block_wire_negative_signal_available: $networkBlockWireNegativeAvailable"
    "- network_block_wire_negative_signal_pass: $networkBlockWireNegativePass"
    "- network_block_wire_negative_signal_expected_fail: $networkBlockWireNegativeExpectedFail"
    "- network_block_wire_negative_signal_reason_match: $networkBlockWireNegativeReasonMatch"
    "- network_block_wire_negative_signal_tamper_mode: $networkBlockWireNegativeTamperMode"
    "- network_block_wire_negative_signal_verified: $networkBlockWireNegativeVerified"
    "- network_block_wire_negative_signal_total: $networkBlockWireNegativeTotal"
    "- adapter_backend_compare_enabled: $adapterCompareEnabled"
    "- adapter_backend_compare_available: $adapterCompareAvailable"
    "- adapter_backend_compare_pass: $adapterComparePass"
    "- adapter_backend_compare_state_root_equal: $adapterCompareStateRootEqual"
    "- adapter_backend_compare_native_backend: $adapterCompareNativeBackend"
    "- adapter_backend_compare_plugin_backend: $adapterComparePluginBackend"
    "- adapter_backend_compare_native_state_root: $adapterCompareNativeRoot"
    "- adapter_backend_compare_plugin_state_root: $adapterComparePluginRoot"
    "- adapter_backend_compare_plugin_path: $adapterComparePluginPath"
    "- adapter_backend_compare_reason: $adapterCompareReason"
    "- batch_a_closure_available: $batchAAvailable"
    "- batch_a_closure_pass: $batchAPass"
    "- batch_a_demo_txs: $batchADemoTxs"
    "- batch_a_target_batches: $batchATargetBatches"
    "- batch_a_expected_min_batches: $batchAExpectedMinBatches"
    "- block_wire_signal_available: $blockWireAvailable"
    "- block_wire_signal_pass: $blockWirePass"
    "- block_wire_codec: $blockWireCodec"
    "- block_wire_bytes: $blockWireBytes"
    "- block_output_signal_available: $blockOutAvailable"
    "- block_output_signal_pass: $blockOutPass"
    "- block_output_batches: $blockOutBatches"
    "- block_output_txs: $blockOutTxs"
    "- commit_output_signal_available: $commitOutAvailable"
    "- commit_output_signal_pass: $commitOutPass"
    "- network_output_signal_available: $networkOutAvailable"
    "- network_output_signal_pass: $networkOutPass"
    "- network_closure_signal_available: $networkClosureAvailable"
    "- network_closure_signal_pass: $networkClosurePass"
    "- network_pacemaker_signal_available: $networkPacemakerAvailable"
    "- network_pacemaker_signal_pass: $networkPacemakerPass"
    "- network_process_signal_available: $networkProcessAvailable"
    "- network_process_signal_pass: $networkProcessPass"
    "- network_process_rounds: $networkProcessRounds"
    "- network_process_rounds_passed: $networkProcessRoundsPassed"
    "- network_process_round_pass_ratio: $networkProcessRoundPassRatio"
    "- network_process_node_count: $networkProcessNodeCount"
    "- network_process_total_pairs: $networkProcessTotalPairs"
    "- network_process_passed_pairs: $networkProcessPassedPairs"
    "- network_process_pass_ratio: $networkProcessPassRatio"
    "- network_process_mode: $networkProcessMode"
    "- network_directed_edges_up: $networkDirectedUp"
    "- network_directed_edges_total: $networkDirectedTotal"
    "- network_directed_edge_ratio: $networkDirectedRatio"
    "- network_block_wire_available: $networkBlockWireAvailable"
    "- network_block_wire_pass: $networkBlockWirePass"
    "- network_block_wire_pass_ratio: $networkBlockWirePassRatio"
    "- network_block_wire_verified: $networkBlockWireVerified"
    "- network_block_wire_total: $networkBlockWireTotal"
    "- network_block_wire_verified_ratio: $networkBlockWireVerifiedRatio"
    "- network_view_sync_available: $networkViewSyncAvailable"
    "- network_view_sync_pass: $networkViewSyncPass"
    "- network_view_sync_pass_ratio: $networkViewSyncPassRatio"
    "- network_new_view_available: $networkNewViewAvailable"
    "- network_new_view_pass: $networkNewViewPass"
    "- network_new_view_pass_ratio: $networkNewViewPassRatio"
    "- coordinator_signal_enabled: $coordinatorSignalEnabled"
    "- coordinator_signal_available: $coordinatorSignalAvailable"
    "- coordinator_signal_pass: $coordinatorSignalPass"
    "- coordinator_signal_reason: $coordinatorSignalReason"
    "- coordinator_negative_signal_enabled: $coordinatorNegativeEnabled"
    "- coordinator_negative_signal_available: $coordinatorNegativeAvailable"
    "- coordinator_negative_signal_pass: $coordinatorNegativePass"
    "- coordinator_negative_unknown_prepare: $coordinatorNegativeUnknownPrepare"
    "- coordinator_negative_non_participant_vote: $coordinatorNegativeNonParticipant"
    "- coordinator_negative_vote_after_decide: $coordinatorNegativeVoteAfterDecide"
    "- coordinator_negative_duplicate_tx: $coordinatorNegativeDuplicateTx"
    "- coordinator_negative_reason: $coordinatorNegativeReason"
    "- prover_contract_signal_enabled: $proverContractSignalEnabled"
    "- prover_contract_signal_available: $proverContractSignalAvailable"
    "- prover_contract_signal_pass: $proverContractSignalPass"
    "- prover_contract_signal_schema_ok: $proverContractSchemaOk"
    "- prover_contract_signal_reason_norm: $proverContractReasonNorm"
    "- prover_contract_signal_fallback_codes: $proverContractFallbackCodes"
    "- prover_contract_signal_reason: $proverContractReason"
    "- prover_contract_negative_enabled: $proverContractNegativeEnabled"
    "- prover_contract_negative_available: $proverContractNegativeAvailable"
    "- prover_contract_negative_pass: $proverContractNegativePass"
    "- prover_contract_negative_missing_formal_fields: $proverContractNegativeMissingFormal"
    "- prover_contract_negative_empty_reason_codes: $proverContractNegativeEmptyReasons"
    "- prover_contract_negative_normalization_stable: $proverContractNegativeNormStable"
    "- prover_contract_negative_reason: $proverContractNegativeReason"
    "- consensus_negative_signal_enabled: $consensusNegativeEnabled"
    "- consensus_negative_signal_available: $consensusNegativeAvailable"
    "- consensus_negative_signal_pass: $consensusNegativePass"
    "- consensus_negative_signal_invalid_signature: $consensusNegativeInvalidSignature"
    "- consensus_negative_signal_duplicate_vote: $consensusNegativeDuplicateVote"
    "- consensus_negative_signal_wrong_epoch: $consensusNegativeWrongEpoch"
    "- consensus_negative_signal_weighted_quorum: $consensusNegativeWeightedQuorum"
    "- consensus_negative_signal_equivocation: $consensusNegativeEquivocation"
    "- consensus_negative_signal_slash_execution: $consensusNegativeSlashExecution"
    "- consensus_negative_signal_slash_threshold: $consensusNegativeSlashThreshold"
    "- consensus_negative_signal_slash_observe_only: $consensusNegativeSlashObserveOnly"
    "- consensus_negative_signal_unjail_cooldown: $consensusNegativeUnjailCooldown"
    "- consensus_negative_signal_view_change: $consensusNegativeViewChange"
    "- consensus_negative_signal_fork_choice: $consensusNegativeForkChoice"
    "- consensus_negative_signal_reason: $consensusNegativeReason"
    "- zk_ready: $zkReady"
    "- zk_formal_fields_present: $zkFormalFieldsPresent"
    "- prover_ready: $proverReady"
    "- zk_contract_schema_ready: $zkContractSchemaReady"
    "- cap_has_fallback_reason: $capHasFallbackReason"
    "- cap_has_fallback_reason_codes: $capHasFallbackReasonCodes"
    "- cap_has_zk_formal_flag: $capHasZkFormalFlag"
    "- fallback_reason: $fallbackReason"
    "- fallback_reason_codes: $($fallbackReasonCodes -join ',')"
    "- msm_ready: $msmReady"
    "- baseline_ready: $baselineReady"
    "- consensus_skeleton_ready: $consensusSkeletonReady"
    "- network_skeleton_ready: $networkSkeletonReady"
    "- adapter_skeleton_ready: $adapterSkeletonReady"
    "- adapter_native_ready: $adapterNativeReady"
    "- adapter_plugin_ready: $adapterPluginReady"
    "- full_scan_f01_status: $f01Status"
    "- full_scan_f02_status: $f02Status"
    "- full_scan_f03_status: $f03Status"
    "- full_scan_f04_status: $f04Status"
    "- full_scan_f05_status: $f05Status"
    "- full_scan_f06_status: $f06Status"
    "- full_scan_f07_status: $f07Status"
    "- full_scan_f08_status: $f08Status"
    "- full_scan_f09_status: $f09Status"
    "- full_scan_f10_status: $f10Status"
    "- full_scan_f11_status: $f11Status"
    "- full_scan_f12_status: $f12Status"
    "- full_scan_f13_status: $f13Status"
    "- full_scan_f14_status: $f14Status"
    "- full_scan_f15_status: $f15Status"
    "- full_scan_f16_status: $f16Status"
    "- domain_d0_status: $domainD0Status"
    "- domain_d1_status: $domainD1Status"
    "- domain_d2_status: $domainD2Status"
    "- domain_d3_status: $domainD3Status"
    ""
    "## Domain Scan (D0~D3)"
    ""
    "| Domain | Status | Done Criteria | Auto Evidence |"
    "|---|---|---|---|"
    "| D0 AOEM Foundation Domain | $domainD0Status | F-01/F-02 = ReadyForMerge | F-01=$f01Status, F-02=$f02Status |"
    "| D1 Execution Facade Domain | $domainD1Status | F-01/F-02 = ReadyForMerge + functional_pass=True | F-01=$f01Status, F-02=$f02Status, functional_pass=$functionalPass |"
    "| D2 Protocol Core Domain | $domainD2Status | F-03/F-04 = ReadyForMerge | F-03=$f03Status, F-04=$f04Status |"
    "| D3 Consensus Network Domain | $domainD3Status | F-05/F-06/F-07/F-08 = ReadyForMerge | F-05=$f05Status, F-06=$f06Status, F-07=$f07Status, F-08=$f08Status |"
    ""
    "## Full Scan Matrix (F-01~F-16)"
    ""
    "| ID | Domain | Status | Auto Evidence |"
    "|---|---|---|---|"
    "| F-01 | AOEM execution entry | $f01Status | exec=$execSkeletonReady, bindings=$bindingsSkeletonReady, adapter_signal.pass=$adapterPass |"
    "| F-02 | AOEM runtime config | $f02Status | exec=$execSkeletonReady, variant_digest.pass=$variantDigestPass |"
    "| F-03 | Execution receipt standard | $f03Status | protocol=$protocolSkeletonReady, tx_codec=$txCodecPass, block_wire=$blockWirePass, block_out=$blockOutPass, commit_out=$commitOutPass |"
    "| F-04 | State root consistency | $f04Status | state_root.available=$stateRootAvailable, state_root.pass=$stateRootPass |"
    "| F-05 | Consensus engine | $f05Status | consensus=$consensusSkeletonReady, batch_a=$batchAPass, consensus_negative.enabled=$consensusNegativeEnabled, consensus_negative.available=$consensusNegativeAvailable, consensus_negative.pass=$consensusNegativePass, weighted_quorum=$consensusNegativeWeightedQuorum, equivocation=$consensusNegativeEquivocation, slash_execution=$consensusNegativeSlashExecution, slash_threshold=$consensusNegativeSlashThreshold, slash_observe_only=$consensusNegativeSlashObserveOnly, unjail_cooldown=$consensusNegativeUnjailCooldown, view_change=$consensusNegativeViewChange, fork_choice=$consensusNegativeForkChoice |"
    "| F-06 | Distributed coordinator | $f06Status | coordinator=$coordinatorSkeletonReady, signal_enabled=$coordinatorSignalEnabled, signal_available=$coordinatorSignalAvailable, signal_pass=$coordinatorSignalPass, negative_enabled=$coordinatorNegativeEnabled, negative_available=$coordinatorNegativeAvailable, negative_pass=$coordinatorNegativePass |"
    "| F-07 | Network layer | $f07Status | network=$networkSkeletonReady, closure=$networkClosurePass, pacemaker=$networkPacemakerPass, process=$networkProcessPass, block_wire=$networkBlockWirePass, view_sync=$networkViewSyncPass, new_view=$networkNewViewPass, block_wire_negative=$networkBlockWireNegativePass |"
    "| F-08 | Chain adapter interface | $f08Status | adapter=$adapterSkeletonReady, abi=$adapterPluginAbiPass, registry=$adapterPluginRegistryPass, consensus=$adapterConsensusPass, compare=$adapterComparePass, matrix=$adapterCompatMatrixReady, non_novovm_sample=$adapterNonNovoSampleReady, abi_negative_enabled=$adapterPluginAbiNegativeEnabled, abi_negative_pass=$adapterPluginAbiNegativePass, symbol_negative_enabled=$adapterPluginSymbolNegativeEnabled, symbol_negative_pass=$adapterPluginSymbolNegativePass, registry_negative_enabled=$adapterPluginRegistryNegativeEnabled, registry_negative_pass=$adapterPluginRegistryNegativePass |"
    "| F-09 | zk execution/aggregation | $f09Status | prover=$proverSkeletonReady, prover_signal=$proverContractSignalPass, prover_negative_enabled=$proverContractNegativeEnabled, prover_negative_available=$proverContractNegativeAvailable, prover_negative_pass=$proverContractNegativePass, schema_ok=$proverContractSchemaOk, reason_norm=$proverContractReasonNorm, zk_runtime_ready=$zkReady |"
    "| F-10 | Web3 storage service | $f10Status | storage_service=$appStorageSkeletonReady |"
    "| F-11 | Domain system | $f11Status | app_domain=$appDomainSkeletonReady |"
    "| F-12 | DeFi core | $f12Status | app_defi=$appDefiSkeletonReady |"
    "| F-13 | Multi-chain plugin capability | $f13Status | adapters_multi=$adaptersMultiSkeletonReady |"
    "| F-14 | vm-runtime split migration | $f14Status | protocol=$protocolSkeletonReady, consensus=$consensusSkeletonReady, network=$networkSkeletonReady, adapter=$adapterSkeletonReady, legacy_vm_runtime_present=$legacyVmRuntimePresent |"
    "| F-15 | AOEM ZK capability contract | $f15Status | zkvm_prove=$zkProve, zkvm_verify=$zkVerify, zk_formal_fields_present=$zkFormalFieldsPresent, schema_ready=$zkContractSchemaReady, fallback_reason=$fallbackReason |"
    "| F-16 | AOEM MSM acceleration contract | $f16Status | msm_accel=$msmAccel, msm_backend=$msmBackend |"
    ""
    "## Ledger"
    ""
    "| ID | Capability | Status | Auto Progress | Evidence Path | Updated |"
    "|---|---|---|---|---|---|"
    "| F-05 | Consensus engine (~80% verified) | $f05Status | novovm-consensus skeleton + tx_codec_signal(pass=$txCodecPass, bytes=$txCodecBytes) + mempool_admission_signal(pass=$mempoolPass, accepted=$mempoolAccepted, rejected=$mempoolRejected, fee_floor=$mempoolFeeFloor) + tx_metadata_signal(pass=$txMetaPass, accounts=$txMetaAccounts, fee=$txMetaMinFee-$txMetaMaxFee) + batch_a_closure(pass=$batchAPass, txs=$batchADemoTxs, target_batches=$batchATargetBatches, expected_min_batches=$batchAExpectedMinBatches) + block_wire_signal(pass=$blockWirePass, codec=$blockWireCodec, bytes=$blockWireBytes) + block_output_signal(pass=$blockOutPass, batches=$blockOutBatches, txs=$blockOutTxs) + commit_output_signal(pass=$commitOutPass) + $consensusNegativeSummary are available | $FunctionalJson | $today |"
    "| F-06 | Distributed coordinator | $f06Status | novovm-coordinator skeleton + $coordinatorSummary + $coordinatorNegativeSummary | $FunctionalJson | $today |"
    "| F-07 | Network layer (core-complete, production hardening pending) | $f07Status | novovm-network skeleton + network_output_signal(pass=$networkOutPass) + network_closure_signal(pass=$networkClosurePass) + network_pacemaker_signal(pass=$networkPacemakerPass) + network_process_signal(pass=$networkProcessPass, mode=$networkProcessMode, rounds=$networkProcessRoundsPassed/$networkProcessRounds, round_ratio=$networkProcessRoundPassRatio, nodes=$networkProcessNodeCount, pairs=$networkProcessPassedPairs/$networkProcessTotalPairs, ratio=$networkProcessPassRatio, directed=$networkDirectedSummary, block_wire=$networkBlockWirePass($networkBlockWireSummary), block_wire_round_ratio=$networkBlockWirePassRatio, view_sync=$networkViewSyncPass($networkViewSyncPassRatio), new_view=$networkNewViewPass($networkNewViewPassRatio)) + $networkBlockWireNegativeSummary are available | $FunctionalJson | $today |"
    "| F-08 | Chain adapter API interface | $f08Status | novovm-adapter-api + native/plugin backends + adapter_signal(pass=$adapterPass, backend=$adapterBackend, chain=$adapterChain, txs=$adapterTxs, accounts=$adapterAccounts) + $adapterPluginAbiSummary + $adapterPluginRegistrySummary + $adapterConsensusSummary + $adapterMatrixSummary + $adapterPluginAbiNegativeSummary + $adapterPluginSymbolNegativeSummary + $adapterPluginRegistryNegativeSummary + $adapterCompareSummary are available | $adapterEvidence | $today |"
    "| F-09 | zk execution/aggregation | $f09Status | novovm-prover skeleton + $proverContractSummary + $proverContractNegativeSummary + zk_runtime_ready=$zkReady | $FunctionalJson | $today |"
    "| F-15 | AOEM ZK capability contract | $f15Status | zkvm_prove=$zkProve / zkvm_verify=$zkVerify / zk_formal_fields_present=$zkFormalFieldsPresent / schema_ready=$zkContractSchemaReady / fallback_reason=$fallbackReason | $CapabilityJson | $today |"
    "| F-16 | AOEM MSM acceleration contract | $f16Status | msm_accel=$msmAccel / msm_backend=$msmBackend | $CapabilityJson | $today |"
    ""
    "## Notes"
    ""
    "- This file is auto-generated and does not replace the manual ledger."
    "- state_root consistency uses hard parity when state_root_available=true; otherwise it falls back to proxy digest."
    "- When baseline_ready=true and performance_compare_pass has a value, it can be used for regression threshold checks."
)

$outputDir = Split-Path -Path $OutputPath -Parent
New-Item -ItemType Directory -Force -Path $outputDir | Out-Null
$md -join "`n" | Set-Content -Path $OutputPath -Encoding UTF8

Write-Host "capability ledger auto snapshot generated:"
Write-Host "  $OutputPath"
