# PT owner coordination feasibility audit — C, 2026-11-15

## Decision

No reviewed route currently satisfies all requirements: supported on this host,
coordinated with the Windows PT owner, per-CPU final state retained before
cleanup, and low-interference validation. This is a scoped negative feasibility
result, not proof that every possible Windows tool lacks the capability.
Do not repeat the failed WPR profile or deploy a second PT owner. No driver,
BCD change, reboot, node launch or new capture was performed.

## Capability matrix

| Route | Documented or observed capability | Missing requirement | Decision |
|---|---|---|---|
| Existing WPR/ETW | Controls recording; exports PT payloads and scheduling/module sideband, session status and loss counters | No reviewed public hook for atomic PT MSR/ToPA snapshots before owner cleanup | Retain evidence; not qualified for required state experiment |
| DbgEng ReadMsr | Documented kernel-debugger MSR access | Kernel debugger inactive on C; ReadMsr alone neither owns WPR buffers nor synchronizes owner stop/cleanup | Supported primitive, not complete qualified route |
| WindowsIntelPT reference | Direct register control, per-processor data, own buffers and stop logic | No demonstrated WPR coexistence or current-host qualification | Source reference only; do not load |
| Separate supported vendor capture owner | Could in principle own stop/drain/state together | No specific product/interface verified here to export all required states | Not selected or represented as available |

WPR is the user-facing session controller. Windows kernel tracing facilities
perform hardware programming and buffer export; the exact internal ownership
split with HAL/hypervisor was not established. Do not call the WPR user process
the direct MSR owner. An external component must not alter registers or inspect
freed ToPA memory behind that owner's lifecycle.

## Register and ordering contract

Intel SDM Volume 3C, order 326019-060US (September 2016), sections 36.2.6.2,
36.2.7.3 and 36.3.5.1 were read directly. They specify that output MSR values
may be stale while TraceEn is set. The owner must preserve control state, clear
TraceEn, then obtain stable output state before reset/free; cross-CPU memory
observation also needs the documented ordering. Stop-time STATUS does not
reconstruct the earlier fault-time ContextEn. CR3_MATCH only has filtering
meaning with the corresponding enabled configuration. A valid record needs
processor group/number, read times, pre/post-stop distinction and retained
buffer ownership; it is not simply five values read after WPR exits.

[Intel SDM](https://www.intel.com/content/dam/www/public/us/en/documents/manuals/64-ia-32-architectures-software-developer-vol-3c-part-3-manual.pdf).
This historical official manual establishes the ordering principle, not this
CPU's complete current capability/errata qualification.

## Reference source audit

Reviewed IntelPt.cpp from the user-specified WindowsIntelPT URL; downloaded
source SHA256 is in environment-public.json. No source was built or loaded.
PauseResumeTrace reads CTL/STATUS and changes TraceEn. StopAndDisablePt clears
TraceEn, reads STATUS, then explicitly resets OUTPUT_BASE and OUTPUT_MASK_PTRS
(lines 480–513 in the downloaded file). It maintains its own per-processor
state and capability checks. Thus a post-stop reader may see reset output
values; this implementation is not a passive WPR observer. The inspected path
provides no verified shared-owner rendezvous with WPR. This is not a full
security or compatibility audit of that project.

[Reference implementation](https://github.com/intelpt/WindowsIntelPT/blob/master/WindowsPtDriver/IntelPt.cpp).

## Current host compatibility evidence

Read-only Win32_DeviceGuard: VirtualizationBasedSecurityStatus=2,
SecurityServicesConfigured=[2], SecurityServicesRunning=[2], Windows build26300.
Microsoft's definitions mean VBS and memory integrity are running. Previous
host query also found a hypervisor and 32 logical processors. These facts do
not prove that C is a guest VM, that PT is unavailable, or that VBS caused loss;
actual previous WPR traces already contained decodable PT.

The Microsoft Hyper-V performance-monitoring procedure concerns exposing
capabilities to guest VMs. It must not be applied as a host-VBS repair recipe.
No guest settings, memory-integrity setting or hypervisor configuration changed.
Driver acceptance, MSR virtualization and WPR coexistence would need separate
verification for a specific supported implementation.

## Sources and remaining requirement

- [WPR schema](https://learn.microsoft.com/en-us/windows-hardware/test/wpt/wprcontrolprofiles-schema): reviewed configuration is not runtime register telemetry.
- [ReadMsr](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/dbgeng/nf-dbgeng-idebugdataspaces-readmsr): kernel-mode debugging only; no WPR coordination contract.
- [Memory integrity status](https://learn.microsoft.com/en-us/windows/security/hardware-security/enable-virtualization-based-protection-of-code-integrity): host status interpretation.
- [Hyper-V guest PMU/IPT](https://learn.microsoft.com/en-us/windows-server/virtualization/hyper-v/manage/performance-monitoring-hardware): guest-specific requirements, not host verification.

Minimal next prerequisite is an owner-supported stop/drain/snapshot contract
for this Windows/hypervisor configuration, with a documented per-CPU state
export. None was found in the reviewed public interface/source. Do not label
another configuration-only change as that capability. This audit completes the
requested feasibility review but does not deliver hardware snapshots, explain
the missing PT tail, or locate original RBP corruption. Original database,
identity and failure evidence stay preserved; double relay remains paused.
