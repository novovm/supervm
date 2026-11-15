# PID3848 allocation request and empty-table state

## Result

The failed allocation requested39582418599952bytes (0x240000000010,36TiB+16), alignment16. The request comes from invalid metadata in a newly initialized IndexMap inside RpcLifecycle::status while constructing archive_work and inserting its first key reads_started. The dense entries Vec has capacity0, pointer8 and length0. The raw table has ctrl8, bucket_mask0x1e5716ef0e0, growth_left0, items0x1e5716c9e40. The mask violates the power-of-two-minus-one invariant. The items value is unrelated to the last recorded185620ticks or3129network writes.

The immediate failure is explained by the invalid table's enormous request. The last target sample had67,760,128 private bytes and sampled peak70,610,944bytes. Dump state provides no global commit-pressure time series, so concurrent system memory pressure remains unmeasured. The evidence supports corrupted capacity/register state as the origin of this request. A production memory-budget increase does not follow from these values.

## Reconstructed arithmetic

The actual reserve_rehash machine code uses additional1, reads items from table+0x18 and bucket_mask from table+8. It computes new_items=2084962082369 and full_capacity=1824341955268, then selects max(new_items,full_capacity+1). fallible_with_capacity scales by8/7 and rounds up to0x40000000000 buckets. Each index bucket is8bytes, producing0x200000000000 data bytes plus0x40000000010 control bytes. The resulting0x240000000010 matches retained RBX and the failure call's argument preparation. R12=16 supplies alignment. The null-return branch reaches handle_alloc_error after __rust_alloc. These are machine-code/data-flow deductions from the dump, rather than captured historical entry-register snapshots.

## Source of invalid fields

The source at a25452f4 crates/novovm-node/src/native_pipeline/service/rpc.rs constructs archive_work with reads_started, verified_material_reused and scope at lines1391 onward. The faulting insertion's key bytes spell reads_started, and the counter value loads from core+0x37e8.

At module RVA0x7721e6/0x7721f4, status loads XMM6/XMM7 from the empty-table constants. Captured constants remain [empty_ctrl,0,0,0], with empty_ctrl0x7ff7f2e45400. Static analysis of status finds these initial loads and the epilogue restores as its explicit XMM6/XMM7 destination instructions.

At RVA0x77823e/0x778245 the constructor stores XMM6/XMM7 into the new table. The resulting table fields exactly equal both the insert_full prologue's saved XMM6/XMM7 slots and the ExceptionStream's raw XMM6/XMM7 values. They are [8,0x1e5716ef0e0,0,0x1e5716c9e40]. The invalid state is therefore already present when this insertion begins. The empty-table constants and these instruction ranges were read from the same full dump. Selected status, insert_full, reserve and allocation instruction bytes match the original EXE exactly. Per-range sizes and hashes are in allocation-values-public.json.

This narrows the upstream anomaly to the retained vector state during the active status call. The dump has no record showing the first XMM change or a specific callee returning different values. The vector-store instruction identifies where those values enter the table on the normal path. A single final snapshot cannot rule out later overwrites of the same slots or prove every historical branch. Do not identify the constructor store, Hashbrown, AOEM or a Windows function as the original corruptor from this evidence alone.

Windows x64 requires callees to preserve XMM6-XMM15. The first violating call or context-restoration event remains unidentified. [Microsoft x64 calling convention](https://learn.microsoft.com/en-us/cpp/build/x64-calling-convention?view=msvc-170)

## Comparison with PID34452 RBP failure

Both runs used a254+AOEM966 and reached allocation-related paths, but their termination sites differ. PID34452 failed inside RtlpReAllocateHeapInternal's cookie check with reconstructed RBP displacement0x2a0. PID3848 reaches Rust's allocation-error abort, FAST_FAIL_FATAL_APP_EXIT7. Its checked active frames match their own prologues. status RSP0x53aaceeb90 implies RBP0x53aaceec10, matching the saved slot at0x53aaceeb80. insert_full RSP0x53aaceea70 implies RBP0x53aaceeaf0, matching slot0x53aaceea60. reserve_rehash RSP0x53aacee990 implies RBP0x53aaceea10, matching slot0x53aacee958. These saved slots and unwind deductions are distinguished from historical entry values.

The common observation is abnormal preserved execution state across calls, involving RBP in the older report and XMM6/XMM7 here. A shared originating instruction, function, hardware cause or ABI violation has not been demonstrated. No RBP displacement0x2a0 is established for PID3848. This comparison does not merge the two executions into one history.

## Evidence and execution boundary

All analysis used the same213986993byte Full Dump, SHA2566c3b7b9dc34ec3c137c2b4e7bcf0f4746cf6ce023db90f6a6dc423dce2613de6, matching PDB GUID E16D0B11-47CA-4EBF-B482-C91380CC57F8 age1. Raw dump and debugger logs remain local. Selected vector values contain only table metadata/address evidence. The original stored EXE/DLL and failure files were read offline. Nodes and tests were not executed. Original database and identity material were not accessed. No production fix was justified or made, and double relay remains paused.

The evidence gap is the transition between the initial correct XMM template loads and their invalid values at insert_full entry during the same status call. The complete dump preserves the endpoint and intermediate object state. It does not preserve the first earlier register write or restore. A defensible fix requires that transition's cause.
