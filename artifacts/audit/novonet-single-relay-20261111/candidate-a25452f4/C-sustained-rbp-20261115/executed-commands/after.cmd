bc 2
.printf "EVENT LEA_AFTER group=%x tid=%x rip=%p rsp=%p rbp=%p\n", @$t10, @$tid, @rip, @rsp, @rbp
.if (@rbp != (@$t4-50)) { .echo RBP_MISMATCH; .printf "EVENT MISMATCH group=%x tid=%x rip=%p rsp=%p rbp=%p\n", @$t10, @$tid, @rip, @rsp, @rbp; kv; .dump /ma D:/WEB3_AI/SUPERVM/target/c-original-rbp-sustained-20261115-run1/mismatch-private.dmp; bc *; .echo MISMATCH_CAPTURED_CONTINUE_TO_NATIVE_CHECK; g }
ba0 e 1 ntdll+5a05b "r @$t19 = @$t19 + 1; .if (@$t19 > 0n100000) { .echo CONTROLLER_ACTION debugger_q hit_limit; q } .else { .if ((@$tid == @$t0) & (@rsp == (@$t4-150))) { $$><D:/WEB3_AI/SUPERVM/target/c-original-rbp-sustained-20261115-run1/child_before.cmd; } .else { gc } }"
ba1 e 1 ntdll+5a060 "r @$t19 = @$t19 + 1; .if (@$t19 > 0n100000) { .echo CONTROLLER_ACTION debugger_q hit_limit; q } .else { .if ((@$tid == @$t0) & (@rsp == (@$t4-150))) { $$><D:/WEB3_AI/SUPERVM/target/c-original-rbp-sustained-20261115-run1/child_after.cmd; } .else { gc } }"
g
