# AOEM Confidential Transfer v1

`confidential_transfer_v1` is a host integration profile over the existing AOEM
RingCT capability. It is not a new proof system, not a new Runtime Canon path,
and not a new public FFI ABI.

```text
host application
  -> aoem_ringct_prove_v1
  -> aoem_privacy_execute_v1
  -> RingCT transaction payload / verification status
```

## Product Meaning

Use this profile when a SUPERVM host wants an amount-hiding confidential
transfer path backed by AOEM RingCT.

```text
confidential_transfer_v1
  semantic layer: SDK / host profile
  underlying capability: AOEM RingCT
  generation symbol: aoem_ringct_prove_v1
  validation/admission symbol: aoem_privacy_execute_v1
```

The sample host checks that the generated transaction payload verifies through
AOEM privacy-native execution and that the raw sample amount is not emitted in
the transaction JSON.

## Host Example

Source:

```text
host-integration/embedded_confidential_transfer_host.c
examples/hosted_confidential_transfer_smoke.c
```

Windows:

```powershell
clang -std=c11 -Wall -Wextra -I aoem\windows\include `
  aoem\host-integration\embedded_confidential_transfer_host.c `
  -o $env:TEMP\supervm_confidential_transfer.exe

& $env:TEMP\supervm_confidential_transfer.exe `
  aoem\windows\core\bin\aoem_ffi.dll
```

Linux:

```bash
cc -std=c11 -Wall -Wextra -I aoem/linux/include \
  aoem/host-integration/embedded_confidential_transfer_host.c \
  -ldl -o /tmp/supervm_confidential_transfer

LD_LIBRARY_PATH=aoem/linux/core/bin \
  /tmp/supervm_confidential_transfer \
  aoem/linux/core/bin/libaoem_ffi.so
```

Fast host wiring probe output:

```text
SUPERVM_AOEM_CONFIDENTIAL_TRANSFER_HOST|profile=confidential_transfer_v1|ringct_symbols=ok|prove=not_run|privacy_execute=not_run|verify=not_run|mode=host_wiring_probe|...|failures=0
```

Full RingCT generation/verification mode:

```powershell
& $env:TEMP\supervm_confidential_transfer.exe `
  aoem\windows\core\bin\aoem_ffi.dll `
  --run-prove
```

Expected full-path output:

```text
SUPERVM_AOEM_CONFIDENTIAL_TRANSFER_HOST|profile=confidential_transfer_v1|ringct=ok|prove=ok|privacy_execute=ok|verify=ok|amount_hidden=ok|...|failures=0
```

`--run-prove` creates a 64-bit RingCT range proof and is intentionally not the
default smoke path.

## Boundaries

```text
no new public FFI ABI
Runtime Canon unchanged
proof worker default unchanged
Graph OS untouched
dedicated LR untouched
not a new ZK circuit
not a generic arbitrary confidential transaction platform claim
not a performance-ready claim
```

`confidential_transfer_v1` is the product semantics for using existing AOEM
RingCT in a host. The lower-level RingCT symbols remain the source of truth.
