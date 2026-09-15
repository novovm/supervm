# Product overlay handshake clock tolerance

KINGCLUB Android-to-Windows relay testing reached device registration and TCP connectivity but rejected the signed handshake as outside its validity window. Read-only wall-clock samples showed the phone behind the host by approximately 47–137 ms. The old response validator required responder issuance to be both no earlier than initiator issuance and no later than the initiator's current wall clock; either clock direction could therefore reject a legitimate immediate response.

`PRODUCT_OVERLAY_CLOCK_SKEW_MS_V1` permits at most 5,000 ms of future issuance and responder/initiator issuance difference. Signed expiration deadlines remain strict; reversed issuance/expiration intervals are rejected. Signature, transcript, peer identity and replay checks remain unchanged. Wire fields and protocol version are unchanged. Both mobile native builds and relay daemons need the corrected source for either direction of skew.

Validation: `cargo test --offline -p novovm-network --lib product_overlay` passed all 7 tests with exit code 0. New coverage derives working encrypted channels at both -5,000 and +5,000 ms, rejects 5,001 ms future issuance, and still rejects an offer one millisecond past expiration. Existing signature, identity, ciphertext and replay tests pass. At this commit Android rebuild and live phone acceptance remain pending.
