# Geth Replay Diff Fixtures

These fixtures pin a minimal real go-ethereum `ethapi/testdata` block and receipt shape for v2b observable replay diff.

The current gate recomputes the real geth legacy transaction trie root from the full transaction object, carries that raw transaction RLP into SUPERVM's canonical batch projection, and then contrasts the same block-level fields with SUPERVM's canonical output.

Run:

```powershell
cargo test -p novovm-node evm_protocol_observable_equivalence_geth_real_block_diff_gate_v2b -- --nocapture
```

Current expected boundary: SUPERVM matches `number`, `gasUsed`, `logsBloom`, `transactionsRoot`, `receiptsRoot`, and `stateRoot` for this fixture when raw transaction RLP is present. If canonical data does not carry raw transaction RLP, SUPERVM intentionally falls back to its existing projection root.
