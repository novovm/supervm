param(
    [string]$RepoRoot = "D:\WorksArea\SUPERVM",
    [string]$OutputPath = "D:\WorksArea\SUPERVM\docs_CN\SVM2026-MIGRATION\NOVOVM-CAPABILITY-MIGRATION-LEDGER-AUTO-2026-03-03.md",
    [string]$FunctionalJson = "D:\WorksArea\SUPERVM\artifacts\migration\functional\functional-consistency.json",
    [string]$PerformanceJson = "D:\WorksArea\SUPERVM\artifacts\migration\performance\performance-compare.json",
    [string]$CapabilityJson = "D:\WorksArea\SUPERVM\artifacts\migration\capabilities\capability-contract-core.json",
    [string]$BaselineJson = "D:\WorksArea\SUPERVM\artifacts\migration\baseline\svm2026-baseline-core.json"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

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
$capContract = if ($capability) { $capability.contract } else { $null }
$zkProve = if ($capContract) { [bool]$capContract.zkvm_prove } else { $false }
$zkVerify = if ($capContract) { [bool]$capContract.zkvm_verify } else { $false }
$msmAccel = if ($capContract) { [bool]$capContract.msm_accel } else { $false }
$msmBackend = if ($capContract) { [string]$capContract.msm_backend } else { "" }
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
$f05Status = if ($consensusSkeletonReady -and $functionalPass) { "InProgress" } elseif ($consensusSkeletonReady) { "InProgress" } else { "NotStarted" }
$f06Status = if ($coordinatorSkeletonReady) { "InProgress" } else { "NotStarted" }
$f07ReadyForMerge = (
    $networkSkeletonReady -and
    $networkOutPass -and
    $networkClosurePass -and
    $networkProcessPass -and
    $networkBlockWirePass -and
    (
        (-not $networkBlockWireNegativeEnabled) -or
        ($networkBlockWireNegativeAvailable -and $networkBlockWireNegativePass)
    )
)
$f07Status = if ($f07ReadyForMerge) { "ReadyForMerge" } elseif ($networkSkeletonReady) { "InProgress" } else { "NotStarted" }
$f08Status = if ($adapterSkeletonReady) { "InProgress" } else { "NotStarted" }
$f09Status = if ($proverSkeletonReady -and $zkReady -and $functionalPass) { "ReadyForMerge" } elseif ($proverSkeletonReady -or $capContract) { "InProgress" } else { "NotStarted" }
$f10Status = if ($appStorageSkeletonReady) { "InProgress" } else { "NotStarted" }
$f11Status = if ($appDomainSkeletonReady) { "InProgress" } else { "NotStarted" }
$f12Status = if ($appDefiSkeletonReady) { "InProgress" } else { "NotStarted" }
$f13Status = if ($adaptersMultiSkeletonReady) { "InProgress" } else { "NotStarted" }
$f14Status = if ($protocolSkeletonReady -and $consensusSkeletonReady -and $networkSkeletonReady -and $adapterSkeletonReady) { "InProgress" } elseif ($legacyVmRuntimePresent) { "NotStarted" } else { "NotStarted" }
$f15Status = if ($zkReady -and $functionalPass) { "ReadyForMerge" } elseif ($capContract) { "InProgress" } else { "NotStarted" }
$f16Status = if ($msmReady -and $functionalPass) { "ReadyForMerge" } elseif ($capContract) { "InProgress" } else { "NotStarted" }
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
    "- state_root_proxy_pass: $stateRootPass"
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
    "- zk_ready: $zkReady"
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
    ""
    "## Full Scan Matrix (F-01~F-16)"
    ""
    "| ID | Domain | Status | Auto Evidence |"
    "|---|---|---|---|"
    "| F-01 | AOEM execution entry | $f01Status | exec=$execSkeletonReady, bindings=$bindingsSkeletonReady, adapter_signal.pass=$adapterPass |"
    "| F-02 | AOEM runtime config | $f02Status | exec=$execSkeletonReady, variant_digest.pass=$variantDigestPass |"
    "| F-03 | Execution receipt standard | $f03Status | protocol=$protocolSkeletonReady, tx_codec=$txCodecPass, block_wire=$blockWirePass, block_out=$blockOutPass, commit_out=$commitOutPass |"
    "| F-04 | State root consistency | $f04Status | state_root.available=$stateRootAvailable, state_root.pass=$stateRootPass |"
    "| F-05 | Consensus engine | $f05Status | consensus=$consensusSkeletonReady, batch_a=$batchAPass |"
    "| F-06 | Distributed coordinator | $f06Status | coordinator=$coordinatorSkeletonReady |"
    "| F-07 | Network layer | $f07Status | network=$networkSkeletonReady, process=$networkProcessPass, block_wire=$networkBlockWirePass, block_wire_negative=$networkBlockWireNegativePass |"
    "| F-08 | Chain adapter interface | $f08Status | adapter=$adapterSkeletonReady, abi=$adapterPluginAbiPass, registry=$adapterPluginRegistryPass, consensus=$adapterConsensusPass, compare=$adapterComparePass |"
    "| F-09 | zk execution/aggregation | $f09Status | prover=$proverSkeletonReady, zk_ready=$zkReady |"
    "| F-10 | Web3 storage service | $f10Status | storage_service=$appStorageSkeletonReady |"
    "| F-11 | Domain system | $f11Status | app_domain=$appDomainSkeletonReady |"
    "| F-12 | DeFi core | $f12Status | app_defi=$appDefiSkeletonReady |"
    "| F-13 | Multi-chain plugin capability | $f13Status | adapters_multi=$adaptersMultiSkeletonReady |"
    "| F-14 | vm-runtime split migration | $f14Status | protocol=$protocolSkeletonReady, consensus=$consensusSkeletonReady, network=$networkSkeletonReady, adapter=$adapterSkeletonReady, legacy_vm_runtime_present=$legacyVmRuntimePresent |"
    "| F-15 | AOEM ZK capability contract | $f15Status | zkvm_prove=$zkProve, zkvm_verify=$zkVerify |"
    "| F-16 | AOEM MSM acceleration contract | $f16Status | msm_accel=$msmAccel, msm_backend=$msmBackend |"
    ""
    "## Ledger"
    ""
    "| ID | Capability | Status | Auto Progress | Evidence Path | Updated |"
    "|---|---|---|---|---|---|"
    "| F-05 | Consensus engine (~80% verified) | $f05Status | novovm-consensus skeleton + tx_codec_signal(pass=$txCodecPass, bytes=$txCodecBytes) + mempool_admission_signal(pass=$mempoolPass, accepted=$mempoolAccepted, rejected=$mempoolRejected, fee_floor=$mempoolFeeFloor) + tx_metadata_signal(pass=$txMetaPass, accounts=$txMetaAccounts, fee=$txMetaMinFee-$txMetaMaxFee) + batch_a_closure(pass=$batchAPass, txs=$batchADemoTxs, target_batches=$batchATargetBatches, expected_min_batches=$batchAExpectedMinBatches) + block_wire_signal(pass=$blockWirePass, codec=$blockWireCodec, bytes=$blockWireBytes) + block_output_signal(pass=$blockOutPass, batches=$blockOutBatches, txs=$blockOutTxs) + commit_output_signal(pass=$commitOutPass) are available | $FunctionalJson | $today |"
    "| F-07 | Network layer (core-complete, production hardening pending) | $f07Status | novovm-network skeleton + network_output_signal(pass=$networkOutPass) + network_closure_signal(pass=$networkClosurePass) + network_process_signal(pass=$networkProcessPass, mode=$networkProcessMode, rounds=$networkProcessRoundsPassed/$networkProcessRounds, round_ratio=$networkProcessRoundPassRatio, nodes=$networkProcessNodeCount, pairs=$networkProcessPassedPairs/$networkProcessTotalPairs, ratio=$networkProcessPassRatio, directed=$networkDirectedSummary, block_wire=$networkBlockWirePass($networkBlockWireSummary), block_wire_round_ratio=$networkBlockWirePassRatio) + $networkBlockWireNegativeSummary are available | $FunctionalJson | $today |"
    "| F-08 | Chain adapter API interface | $f08Status | novovm-adapter-api + native/plugin backends + adapter_signal(pass=$adapterPass, backend=$adapterBackend, chain=$adapterChain, txs=$adapterTxs, accounts=$adapterAccounts) + $adapterPluginAbiSummary + $adapterPluginRegistrySummary + $adapterConsensusSummary + $adapterPluginAbiNegativeSummary + $adapterPluginSymbolNegativeSummary + $adapterPluginRegistryNegativeSummary + $adapterCompareSummary are available | $adapterEvidence | $today |"
    "| F-15 | AOEM ZK capability contract | $f15Status | zkvm_prove=$zkProve / zkvm_verify=$zkVerify | $CapabilityJson | $today |"
    "| F-16 | AOEM MSM acceleration contract | $f16Status | msm_accel=$msmAccel / msm_backend=$msmBackend | $CapabilityJson | $today |"
    ""
    "## Notes"
    ""
    "- This file is auto-generated and does not replace the manual ledger."
    "- When state_root_available=false, proxy_digest consistency is used as a temporary gate."
    "- When baseline_ready=true and performance_compare_pass has a value, it can be used for regression threshold checks."
)

$outputDir = Split-Path -Path $OutputPath -Parent
New-Item -ItemType Directory -Force -Path $outputDir | Out-Null
$md -join "`n" | Set-Content -Path $OutputPath -Encoding UTF8

Write-Host "capability ledger auto snapshot generated:"
Write-Host "  $OutputPath"
