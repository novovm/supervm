bc 0
r @$t10 = @$t10 + 1
r @$t0 = @$tid
r @$t1 = @rsp
r @$t11 = 0
.printf "EVENT OUTER_CALL group=%x tid=%x rip=%p rsp=%p rbp=%p\n", @$t10, @$tid, @rip, @rsp, @rbp
bp10 ntdll!RtlUnwindEx ".echo CONTEXT_GAP_UNWIND; q"
bp11 ntdll!RtlRestoreContext ".echo CONTEXT_GAP_RESTORE; q"
bp12 ntdll!NtContinue ".echo CONTEXT_GAP_CONTINUE; q"
bp13 novovm_node+84eed9 ".echo OUTER_RETURN_WITHOUT_COMPLETED_GROUP; q"
ba1 e 1 ntdll+59dfc "r @$t19 = @$t19 + 1; .if (@$t19 > 1000) { .echo HIT_LIMIT_STOP; q } .else { .if (@$tid == @$t0 && @rsp < @$t1 && poi(@rsp+38) == ntdll+5bddd) { $><D:\WEB3_AI\SUPERVM\target\c-original-rbp-instruction-20261115-run1\lea_before.cmd } .else { gc } }"
g
