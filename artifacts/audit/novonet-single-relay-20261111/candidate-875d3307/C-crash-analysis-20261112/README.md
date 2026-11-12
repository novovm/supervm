# C crash analysis: original dumps recovered; matching875 symbols required from A

This continues the current C failure investigation. No recovery/restart, program
update, observer/capture invocation, object send, database/key/config mutation,
D operation or Full CI was performed. C is not made READY by this analysis.

## Verified evidence

- Found original LocalService dumps for875 PID31296 and3228. Copied once for
  local offline analysis and verified original/copy SHA256 equality. Sizes and
  hashes are in dump-copy-public.json. Dumps, raw WER reports and debugger memory
  logs remain ONLY under local target/c-crash-analysis-20261112T004637Z and the
  original protected locations; none is uploaded.
- Read-only inventory found5 node dumps and30 matching-name WER reports; only
  the two current875 dumps were analyzed. Inventory errors0. Historic reports
  were not used to attribute the current issue and are not uploaded.
- Matching package executable SHA2569174f29bad4bf1feb9acfbd5d2b5ed36bb47ff2a2cd1519d7742a2e1b944dee7.
  Both dump CodeView identities and packaged executable debug directory agree:
  novovm_node.pdb GUID C94A5E99-4CFF-4094-8B63-0663EF0F9C21, age1.
- Existing LLDB21.1.7 loaded both original minidumps and the matching875 image.
  Corrected final debugger commands exited0 for each dump. Local executable and
  package contain no matching PDB at the checked current locations; llvm-symbolizer
  explicitly rejects resolution because novovm_node.pdb is absent.

## Findings, with limits

PID31296 (Nov11 21:27 Beijing): access violation at novovm-node.exe+0x65f59d.
Instruction `movaps xmm6,[rbp+0x20]`; saved register-derived operand address has
modulo16=8. This is consistent with an alignment fault, but does NOT establish
why the alignment is wrong or which source function/ABI boundary caused it.
Only the fault frame could be unwound; no complete caller chain is claimed.

PID3228 (Nov12 04:13 Beijing): write violation at VCRUNTIME140.dll+0x1126b.
Instruction writes2 bytes through RAX; destination matches the exception address.
Unwound frames include node+0x1e47a33, +0x1e4b0f9, +0x1e4d281,
+0x1e47d3f, +0x96490a, +0x9f1ef4, +0xb752b2, +0x7618af,
+0x777fb9, +0x17d5e5, +0x8ad16, +0x1d8d57, +0x1da0f5, +0x1e34e88.
Complete module-relative frame list is in crash-summary-public.json.
LLDB's nearest exported label (__NLG_Return2+offset) is NOT accepted as the actual
routine name. A fault in the runtime does not prove a defective DLL installation.

These are observed instruction/context and unwind results, not a root-cause fix.
Equal exception codes do not prove the same cause. No blame is assigned to AOEM,
VPN, hardware, VC runtime installation or Windows scheduling without further proof.
The absence of AOEM frames on the recovered exception stack does not exclude a
prior memory-corruption source elsewhere.

## Action requested from validator-A (shared native owner)

1. Locate ORIGINAL875 novovm_node.pdb with exact GUID above and age1, from the
   retained candidate build/artifact. Supply its path/hash via Git handoff and
   a local transfer method; no dump upload is required. A may alternatively
   resolve the published frame RVAs against that matching original symbol file.
2. Symbolize both the fault RVA0x65f59d and the second stack above, identify source
   functions/lines and owners. Inspect the relevant memory lifetime/ABI/alignment
   path according to the actual resolved source, not a generic runtime reinstall.
3. Do not substitute a newly rebuilt/different PDB as original875 evidence. If
   original symbols are missing, say so. A newly instrumented candidate requires
   separate qualification/deployment; do not silently replace this node/package.

C can continue local symbolization once matching symbols are available. Preserve
C's existing database, custody, journal and crash logs. Current observer instruction
still requires separate coordination before any recovery or new standby window.

## Tools and privacy

The installed liblldb.dll required python310.dll, which was missing. A verified
[official embeddable package](https://www.python.org/downloads/release/python-31011/)
was extracted ONLY into the ignored local analysis directory; DLL Authenticode
signer is Python Software Foundation. No global PATH, Python installation, node
runtime, service, registry, network or firewall change. The dependency was used
only by local offline debugger processes, not by NOVOVM; no tool binaries uploaded.
No symbol-server upload or remote dump analysis was used.

Raw logs preserve the initial unsupported `image list -v` diagnostic command and
its later correction (`image list -u -f`). Published summaries omit memory contents
and absolute virtual addresses. Raw WER files can contain host information and
are not published. Public files and scripts are byte-preserved by attributes;
SHA256.json covers them, excluding itself.
