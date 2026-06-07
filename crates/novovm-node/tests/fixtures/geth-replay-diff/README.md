# Geth Replay Diff Fixtures

These fixtures pin a minimal real go-ethereum `ethapi/testdata` block and receipt shape for v2b observable replay diff.

The current gate recomputes the real geth legacy transaction trie root from the full transaction object, compares it with geth's block `transactionsRoot`, then contrasts the same block-level fields with SUPERVM's canonical projection output.

Run:

```powershell
cargo test -p novovm-node evm_protocol_observable_equivalence_geth_real_block_diff_gate_v2b -- --nocapture
```

Current expected boundary: SUPERVM matches receipt/gas/log/state projection fields for this fixture, but `transactionsRoot` remains a reported gap until raw transaction RLP is carried into the canonical block projection path.
