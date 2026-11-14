n 16
.sympath D:\WEB3_AI\SUPERVM\target\c-symbolized-original-20261114;D:\WEB3_AI\SUPERVM\target\c-first-fault-20261113\symbols
.reload /f ntdll.dll
.if ((dwo(ntdll+59dfc) != 246c8d48) | (by(ntdll+59e00) != b0) | (dwo(ntdll+5a108) != 304d8b48)) { .echo CONTROLLER_ACTION debugger_q code_mismatch; q }
r @$t17 = 0
sxe -c "$$><D:/WEB3_AI/SUPERVM/target/c-original-rbp-sustained-20261115-run1/exception.cmd;" -c2 "$$><D:/WEB3_AI/SUPERVM/target/c-original-rbp-sustained-20261115-run1/exception.cmd;" av
sxe -c "$$><D:/WEB3_AI/SUPERVM/target/c-original-rbp-sustained-20261115-run1/exception.cmd;" -c2 "$$><D:/WEB3_AI/SUPERVM/target/c-original-rbp-sustained-20261115-run1/exception.cmd;" 0xc0000409
.printf "DEBUGGEE_PID %x\n", @$tpid
.echo HANDLE_GATE_WAIT
