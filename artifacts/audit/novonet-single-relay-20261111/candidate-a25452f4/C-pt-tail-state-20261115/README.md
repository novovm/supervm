# C PT tail-state provenance: existing evidence cannot distinguish hardware layers

This round checks existing evidence and the interface contract, without a new
capture, target restart, driver, kernel monitor or production change.

## Correction to the investigation premise

The last CSwitch is present: target TID 32528 leaves CPU 9 at QPC
3074711522469, 7.1 us after the handler timestamp. What is missing is a PT
envelope at that exact QPC/CPU. Do not describe this as missing switch-out
sideband. Exact associations elsewhere identify incoming TIDs, which cannot
automatically assign their bytes to the preceding outgoing thread.

## Fields that must not be substituted

The existing decoder reads the exported 32-byte IPT header as
`QPC, PID, TID, options, bufferSize, writePosition`. Its structural checks and
successful packet decoding do not turn these fields into CPU MSR snapshots.
In particular, `options=0x3000001` is not established as IA32_RTIT_CTL;
`writePosition` is not established as IA32_RTIT_OUTPUT_MASK_PTRS or a physical
ToPA cursor. No hardware enable bits are inferred from either value.

The same-run user dump preserves exception and thread contexts. Its inspected
streams do not supply the relevant per-CPU PT MSR state. A complete user dump
does not imply a kernel/hardware-state dump.

[state-provenance.json](state-provenance.json) links the existing source hashes
and separates present scheduling/ETW evidence from missing TraceEn,
ContextEn, FilterEn, TriggerEn, Error/Stopped, output-base/pointer and ToPA state.

## Supported-interface check and consequence

Intel describes PacketEn as the conjunction of TriggerEn, ContextEn, FilterEn
and BranchEn. Thus control-only packets do not uniquely identify which gate or
output stage suppressed the required branch history. User-only capture also
cannot be interpreted as continuous tracking through kernel exception work.
These are mechanism constraints, not a proved cause in this run.

Sources: [Intel system programming manual](https://www.intel.com/content/dam/www/public/us/en/documents/manuals/64-ia-32-architectures-software-developer-vol-3c-part-3-manual.pdf),
[Microsoft WPR schema](https://learn.microsoft.com/en-us/windows-hardware/test/wpt/wprcontrolprofiles-schema).
The reviewed installed WPR schema exposes PT BufferSize, CodeMode and Events;
it does not expose a contemporaneous PT STATUS/ToPA-state logging option.
This is a limited interface finding, not a claim that no Windows tool can do it.

The evidence therefore cannot identify hardware generation, context filtering,
buffer export or final drain as the loss layer. Repeating the current profile
cannot fill that state gap by itself. No supported low-interference mechanism
for acquiring the missing same-instant state has been verified in this round;
no new experiment was launched under a false claim that it records those fields.

Original RBP corruption remains unresolved. No speculative fix, original
database/identity access, sustained a254/AOEM966 run, or double relay. A's
approval is not required; Git shares this evidence boundary only.
