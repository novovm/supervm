# D read-only LAN receiver collection

Handoff: `7480930a49487b0bbdcaad90e53c06758e5ab773`.
Ran the unmodified `Collect-Receiver.ps1` once in administrator PowerShell 7,
HostId D, fresh EvidenceRoot `target/d-1ef-receiver-20261110/run1`.
Completed 2026-11-10 11:39:41 Beijing, exit code 0, stderr empty.

Actual executed script SHA256 (Windows checkout bytes):
`edbb1dc9302dae5569d203e9ae4137560203791a4de5f306ad4f0125af95464c`.

## Observed result

- Same node PID 2068, LocalService; execution_failures=0,
  recovery_in_progress=false, current_height=1.
- Fixed object:
  `b379c39bf958f5f05eef55b136b22a123f05f8d2ed2b5c0583f26c0d3628c2c7`.
- One `probe_received_integrity_verified` event; receiver_verified_observed=true.
- Event session matches `147302e8176d0de12c49f4f0d9f9de3e`;
  path=native_udp, route=192.168.71.74:41051.
- `transport=[]`: zero matching transport records for the expected tag
  `0908559beee64f72a6c3605bb2e4a95f70e9fa31f27e3022fd493b267e8f3bf2`.
  Probe evicted events, transport evicted records and omitted records are all 0.
  The collection does not establish why the transport record is absent.

No resend, restart, update, key/DB read or remote C operation was performed.
Only the original collector ran against live progress; no retry was used to
alter the outcome. The script hashes the progress bytes it reads but does not
save a separate immutable full-progress snapshot; this receipt does not claim
such a snapshot was archived.

The original public JSON, stdout/stderr and execution receipt are copied
byte-for-byte here with Git text conversion disabled. SHA256SUMS.json records
their hashes. The JSON retains sender/recipient/nonce/session for A's existing
matching procedure. Exit 0 is collection success, not full double-ended
delivery acceptance, relay qualification, consensus, or a business receipt.
