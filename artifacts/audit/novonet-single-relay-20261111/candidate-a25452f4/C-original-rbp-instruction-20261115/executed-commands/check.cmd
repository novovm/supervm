.printf "EVENT COOKIE_PRECHECK group=%x tid=%x rip=%p rsp=%p rbp=%p\n", @$t10, @$tid, @rip, @rsp, @rbp
.if (@rbp != (@rsp + 100)) { .echo PRECHECK_MISMATCH; kv; .dump /ma D:\WEB3_AI\SUPERVM\target\c-original-rbp-instruction-20261115-run1/captured-private.dmp; q }
bc *
r @$t7 = 0
.if (@$t10 < 8) { $><D:\WEB3_AI\SUPERVM\target\c-original-rbp-instruction-20261115-run1\arm.cmd } .else { .echo CAPTURE_GROUP_LIMIT; g }
