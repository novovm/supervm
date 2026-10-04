# Embedded Host Mode

Embedded host mode is the recommended integration shape; it does not qualify
these diagnostic profiles as production ZK. For NOVOVM, the original
`novovm-node -> novovm-exec -> aoem-bindings -> AOEM` remains the product route.
These reference programs are SDK checks, not a separate product proof service.

Profiles 1/2 are public diagnostics (`scope=not_zk`); profile 3 is retired and
must be rejected without proof output. See the
[security correction](proof-engine-v1.0-contract.md#security-correction-2026-10-04).

The host loads the AOEM dynamic library and calls the existing wire entry:

```text
aoem_execute_ops_wire_v1
```

Diagnostic results are read through:

```text
aoem_state_read_v1
```

## Reference Hosts

```text
host-integration/embedded_proof_host.c
  Minimal public diagnostic host reference.

host-integration/embedded_batch_proof_host.c
  Batch public diagnostic and retired-profile rejection reference.

host-integration/embedded_asset_lifecycle_host.c
  Resident public asset lifecycle host reference (use --asset-lifecycle).
```

## Windows Compile Example

```powershell
New-Item -ItemType Directory -Force tmp | Out-Null
clang -std=c11 -Wall -Wextra `
  -I aoem\windows\include `
  aoem\host-integration\embedded_proof_host.c `
  -o tmp\embedded_proof_host.exe

tmp\embedded_proof_host.exe `
  aoem\windows\core\bin\aoem_ffi.dll
```

## Linux Compile Example

```bash
cc -std=c11 -Wall -Wextra \
  -I aoem/linux/include \
  aoem/host-integration/embedded_proof_host.c \
  -o /tmp/embedded_proof_host

LD_LIBRARY_PATH=aoem/linux/core/bin \
  /tmp/embedded_proof_host \
  aoem/linux/core/bin/libaoem_ffi.so
```

## macOS

```text
macOS runtime artifacts are pending fresh FULLMAX rebuild and are not bundled in
this SUPERVM package. Do not use stale .dylib files for audit or host
integration claims.
```

## Result Boundary

Successful diagnostics explicitly report
`verification_scope=envelope_integrity_only_not_zk`,
`envelope_integrity_verified=true`, `cryptographic_proof_verified=false`,
`proof_verified=false` and `verify_status.accepted=false`. Neither a zero return
code, a generated envelope nor GPU work proves a private/business relation.
Private profile 3 must return an error and no new output; never reuse old state
as the result of a failed call. Check request identity before reading results.

## Host Responsibilities

The host owns:

```text
job admission
business API
queueing
host application result retention (not NOVOVM authoritative state mutation)
authentication
deployment
retry policy
```

AOEM owns:

```text
wire_v1 supported diagnostic workload execution
resident asset lifecycle workload
state readback
public diagnostic envelope generation
```

No telemetry, scheduler, queue system, Graph OS route, or new public FFI ABI is
introduced by this package.
