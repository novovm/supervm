.if (@$t7 != 1) { .echo CONTEXT_GAP_CHILD_WITHOUT_ENTRY; q }
.printf "EVENT CHILD_AFTER group=%x tid=%x rip=%p rsp=%p rbp=%p\n", @$t10, @$tid, @rip, @rsp, @rbp
.printf "CHILD_SEQUENCE %x\n", @$t11
.if (@rbp != @$t6) { .echo CHILD_INTERVAL_MISMATCH; .dump /ma D:\WEB3_AI\SUPERVM\target\c-original-rbp-instruction-20261115-run2/captured-private.dmp; q }
r @$t7 = 0
g
