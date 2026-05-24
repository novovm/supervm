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
```

They use the existing public entry and state read path:

```text
aoem_execute_ops_wire_v1
aoem_state_read_v1
```

No new public FFI ABI, compute op, Runtime Canon path, Graph OS path, or
dedicated LR path is introduced.
