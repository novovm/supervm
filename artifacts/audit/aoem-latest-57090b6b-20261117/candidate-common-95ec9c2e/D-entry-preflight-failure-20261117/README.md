# D entry implemented; read-only validation rejected

D-scoped scripts/windows/Switch-D-Common95.ps1 was added, preserving
the original helper hash guard, package qualification and LocalService
existing-start checks. One VerifyOnly call failed at helper hash identity:
Original deployment helpers changed. The actual update was NOT called.
No node startup, DB modification, initialization, key change or retry.
Original exit/logs and separate raw/LF hash observations are preserved.
The entry is unqualified and must not be deployed until this check is resolved;
this is not D_COMMON95_EXISTING_READY_FOR_A_QUERY.
