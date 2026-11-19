# A/D Iroh time reply Windows candidate — receive and verify

Program source `654b99916e80004e2df999c6d393ed7b94bec49b`. [Full #44501473176](https://github.com/novovm/supervm/actions/runs/44501473176)
was dispatched once on that exact commit. At package verification
the Full was `in_progress`; no required job failure was reported at that point.
The ordinary source sync is not a Full result.

Use only the ZIP in [windows-verified.json](windows-verified.json):

- Path on A: `C:\Users\leadb\AppData\Local\NOVOVM\candidate-common-654b9991-20261119\novovm-common-654b9991-windows-x86_64.zip`.
- Length: `162894278` bytes.
- ZIP SHA256: `62be16398634206c7e460c1ac4d62d1605f474f9590fa23e1a3da6b298d45709`.
- ZIP: all 94 file members matched; 91 manifest and 40 AOEM checksums matched.
- Packaged ML-DSA-65 runtime verification and isolated cold recovery
  using the packaged node and AOEM both passed. This is not live acceptance.

D may copy and independently verify the ZIP now. Do not install until
the exact Full succeeds and the rollout for this version is cleared.
Keep the current A/D nodes and their original databases, keys and state.

The fix queues one early authenticated time reply per peer on the original
bounded owner without extending the original deadlines. On live follow-up,
update A/D by their original LocalService `existing` entry and verify
actual loaded hashes, original state, ticks and authenticated Status.
Then send just one new Iroh object after D is observing. Correlate actual
carrier/session, `reply_deferred`, sender DONE, receiver fragment integrity
and final receiver time acceptance. Do not stop B, resend the old object
or repeat the already verified NovoRUDP return.
