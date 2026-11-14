bc 0
r @$t10 = @$t10 + 1
r @$t0 = @$tid
r @$t1 = @rsp
r @$t4 = 0
r @$t7 = 0
r @$t11 = 0
.printf "EVENT OUTER_CALL group=%x tid=%x rip=%p rsp=%p rbp=%p\n", @$t10, @$tid, @rip, @rsp, @rbp
ba1 e 1 ntdll+59dfc "r @$t19 = @$t19 + 1; .if (@$t19 > 0n100000) { .echo CONTROLLER_ACTION debugger_q hit_limit; q } .else { .if ((@$tid == @$t0) & (@rsp < @$t1) & (poi(@rsp+38) == ntdll+5bddd)) { $$><D:/WEB3_AI/SUPERVM/target/c-original-rbp-sustained-20261115-run1/before.cmd; } .else { gc } }"
g
