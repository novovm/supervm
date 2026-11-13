# C first-fault diagnosis — prepared, not a crash fix

C owns this source investigation. The user authorized bounded controlled
diagnostics after PID37796 crashed. Original a254 program, AOEM966, original
database/identity/network and old failures are retained. A/B/D are not operated.

## Method and bounds

The previous minidump identifies the failing read, not the earlier memory write.
This round uses Microsoft's full PageHeap and CDB, not repeated uninstrumented
restarts. PageHeap places inaccessible pages after allocations to stop on an
out-of-bounds access and can expose freed-allocation access. It does not cover
every stack overwrite, intra-allocation error or source of corruption; a clean
run will not establish a fix.

Before any node launch, the same LocalService owner must pass a clean HeapAlloc
control and catch a deliberate one-byte overrun in a separate fixture. The
fixture is explicitly synthetic diagnostic data, not a network object or result.
Both control paths and their raw output are retained. Failed controls stop the
task before opening the node database.

- One new diagnostic task, no restart/retry policy.
- Original node SHA256 `1758e4a79dd34d1688d8f7bf4b60f46ed9ced518186f42fdb16baf04c0d0c46d`.
- Original AOEM966 SHA256 `5743f5875131330f8c8fec58cdc2a85aa39dc6e9689358609a5f0a09a01c0d52`.
- Start is `existing`; no initialize, key generation, database replacement,
  manual object send, relay observer or production timeout change.
- Node run bound30minutes, debugger hard bound32minutes; two fixture runs each
  at most60seconds. Task maximum35minutes. No lifetime limit change outside this run.
- Stop at6GiB target private memory or less than10GiB free on D. A resource-bound
  stop is inconclusive, not a fixed node. Only the matching diagnostic debugger
  and its owned child may be terminated.
- Reject existing IFEO settings for either diagnostic image name. Only newly
  added PageHeap flags are removed, immediately after initial node verification
  and in the error cleanup path. No global heap, VPN, network or firewall change.
- Private dumps/logs/config are stored locally under a restricted ACL; no raw
  dump, key or database is published. Only curated public summaries go to Git.
- First AV/verifier stop records native context and one full dump, then exits.
  Debugger exit0 is never called node success. Normal expiry also is not a fix.

## Preparation actually completed

SDK debugging tools downloaded through the official Microsoft SDK installer
with only the debugger layout feature. Layout exit0; x64 administrative
extraction exit0. This extracted tools locally, not a runtime installation.
Microsoft signatures on CDB/GFlags/engines are checked again before use.
The normal diagnostic command launcher was exercised against the clean fixture
without PageHeap, including the empty argument used for the node, exit0.
13 static contract checks and PowerShell AST parsing passed.

Live PageHeap controls and node diagnostics are **pending Windows UAC** at this
checkpoint. C remains **NOT READY**; A must not send a relay object. Follow-up
results must distinguish controls, actual instrumentation, first violation,
resource/time bound and fix validation. No product code change is asserted.

Sources: [Microsoft PageHeap](https://learn.microsoft.com/en-us/windows-hardware/drivers/debugger/gflags-and-pageheap),
[image-specific GFlags](https://learn.microsoft.com/en-us/windows-hardware/drivers/debugger/gflags-overview),
[SDK debugger tools](https://learn.microsoft.com/en-us/windows-hardware/drivers/debugger/debugger-download-tools).

## Attempt01 / corrected v2 preflight

The LocalService clean control exited0. The owner then failed closed because
`!gflag` needs NTDLL type symbols that are unavailable (Microsoft symbol server
returned404 for the actual GUID). No node was started. Both temporary IFEO keys
were removed; original failures and this tool failure are preserved.

The actual Windows loader already emitted `page heap enabled with flags 0x3`.
V2 checks that runtime event instead of the symbol-dependent display, plus the
mandatory deliberate-overrun control. The check passed against the actual enabled
control log and correctly failed against the earlier non-PageHeap log. This is
not accepting mere registry configuration as proof of instrumentation. V2 uses
new evidence/task names and still requires a new OS UAC; node diagnosis pending.
