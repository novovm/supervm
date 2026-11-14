.if (@$t7 != 1) { .echo CONTROLLER_ACTION debugger_q missing_child_entry; q }
.printf "EVENT CHILD_AFTER group=%x tid=%x rip=%p rsp=%p rbp=%p\n", @$t10, @$tid, @rip, @rsp, @rbp
.if (@rbp != @$t6) { .echo RBP_MISMATCH; .printf "EVENT MISMATCH group=%x tid=%x rip=%p rsp=%p rbp=%p\n", @$t10, @$tid, @rip, @rsp, @rbp; kv; .dump /ma D:/WEB3_AI/SUPERVM/target/c-original-rbp-sustained-20261115-run1/mismatch-private.dmp; bc *; .echo MISMATCH_CAPTURED_CONTINUE_TO_NATIVE_CHECK; g }
r @$t7 = 0
g
