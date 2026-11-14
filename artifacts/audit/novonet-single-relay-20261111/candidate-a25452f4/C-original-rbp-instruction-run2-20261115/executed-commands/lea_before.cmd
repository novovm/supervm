bc 1
r @$t4 = @rsp
r @$t5 = @rbp
r @$t12 = poi(@rsp+38)
.printf "EVENT LEA_BEFORE group=%x tid=%x rip=%p rsp=%p rbp=%p\n", @$t10, @$tid, @rip, @rsp, @rbp
.printf "HEAP_RETURN %p\n", @$t12
u @rip L1
ba2 e 1 ntdll+59e01 "r @$t19 = @$t19 + 1; .if (@$t19 > 0n1000) { .echo HIT_LIMIT_STOP; q } .else { .if ((@$tid == @$t0) & (@rsp == @$t4) & (poi(@rsp+38) == @$t12)) { $$><D:/WEB3_AI/SUPERVM/target/c-original-rbp-instruction-20261115-run2/lea_after.cmd; } .else { gc } }"
g
