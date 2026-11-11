# C current875 recovery succeeded; fresh v2 relay observer ARMED

Synced main9a0aec5e. Local recovery helper checks:22 passed. Executed the
unchanged Resume-Novonet875C.ps1 once in administrator PowerShell7; actual child
PID676 exited0 at2026-11-11T15:11:26.8561361Z.

Recovery result: C_875_RECOVERED_READY_FOR_A_QUERY. Current875 nodePID3228,
LocalService S-1-5-19, original chain-state opened with START=existing, height1,
headnull, execution_failures0. Both192.168.71.125:41052/42052 belong toPID3228.
Actual node/AOEM hashes, configuration and custody equality are in the original
result-public.json. No initialize, old8fed launch, package transfer, config/task
change, object send or D operation occurred.

Crash event1000 record546255 / PID31296 matches task and exit c0000005 at
13:27:53.4454568Z; fault module novovm-node.exe offset000000000065f59d. Recovery
is not a crash fix. Original logs were preserved in the exact same work parent
as logs-crash-875-8e264dcea90d488bbb500e151b2a5a60. Task XML, event XML and raw
private logs remain local. They are not uploaded. No dumps or private custody
material are included.

After success the exact old failed observerPID1700 was confirmed absent; its
original exit1 is preserved. One original v2 observerPID21548 then started hidden
and independently, with the unchanged Watch-Relay.ps1 and adjacent reader.
Actual READY2026-11-11T15:11:27.622273Z (Beijing23:11:27), maximum window until
15:41:27.622273Z (Beijing23:41:27). Fresh real liveness is included. At handoff,
result/error/observer-exit/launcher-error are absent, snapshots0. This is an
active readiness receipt, not relay delivery acceptance.

A: query recovered C through the original authenticated interface, confirm C/D
actual current liveness, then send the single authorized relay object. D's older
published sample is not fresh proof; this machine does not operate or certify D.
Do not reuse an expired READY or send if either observer has exited.

Public originals are copied byte-for-byte; SHA256.json lists all published files.
The local bounded wrapper remains to record the actual observer exit. No manual
Full CI run was triggered. Previous failures remain untouched.
