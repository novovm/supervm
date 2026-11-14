# C raw PT tail byte audit

Existing run-filecs01 only, no new recording. All 192/512/384 bytes of the three
post-fault fragments were compared to the decoded packet offsets: every byte
is a PSB/CBR/PSBEND packet byte or zero padding. No unexplained nonzero bytes
remain. This excludes an instruction decoder merely failing to consume hidden
TIP/TNT data within these specific buffers. The JSON preserves packet order,
raw control bytes, SHA256 and byte accounting; no unrelated process data.

Controller target exit: 22:56:57.2274808Z. WPR stop requested:
22:56:57.3702182Z, completed 22:57:19.9773222Z, exit 0. Therefore the controller
did not stop WPR before the exception; this does not prove the kernel flushed
the target's last user-mode PT residence. Exact CSwitch attribution and missing
switch-out evidence remain as recorded in the preceding report.

The observed post-fault worker waits for dump completion in the original
qualification source. Control-only packets are compatible with tracing while
user execution is filtered/inactive; the trace does not contain the hardware
TraceEn/ContextEn/FilterEn or ToPA state needed to establish that as the cause.
Do not assert PT stopped globally, a ToPA overflow, or faulty decoding without
that evidence. No fix to instruction retention is established.

No repeated failed configuration, no new ordinary sample, no original node,
database or identity operation. Full exception-path qualification and original
RBP root cause remain unresolved; sustained a254/AOEM966 load and double relay
remain paused. This is a raw-evidence exclusion, not completed user acceptance.

Audit script initially used a nonexistent controller-result location; failed
before output creation. Corrected to the published immutable controller record
and reran offline only. All raw source files remain unchanged.
