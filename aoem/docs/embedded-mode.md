# Embedded Host Mode

Embedded host mode is the recommended production integration shape.

The host loads the AOEM dynamic library and calls the existing wire entry:

```text
aoem_execute_ops_wire_v1
```

Proof results are read through:

```text
aoem_state_read_v1
```

## Reference Hosts

```text
host-integration/embedded_proof_host.c
  Minimal proof host reference.

host-integration/embedded_batch_proof_host.c
  Batch proof host reference.

host-integration/embedded_asset_lifecycle_host.c
  Resident asset lifecycle host reference.
```

## Windows Compile Example

```powershell
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

## Host Responsibilities

The host owns:

```text
job admission
business API
queueing
storage
authentication
deployment
retry policy
```

AOEM owns:

```text
wire_v1 proof workload execution
resident asset lifecycle workload
state readback
proof envelope generation
```

No telemetry, scheduler, queue system, Graph OS route, or new public FFI ABI is
introduced by this package.
