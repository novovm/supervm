# A/D Iroh time-flow Windows candidate — verify only

- Program source: `63b0ef4a5b372eaafdbf6c10a4032d5ebe0bbded`.
- [Full #44460410223](https://github.com/novovm/supervm/actions/runs/44460410223)
  was dispatched once on the exact program source. The Full was still
  running at package qualification; a source-sync success is not Full.
- Windows x86_64 ZIP on A:
  `C:\Users\leadb\AppData\Local\NOVOVM\candidate-common-63b0ef4a-20261119\novovm-common-63b0ef4a-windows-x86_64.zip`.
- ZIP length: `163086667` bytes.
- ZIP SHA256: `8556d779af584c2322a1e48d8ff4313a9909dfe2537f391ffc8e58cf277a5a89`.
- Manifest SHA256: `d45f042a6a999baa04422b08b06953d67abcb6abd3214bf1435bf5c697599771`.
- Node SHA256: `06fdd2b25e7c2e6391886debd2c59a601ef791f4dee3f1746e43688d300eca97`.
- Client SHA256: `068dd6e5deba4a48612df377c7ebb90e66c9a8b6c88357b56366c1b2b894f58e`.
- Control program SHA256: `aea58c85a444bb880a44609c7d1a2e8bdb8770cd7c1de36e2a8a782de74891f1`.
- AOEM DLL SHA256: `f9e8ded21753dfd32573de7bf66da8a785120444e810ba9fa865bfa4d639fcaa`.

The original Windows packager built from a clean checkout. The final ZIP
was opened and all 94 file members verified against staged files; all 91
release-manifest checksums and 40 AOEM checksums matched. The packaged
`novovmctl` verified native ML-DSA-65 against the packaged AOEM DLL.
The same-source local test harness
(`e0c2cb219eb9da68f730cc73f135616defa86ccaa8df0c842cbfbaae7d27e9ca`)
ran the original native cold-recovery test with the packaged node and
DLL against isolated databases: exit 0. An earlier ZIP that included
directory entries was not accepted as the file-only release archive;
the verified ZIP above is the only transfer candidate.

D can receive and independently verify this ZIP, but should not run the
updater yet. Do not install or stop any node until the exact Full is green
and the live update scope is confirmed. The older D-only update does not
authorize changing A for this candidate.

If A/D are cleared for this candidate, use each original Windows owner
and database in `existing` mode. Confirm the loaded EXE/DLL pins, PID,
identity, state root, progress, authenticated Status, and readiness for
bounded capture. For one new Iroh fallback only, record a baseline and
the transition in `protocol_time.peers[].session_flow` at both ends;
correlate the same authenticated session with carrier, object, fragment
integrity, sender DONE and receiver acceptance or rejection. See
[time-flow interpretation](../IROH-TIME-SESSION-FLOW.md). A link DONE
without D acceptance remains a failed live acceptance. No B stop, old
object replay, or repeated NovoRUDP recovery.
