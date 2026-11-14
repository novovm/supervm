# C per-CPU PT hardware-state access preflight

Actual read-only check: 32 logical processors, hypervisor present;
NtQuerySystemInformation class 35 returned NTSTATUS 0, two bytes:
KernelDebuggerEnabled=0, KernelDebuggerNotPresent=1. kdnic is running but does
not prove an active kernel debugging session. No PT MSR was read. No inference
is made that the hypervisor caused either PT loss or NOVOVM corruption.

The checked WPR interface supplies configuration/events, not same-instant MSR
snapshots. Microsoft distinguishes user-mode virtual memory/register access
from kernel-mode MSR access. A new administrator process alone does not create
this channel. Local kernel debugging setup normally changes boot configuration
and requires reboot; limited live-local debugging also is not automatically a
synchronized stop across all logical processors.

Sources:
- https://learn.microsoft.com/en-us/windows-hardware/drivers/debugger/memory
- https://learn.microsoft.com/en-us/windows-hardware/drivers/debugger/setting-up-local-kernel-debugging-of-a-single-computer-manually
- https://learn.microsoft.com/en-us/windows-hardware/drivers/debugger/live-local-debugging

A valid future hardware snapshot must distinguish pre-stop CTL/STATUS and
post-stop OUTPUT_BASE/MASK_PTRS/ToPA, identify processor group and processor
number plus timing, and coordinate with the actual trace owner. Reading after
ordinary WPR stop may instead observe deconfigured or freed resources. Direct
WRMSR behind WPR would interfere with its ownership and is not an accepted
read-only experiment. A post-stop state cannot retroactively recover the
pre-stop ContextEn. No driver, boot-security changes or reboot were attempted.

This preflight does not establish that no supported channel exists anywhere;
it establishes that none has been verified and active on C for this task.
The requested per-CPU state experiment has NOT run and its acceptance is NOT
passed. No repeat capture, original node/database/identity access, speculative
fix or double relay. Further execution requires a validated kernel-level
access/trace-owner coordination method, not another identical WPR profile.
