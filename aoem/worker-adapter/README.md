# Worker Adapter Reference

`aoem_proof_worker.c` is an optional reference host adapter. It reads JSONL proof
jobs, calls the AOEM dynamic library through `aoem_execute_ops_wire_v1`, reads
proof outputs through `aoem_state_read_v1`, and writes JSONL proof results.

This adapter is useful for SDK trials, CI acceptance, and sidecar-style
integration while a host team migrates to direct embedded use.

It is not the AOEM runtime itself and not the required production deployment
model.

Example job files:

```text
examples/jobs.merkle.jsonl
examples/jobs.zk_merkle.jsonl
examples/jobs.mixed.jsonl
```
