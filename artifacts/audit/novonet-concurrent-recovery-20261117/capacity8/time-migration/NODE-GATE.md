# Full 44089528662: test lifecycle race, not a chrono/security failure

Exact Full source: 6d2c3e7d001b9d1c6635f07eb8b9a30bc1e3b4cd.
Rust security checks succeeded; Node checks job134061108092 failed30/31 on
`controls.test`: `409 session busy or needs operator recovery`.

The test watched the internal task status, then slept20ms before reusing the
session. Broker.run publishes task completion before awaiting meter/session
cleanup. The existing public events API deliberately reports running while its
controller still owns cleanup. The fixed test consumes that original API until
completed for both turns. No Broker, meter, engine, retry, admission, permission,
transport or production timeout changes. No assertions removed; the original
10-second test timeout remains. This is a test-only gate correction.

Windows npm build/test:31/31, exit0. Linux native Node22.23.5 TypeScript build and
the same31 tests:31/31, exit0 (existing installed dependencies, not a claim that
npm ci was rerun locally). Linux tool archive was downloaded from nodejs.org and
verified against its published SHA256:
`cdeb8670a0c81f51b4dc4ac61919f9b1ad917b304bf8221eb939570663d240f8`.

The failed Full remains failed, other jobs finish without cancellation. Final
candidate must include this test correction and use another exact-source Full;
local passes do not override the prior failure. No actual node deployment.

The first Linux package attempt on D correctly refused a dirty-worktree view:
historical audit EOL/index discrepancies, not a relaxed source check. Original
frozen C checkout remains available. C disk capacity is low; deletion of the
verified obsolete/uninstalled e666 Linux assembly was blocked by tool policy,
not performed. User was given the single exact cleanup command. No database,
key, journal, source or current installed program was targeted.
