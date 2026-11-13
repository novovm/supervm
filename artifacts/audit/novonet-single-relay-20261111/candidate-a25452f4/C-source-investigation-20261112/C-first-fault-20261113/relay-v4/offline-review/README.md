# C first-chance dump: offline follow-up

The original277,824,196-byte full dump remains local. This follow-up did not
restart a node, read private keys, modify the DB or operate another host.

## Current trace and limits

At the first-chance fault, ntdll's unwind path loaded a pointer value1 from a
stack parameter field and attempted a write to0xb9. Read-only comparison of
the captured fault instructions, a selected node allocation function region
and its16-byte unwind record matched the corresponding disk image bytes.
The node unwind descriptor belongs to RVA0x2432750..0x24327bc. Selected
matches do not prove whole-image integrity or correctness of all metadata.

Dump-time register/argument inspection narrows the unresolved question: the
live RSI points to a local-record address different from the pointer retained
in the relevant argument stack slot. A dump does not show what that slot or
register held earlier, or which instruction/thread changed it. This is not
enough to label a first corrupting writer, compiler defect, Windows defect or
hardware failure. No code patch is being invented from that uncertainty.

The fault is in verifier/heap allocation stack capture, not a demonstrated
out-of-bounds write caught at an application's allocation boundary. Its common
cause with prior String-copy failures remains unproven. Exact a254 node and
installed AOEM966 PDBs are still absent from C's examined symbol files; the
older875 PDB was not substituted as exact symbols.

## Isolated Windows stack-capture experiment

A separate fixture reuses the original bounded profile/JSON source and its
dependency lock. A System allocator wrapper optionally invokes the documented
[RtlCaptureStackBackTrace](https://learn.microsoft.com/en-us/windows/win32/api/winnt/nf-winnt-rtlcapturestackbacktrace)
before allocation/reallocation. It uses neither AOEM nor PageHeap, and opens
no node database or original identities. Both debugger runs are bounded to75s;
the actual clean workload is5s and tracing workload30s. Their logs stay local.

- Control without tracing: target exit0; no AV.
- Tracing:26,114,540captures,0empty results; target exit0; no AV.

This negative experiment does not reproduce the exact a254 binary/stack or
the PageHeap implementation. It neither fixes nor clears the native crash.
The unused initial dependency resolution was retained locally, then replaced
with the original fixture lock before either actual run. Source hashes, lock,
main and public results are provided; copied profile modules can be obtained
from the recorded original source. No executable/dump is published.

## End state

Fresh read-only snapshot at09:42:41Z: no node/debugger/C listeners and no
residual PageHeap image settings. Task-result read was denied to the normal
collector and is explicitly not inferred. The stop-helper UAC had cancelled;
it never stopped anything, and no second prompt/action was needed after the
first-chance capture ended the debuggee. Actual normal node exit remains null;
debugger exit0 only indicates capture completion.

The publisher completeness fix and diagnostic timestamp fix are in1f4bd204;
full node check, strict Clippy and scoped regressions passed. No new program
was installed. Original DB, identities and earlier failures remain. C keeps
ownership of the native pointer-write investigation. **NOT relay READY; do not
restart observation or send a double-relay object from this report.**
