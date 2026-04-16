# External Geth Export Samples (`store_path`)

This directory contains parity sample descriptors that directly reference
real geth export files via `store_path`.

## Requirements

Set `NOVOVM_GETH_REPO_ROOT` to your local go-ethereum checkout root.

Example (Windows PowerShell):

```powershell
$env:NOVOVM_GETH_REPO_ROOT="D:\WEB3_AI\go-ethereum"
$env:NOVOVM_GETH_PARITY_SAMPLE_DIR="D:\WEB3_AI\SUPERVM\crates\novovm-node\tests\fixtures\geth-parity-external"
cargo test -p novovm-node mainline_query::tests::eth_end_to_end_geth_sample_batch_parity_report_from_files_v1 -- --nocapture
```

## Notes

- `store_format: "geth-export-receipt/v1"` tells loader to parse receipt-style geth JSON.
- `store_format: "geth-export-receipts/v1"` parses receipt-array exports (for example `eth_getBlockReceipts-*`).
- `store_path` supports `${NOVOVM_GETH_REPO_ROOT}` placeholder.
- Batch report schema remains `supervm-e2e-geth-parity-batch-report/v1`.

## Auto-generate Sample Descriptors

Use the Rust-native sync tool to regenerate `*.sample.json` directly from geth exports:

```powershell
$env:NOVOVM_GETH_REPO_ROOT="D:\WEB3_AI\go-ethereum"
cargo run -p novovm-node --bin supervm-mainline-geth-sample-sync --
```

Optional flags:

- `--source-dir <path>`: custom geth export dir.
- `--output-dir <path>`: custom sample descriptor output dir.
- `--chain-id <u64>`: override default chain id (`1`).
- `--dry-run`: report changes without writing files.
