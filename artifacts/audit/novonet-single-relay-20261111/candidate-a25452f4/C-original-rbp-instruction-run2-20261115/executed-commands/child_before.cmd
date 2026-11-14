.if (@$t7 != 0) { .echo CONTEXT_GAP_NESTED_CHILD; q }
r @$t11 = @$t11 + 1
r @$t6 = @rbp
r @$t7 = 1
.printf "EVENT CHILD_BEFORE group=%x tid=%x rip=%p rsp=%p rbp=%p\n", @$t10, @$tid, @rip, @rsp, @rbp
.printf "CHILD_SEQUENCE %x\n", @$t11
u @rip L1
g
