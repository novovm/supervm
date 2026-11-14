r @$t0 = 0
r @$t4 = 0
r @$t7 = 0
r @$t10 = 0
r @$t18 = 0
r @$t19 = 0
bp10 ntdll!RtlUnwindEx ".echo CONTROLLER_ACTION debugger_q unwind_gap; q"
bp11 ntdll!RtlRestoreContext ".echo CONTROLLER_ACTION debugger_q restore_gap; q"
bp12 ntdll!NtContinue ".echo CONTROLLER_ACTION debugger_q continue_gap; q"
bp13 novovm_node+84eed9 ".if ((@$t0 != 0) & (@$tid == @$t0) & (@rsp == @$t1)) { .echo CONTROLLER_ACTION debugger_q outer_return_without_group; q } .else { gc }"
ba3 e 1 ntdll+5a108 "r @$t19 = @$t19 + 1; .if (@$t19 > 0n100000) { .echo CONTROLLER_ACTION debugger_q hit_limit; q } .else { .if (1) { $$><D:/WEB3_AI/SUPERVM/target/c-original-rbp-sustained-20261115-run1/check.cmd; } .else { gc } }"
.echo MONITOR_ON
$$><D:/WEB3_AI/SUPERVM/target/c-original-rbp-sustained-20261115-run1/arm.cmd;
