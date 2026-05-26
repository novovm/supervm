# Host Integration References

These files are host-side references for embedding AOEM Proof Engine directly
inside an application process.

```text
embedded_proof_host.c
  single proof host reference

embedded_batch_proof_host.c
  batch proof host reference

embedded_asset_lifecycle_host.c
  resident asset lifecycle host reference

embedded_confidential_transfer_host.c
  confidential_transfer_v1 host reference over existing AOEM RingCT
```

They use the existing public entry and state read path:

```text
aoem_execute_ops_wire_v1
aoem_state_read_v1
```

`embedded_confidential_transfer_host.c` intentionally uses the existing RingCT
and privacy-native FFI symbols:

```text
aoem_ringct_prove_v1
aoem_privacy_execute_v1
```

It is an SDK/host product profile for confidential transfer integration, not a
new Runtime Canon path and not a new proof worker default task.

No new public FFI ABI, compute op, Runtime Canon path, Graph OS path, or
dedicated LR path is introduced.
