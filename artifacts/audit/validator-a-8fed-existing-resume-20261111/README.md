# A current 8fed existing recovery — completed, do not rerun

User-executed elevated inspection at 11:01 Beijing established A's current
8fed task Ready/result267014, no node/listener, valid last progress and missing
exit.json. Last progress write was 09:47:26 Beijing. Exit cause remains unknown;
this is not proof of a clean drain or a program crash.

A ran the narrowly scoped elevated recovery once at 11:03:51–11:03:58 Beijing.
It checked the installed original owner hash, exact LocalService existing task
action, all fixed package members, original database/custody and absence of
processes/ports. Original logs were MOVED intact to the same work directory's
`logs-interrupted-20261111`; no database, journal, cache or key was removed.
Fresh logs received the original LocalService ACL. The same existing task and
same 8fed program were started, not an old version or initialization task.

- Actual PID13996 / S-1-5-19; ports 192.168.71.74:41051 and :42051.
- Node SHA256 021e7f8b0b41d8b72577d7394cd9c1fb9edb8388c4ae90903dca6f86a53a8a32.
- Loaded AOEM SHA256 1b764a36c1f782425237d40446d4ea827f56a66fb341b870855d84c15acdce50.
- Original `chain-state`, existing, custody unchanged, current_height1/headnull,
  execution_failures0/recoveryfalse; no initialization or key generation.
- Independent fixed SDK authenticated query PASS **33ms**, original state root
  f6186d40260aa18e84b01101d56ca3e918aff799ef41d927aeab554495f56333,
  finalized_height0/current_height1, no recovery/projection failure.

Raw public inspection/result/query and the executed helper are archived with
hashes. The helper is one-shot historical evidence, not a command to repeat.
No production code, timeout, resource budget or firewall was changed. B/C/D
were not restarted. No Full was triggered. No object was sent in this step.

Next: D arms the bounded read-only collector; A then sends one diagnostic
object through the original owner. The old task termination cause remains open.
