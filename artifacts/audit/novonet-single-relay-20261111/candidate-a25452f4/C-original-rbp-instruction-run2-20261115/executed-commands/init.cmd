n 16
.sympath D:\WEB3_AI\SUPERVM\target\c-symbolized-original-20261114;D:\WEB3_AI\SUPERVM\target\c-first-fault-20261113\symbols
.reload /f ntdll.dll
.if ((dwo(ntdll+59dfc) != 246c8d48) | (by(ntdll+59e00) != b0) | (dwo(ntdll+5a108) != 304d8b48)) { .echo CODE_MISMATCH_STOP; q }
r @$t10 = 0
r @$t19 = 0
r @$t7 = 0
sxe -c ".echo FIRST_CHANCE_AV_CONTROLLED_STOP; .dump /ma D:\WEB3_AI\SUPERVM\target\c-original-rbp-instruction-20261115-run2/captured-private.dmp; q" av
sxe -c ".echo FIRST_CHANCE_FAST_FAIL_CAPTURE; .dump /ma D:\WEB3_AI\SUPERVM\target\c-original-rbp-instruction-20261115-run2/captured-private.dmp; q" 0xc0000409
sxe -c ".echo CONTEXT_GAP_CPP_EXCEPTION; q" eh
$$><D:/WEB3_AI/SUPERVM/target/c-original-rbp-instruction-20261115-run2/arm.cmd;
