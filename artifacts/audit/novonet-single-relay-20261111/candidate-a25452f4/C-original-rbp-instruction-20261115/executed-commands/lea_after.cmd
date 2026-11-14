bc 2
.printf "EVENT LEA_AFTER group=%x tid=%x rip=%p rsp=%p rbp=%p\n", @$t10, @$tid, @rip, @rsp, @rbp
.if (@rbp != (@$t4 - 50)) { .echo LEA_MISMATCH; .dump /ma D:\WEB3_AI\SUPERVM\target\c-original-rbp-instruction-20261115-run1/captured-private.dmp; q }
ba0 e 1 ntdll+5a05b "r @$t19 = @$t19 + 1; .if (@$t19 > 1000) { .echo HIT_LIMIT_STOP; q } .else { .if (@$tid == @$t0 && @rsp == (@$t4 - 150)) { $><D:\WEB3_AI\SUPERVM\target\c-original-rbp-instruction-20261115-run1\child_before.cmd } .else { gc } }"
ba1 e 1 ntdll+5a060 "r @$t19 = @$t19 + 1; .if (@$t19 > 1000) { .echo HIT_LIMIT_STOP; q } .else { .if (@$tid == @$t0 && @rsp == (@$t4 - 150)) { $><D:\WEB3_AI\SUPERVM\target\c-original-rbp-instruction-20261115-run1\child_after.cmd } .else { gc } }"
ba3 e 1 ntdll+5a108 "r @$t19 = @$t19 + 1; .if (@$t19 > 1000) { .echo HIT_LIMIT_STOP; q } .else { .if (@$tid == @$t0 && @rsp == (@$t4 - 150)) { $><D:\WEB3_AI\SUPERVM\target\c-original-rbp-instruction-20261115-run1\check.cmd } .else { gc } }"
g
