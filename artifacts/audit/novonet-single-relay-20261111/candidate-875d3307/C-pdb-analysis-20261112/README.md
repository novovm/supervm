# C original875 PDB verified and fault contexts correlated

Original PDB supplied via E:/区块链四机通讯/novovm_node.pdb:17,281,024 bytes,
SHA256b46bc1763c06c7370733763391d56e083b2e52cc8dcd370a377be2f7928dac38,
GUID C94A5E99-4CFF-4094-8B63-0663EF0F9C21, age1. All match executable and both
original dumps. PDB/executable copies and all memory inspection remain in ignored
local target/c-crash-analysis-20261112T004637Z/pdb-analysis. No PDB/dump/binary
uploaded. Original package, stopped node, database, custody and D were not changed.

## PID31296: indexmap insert frame invariant, not merely a startup alignment claim

The exact PDB public function starts at RVA0x65f280; PE runtime-function table
covers0x65f280..0x65f69a. Fault0x65f59d is indexmap::inner::Core::insert_full,
restoring XMM6 with movaps [rbp+0x20]. Prologue saves8 registers, subtracts0xd8
from RSP, sets RBP=RSP+0x80, and performs aligned XMM saves at the same locations.
At the fault, captured RBP-RSP is -0x18, not +0x80; operand alignment modulo16=8.
Disassembly has no other ordinary RBP assignment before the fault epilogue.
Thus the observed register/frame relation is inconsistent with the normal path.
The dump does NOT establish which earlier code altered it or whether control flow
entered abnormally. Simply blaming initial function-entry alignment is premature:
the prologue itself also uses aligned saves. Only one exception frame unwinds.

## PID3228: invalid descriptor observed before the runtime copy

Original exception is a2-byte write. The exact node caller begins at RVA0x1e47a00:
it loads capacity from writer+0, length from writer+0x10, buffer from writer+8,
checks capacity-length, and calls the runtime copy at RVA0x1e47a2e. Original
unwound RDI selects this descriptor; captured RBX equals its length field and
RSI=2. The descriptor's length is504045167760 bytes (see machine summary for
canonical conversion); its buffer+length equals the exception's destination,
which LLDB classifies in a region with no access. Absolute addresses/content stay
local. The invalid length/pointer arithmetic is already present at the caller;
this does not establish whether the descriptor was overwritten, misidentified,
or reached through a wrong dispatch/lifetime path.

Exact-PDB lookup of return PCs minus1 recovers String::write_str / push_str /
Vec::append_elements, u8 LowerHex, fmt::write / format_inner, iterator collection,
RpcLifecycle::status and observe_tick. This supports the reported Status snapshot
crash path, not proof that formatting caused the original damage. Public symbol
lookup also returns profile::elapsed for RVA0x7618ae, but that is0x50de beyond its
public start: do NOT treat that fallback label as a proved precise source call.
Most application symbols have no source line records; absent lines remain absent.

## Reproduction of symbolization and next shared-owner work

LLDB21.1.7 reads the existing dumps and original code/contexts. Its explicit
`target symbols add` rejected this PDB despite verified identity; these failed
attempts remain local. llvm-symbolizer successfully uses the same PDB adjacent
to the verified local executable to resolve original module RVAs. This report
uses that cross-tool correlation; it does not pretend LLDB locals were decoded.
Return-address lookups subtract1; the actual fault RVA does not. symbols-public.json
preserves all returned inline symbols and genuine source line information.

A native owner: inspect the String writer/formatter object lifetime and dispatch
at these exact callsites, alongside possible nonvolatile frame-register corruption.
Use the matching original PDB for further range/type lookup; avoid attributing
all faults to a runtime installation or replacing safe String formatting solely
to suppress the observed stack. A controlled instrumented reproducer may be
needed if the original mini dumps cannot expose the earlier corrupting operation.
Two crashes sharing a root cause remains unproved. Any candidate fix, restart or
new capture should have its own explicit handoff; no such action occurred here.
