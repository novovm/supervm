bc 1
r @$t4 = @rsp
r @$t12 = poi(@rsp+38)
.printf "EVENT LEA_BEFORE group=%x tid=%x rip=%p rsp=%p rbp=%p\n", @$t10, @$tid, @rip, @rsp, @rbp
ba2 e 1 ntdll+59e01 "r @$t19 = @$t19 + 1; .if (@$t19 > 0n100000) { .echo CONTROLLER_ACTION debugger_q hit_limit; q } .else { .if ((@$tid == @$t0) & (@rsp == @$t4) & (poi(@rsp+38) == @$t12)) { $$><D:/WEB3_AI/SUPERVM/target/c-original-rbp-sustained-20261115-run1/after.cmd; } .else { gc } }"
g
