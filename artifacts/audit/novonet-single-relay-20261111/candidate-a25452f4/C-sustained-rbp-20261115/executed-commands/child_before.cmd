.if (@$t7 != 0) { .echo CONTROLLER_ACTION debugger_q nested_child_gap; q }
r @$t7 = 1
r @$t11 = @$t11+1
r @$t6 = @rbp
.printf "EVENT CHILD_BEFORE group=%x tid=%x rip=%p rsp=%p rbp=%p\n", @$t10, @$tid, @rip, @rsp, @rbp
g
