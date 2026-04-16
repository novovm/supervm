# Geth Parity Samples (`*.sample.json`)

This directory is the default source for batch parity gate samples used by:

`mainline_query::tests::eth_end_to_end_geth_sample_batch_parity_report_from_files_v1`

Only files matching `*.sample.json` are loaded as sample definitions.
You can keep auxiliary files (for example `store.json`) in the same directory without being treated as sample descriptors.

## Sample Schema

- `schema`: `supervm-e2e-geth-parity-sample/v1`
- `name`: unique sample name
- input source:
  - `scenario` (built-in scenario, for fast baseline checks), or
  - `store_path` (external canonical store JSON path, relative to sample file)
- optional `store_format`:
  - `geth-export-receipt/v1` (parse geth receipt/export JSON directly)
- `expected`: expected parity assertions for:
  - `block`
  - `receipts`
  - `logs_canonical`
  - `logs_noncanonical`
  - `typed_failures`
- optional: `reorg_hash_hex` (32-byte hex used for non-canonical log/ownership check)

## Built-in Scenarios (current)

- `adapter_e2e_default_v1`
- `adapter_e2e_geth_create_contract_access_list_v1`
- `adapter_e2e_geth_dynamic_fee_failure_v1`
- `adapter_e2e_geth_blob_tx_success_v1`
- `adapter_e2e_geth_legacy_with_logs_v1`
- `adapter_e2e_geth_deploy_success_with_logs_v1`
- `adapter_e2e_geth_deploy_fail_revert_v1`
- `adapter_e2e_geth_blob_tx_failure_v1`
- `adapter_e2e_geth_reorg_dual_tx_v1`
- `adapter_e2e_geth_type2_priority_over_max_fee_v1`
- `adapter_e2e_geth_type2_intrinsic_gas_low_v1`

These built-in scenarios are used by the default sample files and map to parity expectations derived from real geth receipt/log fixtures.

## Current Fixture Mapping (go-ethereum)

- `geth-create-contract-with-access-list.sample.json`
  - source: `go-ethereum/internal/ethapi/testdata/eth_getTransactionReceipt-create-contract-with-access-list.json`
- `geth-dynamic-tx-failure.sample.json`
  - source: `go-ethereum/internal/ethapi/testdata/eth_getTransactionReceipt-dynamic-tx-with-logs.json`
- `geth-blob-tx-success.sample.json`
  - source: `go-ethereum/internal/ethapi/testdata/eth_getTransactionReceipt-blob-tx.json`
- `geth-legacy-with-logs.sample.json`
  - source: `go-ethereum/internal/ethapi/testdata/eth_getTransactionReceipt-with-logs.json`
- `geth-deploy-success-with-logs.sample.json`
  - source baseline: `go-ethereum/internal/ethapi/testdata/eth_getTransactionReceipt-create-contract-tx.json` + log semantics stress
- `geth-deploy-fail-revert.sample.json`
  - source baseline: dynamic-failure/revert semantics stress sample
- `geth-blob-tx-failure.sample.json`
  - source baseline: blob tx semantics stress sample (typed tx failure path)
- `geth-reorg-dual-tx.sample.json`
  - source baseline: canonical/non-canonical ownership + removed-flip stress sample
- `geth-type2-priority-over-max-fee.sample.json`
  - source baseline: EIP-1559 fee edge (`max_priority_fee > max_fee`) failure-path stress
- `geth-type2-intrinsic-gas-low.sample.json`
  - source baseline: intrinsic gas lower-bound failure-path stress

## Run Batch Parity Report

Default fixture directory:

```powershell
cargo test -p novovm-node mainline_query::tests::eth_end_to_end_geth_sample_batch_parity_report_from_files_v1 -- --nocapture
```

External sample directory:

```powershell
$env:NOVOVM_GETH_PARITY_SAMPLE_DIR="D:\path\to\geth-parity"
cargo test -p novovm-node mainline_query::tests::eth_end_to_end_geth_sample_batch_parity_report_from_files_v1 -- --nocapture
```

External geth export samples with placeholder path:

```powershell
$env:NOVOVM_GETH_REPO_ROOT="D:\WEB3_AI\go-ethereum"
$env:NOVOVM_GETH_PARITY_SAMPLE_DIR="D:\WEB3_AI\SUPERVM\crates\novovm-node\tests\fixtures\geth-parity-external"
cargo test -p novovm-node mainline_query::tests::eth_end_to_end_geth_sample_batch_parity_report_from_files_v1 -- --nocapture
```

Gate path (`supervm-mainline-gate`) already executes this batch parity test and fails on any mismatch.
