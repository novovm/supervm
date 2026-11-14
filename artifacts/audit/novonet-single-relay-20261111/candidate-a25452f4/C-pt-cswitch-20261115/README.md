# C File + CSwitch qualification: actual capture complete, fault tail not qualified

The renewed administrator launch succeeded. C executed the already prepared
File + native CSwitch entry **once**, then decoded the resulting trace offline.
This is a qualification-program result, not a NOVOVM crash reproduction or fix.
The required same-call exception path remains absent from the examined target
fragments, so **a254 + AOEM966 sustained isolated load was not started**.

## Actual execution and exits

- Prepared source: main `8fe2a73898756e67015c2c797ba64c15cb844311`.
- Beijing time: **2026-11-15 06:56:26–06:57:20**; exact UTC in
  [controller-result.json](controller-result.json).
- `launch-02.json`: elevated helper PID 15408, created UTC 22:56:26.0202278.
  The previous cancelled launch is retained in
  [the preceding report](../C-pt-retention-20261115/README.md); it did not run.
- Same unchanged qualification EXE/PDB, 15 seconds baseline and 15 seconds
  traced work, both using the declared qualification exception. Fixed hashes
  are in [input-hashes.json](input-hashes.json). No rebuild or node launch.
- Baseline PID 27012 and traced PID 22116 both exited **-532528300 /
  0xE0424354**, the expected declared qualification exception, not the original
  NOVOVM access violation or fast-fail. Fault/work thread: **32528**.
- WPR pre-status, start, active-status, counters, stop and both final-status
  commands exited **0**; xperf exports and tracerpt exited **0**. Actual decoder
  exits and errors are retained in [decode-public.json](decode-public.json).
  The elevated helper's independent OS exit code was not separately retained.
- ETL: **367,263,744 bytes**. Same-run Full Dump: **25,512,463 bytes**;
  SHA-256 `9597da0611bcf3bd0cfe14427fa6e5b77b66715b64879566e502b244bcfc81c4`.
  Private artifact identities are in [private-artifact-hashes.json](private-artifact-hashes.json).
- WPR confirms File mode and **CSwitch + SampledProfile** PT triggers, with
  4 KiB PT buffers. Requested native sequential cap and external disk/size
  guard were 512 MiB; neither guard fired. Cap-trigger behavior is not proven.
- Final check: no qualification/node/CDB process and WPR not recording.
  See [final-status.json](final-status.json).

The controller's original `actualPtDecoded: false` is its pre-offline-analysis
state; it was not rewritten. Subsequent decode artifacts document actual
decoding. `qualificationPassed: false` remains the correct overall result.

## Same-run thread, module and packet findings

The [compact coverage](filecs01-coverage-compact-public.json) and
[owner correlation](filecs01-owner-public.json) retain target-only evidence:

- Process Start/End/Zombie, worker Start/End, **34 module events**,
  **11,650 target CSwitch events**, and **20,733 target-thread PT envelopes**.
- **14,913 raw target samples** match xperf's raw QPC/IP. Ordinary ETW lost
  counters are zero; no LastDroppedTimes record is present. These facts do not
  establish complete PT instruction history.
- All **11,382 exact QPC+CPU associations** between target-related switches
  and PT headers identify the **incoming** thread. This is observed envelope
  attribution, not proof of which residence the attached bytes represent.
- Handler QPC is **3074711522398**, frequency 10,000,000. The target switches
  into CPU 9 at 3074711503759 and out at **3074711522469**, 7.1 microseconds
  after the handler QPC. No raw PT header at that exact switch-out QPC/CPU
  exists anywhere in the retained system headers.
- Last pre-handler target envelope is **505.8 microseconds** earlier. All
  three post-handler target envelopes were checked: +39.6617 / +62.4581 /
  +73.3206 ms, respectively **192 / 512 / 384 bytes**. Each contains only
  **PSB/CBR/PSBEND, no TIP/FUP/TNT, and zero decoded instructions**.
- Six selected pre-handler fragments and those three post-handler fragments
  were decoded against same-run captured code. Nonempty fragments contain
  real workload instructions, but none reaches the mapped Work call to
  RaiseException, RaiseException, captured exception RIP, or handler/wait.
  See [fault-path-public.json](fault-path-public.json).
- Nearby CPU 9 headers assigned to other threads were also examined privately;
  they do not establish the missing target path. No other process's decoded
  addresses or trace bytes are published or relabeled as this target.

Thus adding CSwitch did **not** establish the required
`Work -> exception -> handler -> Wait` path. A decoder exit of zero with zero
instructions is not successful fault-tail capture. QPC on an ETW envelope is
not the timestamp of its last instruction; a post-fault scheduling delay is
not automatically an execution gap. Independent fragments were not joined
into fictitious continuity. Not all system-wide PT was instruction-decoded.

The conservative whole-workload bit bound in the fault-path summary also
excludes a complete 15-second branch history in this ETL. It is neither an
actual coverage percentage nor proof that short-tail capture is impossible.

## Interference and full-context limits

[qualification-analysis-public.json](qualification-analysis-public.json)
records **-3.16% throughput**, **+2.82% thread CPU per iteration**, and
**+3.00% cycles per iteration**. This is one sequential short baseline/trace
pair. It does not qualify NOVONET throughput, P95/P99, whole-system overhead,
or a 66-minute production fault window.

The Full Dump has FullMemory, ThreadInfo and FullMemoryInfo flags, 157 memory
ranges and six thread contexts. Its ExceptionStream matches the original
saved CONTEXT: RIP `0x7ffce03b569a`, RSP `0xf2bacff6a0`, RBP `0`.
Exception RIP and RSP pages are covered. RBP zero in this optimized qualification
program is not an ABI violation. This dump is not a NOVOVM natural failure,
and neither the dump nor PT supplies historical per-instruction RBP values.

## Evidence and disposition

Raw ETL, DMP, whole-system XML/CSV, PT bytes and instruction listings remain
local under `target/c-pt-live-qualification-20261115/run-filecs01`.
The published files are a whitelist of target-only projections, tool exits,
original WPR output, hashes and offline audit source. Source scripts are
archival copies for their original private analysis layout; no replay is
performed by this report. Earlier captures and cancelled launches remain.

The next missing evidence is the target's actual exception-path control flow
in this same run, not another successful parsing/configuration check. This
File + SampledProfile + CSwitch configuration has not demonstrated that
retention capability; extending it to the original long fault window is not
justified. There is **no first RBP-corruption instruction and no repair**.

Original database, identity and failure evidence were not accessed or changed.
No original node restart, speculative production fix, or double relay.
C publishes independently; A's acknowledgement is not a prerequisite.
