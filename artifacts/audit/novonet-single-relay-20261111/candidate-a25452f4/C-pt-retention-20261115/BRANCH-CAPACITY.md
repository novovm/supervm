# File01 has insufficient retained PT bits for the whole workload

This is a same-run quantitative proof, not a timestamp inference or a new recording. Machine-readable arithmetic and input hashes: `run-file01/decode/selected-private/branch-capacity-public.json`.

The unmodified qualification executable reported **3,590,651,904 completed loop iterations**. Its counter increases by 16,384 only after `ExecuteChunk` returns. Exact optimized machine code has two conditional branches per iteration: `74 07` at RVA `0x13c1` and `75 c9` at RVA `0x13d5`. To keep the bound deliberately loose, count only **one** conditional outcome per iteration.

Intel specifies that each valid TNT payload bit describes one conditional branch or compressed return; N payload bits cover N such outcomes. It is not run-length encoding of repeated arbitrary branch outcomes. The exact Intel libipt implementation likewise consumes one bit per conditional query. See [Intel SDM, Table 33-16, page 33-40](https://cdrdv2-public.intel.com/789585/326019-sdm-vol-3c.pdf#page=272) and [Intel libipt cache implementation](https://github.com/intel/libipt/blob/7f8575ce9bf25e48cf03983c12859e1edb592008/libipt/src/pt_tnt_cache.c#L52).

| Retained data granted to the target | Raw PT buffer bytes | Extremely generous outcome-bit ceiling | Fraction of the one-Jcc minimum |
|---|---:|---:|---:|
| Actual target PID/TID, 14,931 records | 60,773,704 | 486,189,632 | 13.5404% |
| Every process/thread in the ETL, 38,302 records | 121,386,176 | 971,089,408 | 27.0449% |

Even the second ceiling is below 3,590,651,904. It includes every byte of unrelated processes, headers, PSBs, padding, duplicated/overlapping fragments, and non-TNT packets as though all bits were usable outcomes. Thus the complete measured 15-second conditional-branch history **cannot be present in the retained PT data**, regardless of thread-attribution uncertainty or temporal gaps. This uses raw event-buffer lengths, not potentially compressed ETL file size.

This proves incomplete whole-workload representation. It does **not** measure a precise instruction-coverage percentage, identify which particular instruction was omitted, or rule out capturing a sufficiently short pre-fault interval. It also supplies no RBP values and says nothing about the original NOVOVM bug. File retention preserves emitted records; it does not turn periodic hardware-buffer samples into a full execution history.

No additional target run, PT capture, original-node operation, or production change was performed for this proof.
