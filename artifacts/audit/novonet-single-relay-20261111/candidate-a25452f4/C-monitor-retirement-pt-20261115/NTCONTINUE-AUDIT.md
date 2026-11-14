# Offline NtContinue control-flow audit

Scope: read-only analysis of archived diagnostic scripts/logs and an existing PID34452 full dump. No node, sample, live attach, database access, identity access or network object was started. Existing evidence was not overwritten.

## Proven monitored-run facts

Archive: target/c-original-rbp-sustained-20261115-run1.
- executed-script.py:86 defines the guard action as echo plus q.
- executed-script.py:98 and on.cmd:9 place an unconditional, all-thread NtContinue guard. It does not check the tracked TID or active call instance.
- executed-script.py:111-113 calls DebugBreakProcess before submitting off.cmd on debugger stdin.
- off.cmd:2 clears breakpoints only after the debugger processes that command file.
- actions-public.jsonl records OFF break request at 2026-11-14T19:52:00.872215Z.
- cdb-private.log:13060-13061 records CONTROLLER_ACTION debugger_q continue_gap then quit. MONITOR_COUNTS and MONITOR_OFF were never emitted.
- The retained target process exit was 0xC0000354. This is not evidence of the original natural NOVOVM fault.

## Static system-code evidence

system-code-verified.txt contains only bounded system-function disassembly and commands. CDB exit was 0. Its instruction starts were checked against the earlier full function disassembly.
- DbgUiIssueRemoteBreakin at 0x00007ffce3288f03 selects DbgUiRemoteBreakin; at 0x00007ffce3288f62 calls RtlpCreateUserThreadEx.
- LdrInitializeThunk at 0x00007ffce322ff63 calls NtContinue, after moving saved RCX to RCX and setting DL=1.
- DbgUiRemoteBreakin at 0x00007ffce3289029 calls DbgBreakPoint, followed by RtlExitUserThread.

This establishes a statically feasible self-conflict: requesting a remote debugger break can create a thread whose initialization reaches NtContinue while the unconditional guard is still installed, before off.cmd can clear the guard.

## Evidence boundary

The system instructions were examined offline using the OLD PID34452 Full Dump, target/c-wer-natural-full-20261114T122549Z/novovm-node.exe.34452.dmp. They are NOT a captured stack or execution history of the monitored PID29764 guard hit. The guard did not capture its hit TID, caller stack, CONTEXT pointer or thread creation event. Therefore the exact actual caller of NtContinue in the monitored run remains unproved. Static feasibility must not be reported as proof of that dynamic thread path or as the original RBP-corruption root cause.

The last monitored group 0x86a stops after CHILD_BEFORE. Incomplete records caused by the guard exit do not prove an RBP violation.

## Local capture correction

The initial system-code-excerpt.txt is retained but superseded: its first bounded disassembly began inside an instruction at 0x00007ffce3288ef9. Do not use those initial two decoded instructions. system-code-verified.txt starts at verified instruction boundaries and is the only excerpt for the public report.

Conclusion: retire this guarded break/off control sequence. No production fix is justified by these results; no original natural fault was captured by this monitored run.
