# Cross-project capability reuse index

Source audit: 2026-10-19. SUPERVM `1ed21cc3` (communications implementation
`07bed362`), NOVOCHAT `f807314` / Android v76, AOEM local `1bfa7037`.
These are implementation pointers, not blanket production acceptance.

| Need | Existing source / interface | Reuse boundary |
| --- | --- | --- |
| Continuous object transfer | [NativeObjectLink](../crates/novovm-network/src/novorudp/object_link.rs): `try_send_until`, `poll_with_receiver`, `poll_complete`; [sender](../crates/novovm-network/src/novorudp/object_sender.rs), [receiver](../crates/novovm-network/src/novorudp/object_receiver.rs) | Existing flow/window, fragments, repair and integrity. Do not implement another app sender/transport. `reliable_carrier_v1` applies to reliable authenticated carriers, not raw UDP. |
| Fragment integrity / gaps | [fragments](../crates/novovm-network/src/novorudp/fragments.rs), [object ACK](../crates/novovm-network/src/novorudp/object_ack.rs) | Bounded reassembly and missing ranges. Transport completion is not durable custody or device delivery. |
| Persistent carrier | [NativeCarrierPump](../crates/novovm-network/src/novorudp/carrier_io.rs): `attach_continuous_v7`, `progress`; [CarrierStream](../crates/novovm-network/src/carrier_stream.rs) | Reuse authenticated Iroh/WSS bindings and original owner. External Iroh use remains an external dependency. |
| Mailbox objects | [structured codec](../crates/novovm-network/src/mailbox_structured.rs), [native mailbox](../crates/novovm-network/src/mailbox_structured/native_session.rs), [store](../crates/novovm-network/src/mailbox_store.rs), [HPKE](../crates/novovm-network/src/mailbox_seal.rs) | Bulk v1 is explicitly advertised by mailbox v8; 2 MiB ciphertext bound, old 64 KiB path preserved. StoreReceipt, TTL, capability checks and device receipt retain separate meanings. |
| Chat attachment semantics | [object envelope](../../NOVOCHAT/crates/chat-core/src/offline_delivery/attachment_object.rs), [persisted object plan](../../NOVOCHAT/crates/chat-core/src/message_journal/attachment_object_plan.rs), [attachment store](../../NOVOCHAT/crates/chat-core/src/message_journal/attachment_store.rs) | Keep original Message ID, immutable retry plan, Merkle/file verification and final device receipt. These are chat semantics, not another network protocol. |
| Local source / history | [AttachmentIndexV1](../../NOVOCHAT/crates/chat-core/src/message_journal/attachment.rs), [document JNI](../../NOVOCHAT/crates/chat-core/src/attachment_document_jni.rs), [message owner](../../NOVOCHAT/crates/chat-core/src/message_owner.rs) | Reuse OS-granted file and journal ownership. Thin bounded batch reads can remove repeated platform/catalog work without new storage. Historical preview must remain valid after Delivered; send eligibility is a different rule. |
| Identity / routing | [UCA](../crates/novovm-adapter-api/src/unified_account.rs), [local identity](../crates/novovm-adapter-api/src/local_identity.rs), [device identity](../crates/novovm-network/src/product_identity.rs), [adaptive overlay](../crates/novovm-network/src/adaptive_overlay.rs) | Purpose-bound keys and existing route policy. Do not create another account, funding authority or scheduler; these files alone do not prove mobile UCA migration completed. |
| Cryptography / execution | [AOEM signatures](../../AOEM/crates/adapter/aoem-adapter-crypto/src/signature.rs), [batch verification](../../AOEM/crates/adapter/aoem-adapter-crypto/src/batch_verify.rs), [privacy FFI](../../AOEM/crates/ffi/aoem-ffi/src/classic_privacy_wire.rs) | Existing cryptographic/execution components; not an Android file-grant or encrypted SQLite journal API. No transaction/consensus detour for attachments. |
| Media experience | [KINGCLUB media cache/transfer](../../KINGCLUB-APP-V2/lib/src/features/messaging/data/chat_media_transfer.dart), [audio constraints](../../KINGCLUB-APP-V2/lib/src/features/messaging/data/call_audio_constraints.dart) | Read-only reference. Reuse applicable lifecycle/cache/audio experience; do not import central account, HTTP service credentials or assume media E2EE. |

## Current attachment call chain and smallest remaining seam

NOVOCHAT object envelope -> mailbox structured bulk request -> NativeObjectLink
-> NativeObjectSender/Receiver -> NativeCarrierPump -> authenticated CarrierStream.
This already exists in v76. The older v74 per-chunk mailbox description is not
the current implementation. Small text stays on its existing mailbox path.

The 1,600,067-byte, 49-chunk image is one bounded object in v76. Existing real
two-phone samples measured selection to device receipt at 4.502 s and 4.395 s;
both phones used Wi-Fi and external Iroh remained involved. These are two
samples, not P95, pure transfer time or screen-display time.

Read-only local audit found 49 document platform reads on import and 49 owner/
catalog opens for preview. The existing source descriptor/index is already
held, so import does not reopen the journal 49 times. Preview does repeat full
catalog inventory validation and per-message metadata decode. Bounded source
and journal batch adapters are the minimal reuse opportunity; no new transport
or storage engine is justified by these findings.

The same-carrier release comparison is tracked separately: continuous frames,
native object + integrity ACK, bulk persistent mailbox and legacy block mailbox.
Their differing semantic costs must remain explicit. Loopback throughput cannot
be presented as Android or public Relay performance.

## Same-carrier release comparison

2026-10-19, Windows loopback Iroh, authenticated persistent sessions, release
build, identical 1,600,067-byte payload, three sequential samples. Continuous
carrier and object runs use equivalent endpoint pairs; legacy/bulk mailbox
operations share the exact same authenticated connection and SQLite store.
Mailbox test quota: burst 256 / refill 128 requests/s for both alternatives;
this removes quota throttling from this comparison and does not change phones.

| Boundary (milliseconds) | Sample 1 | Sample 2 | Sample 3 |
| --- | ---: | ---: | ---: |
| Continuous carrier frames to receiver digest | 14.067 | 13.261 | 12.022 |
| NativeObjectLink to integrity ACK | 210.057 | 215.553 | 217.227 |
| Bulk Put to durable StoreReceipt | 214.826 | 216.004 | 212.007 |
| Bulk Fetch only | 99.993 | 94.822 | 86.823 |
| Bulk Fetch + Consume | 116.261 | 110.379 | 101.126 |
| Legacy 49 Put requests to StoreReceipts | 529.371 | 538.580 | 801.325 |
| Legacy 49 Fetch + Consume | 428.432 | 432.264 | 443.002 |

Legacy comparison uses sequential 32 KiB requests (window 1), not v74's exact
concurrent window. Carrier uses 16 KiB frames; native object retains its 768-byte
fragment profile, envelope, AEAD and integrity ACK (2,084 fragments). Therefore
these columns isolate different semantic costs; they are not interchangeable
raw QUIC, custody or product delivery measurements. Initial object harness
finished its transfers but timed out on strict stream finish after pump teardown;
the final harness uses scoped endpoint closure and all three final runs exit
successfully. The failed harness run is excluded, retained locally.

The object layer adds about 200 ms before SQLite in these samples. This supports
profiling framing/crypto/single-slot write progress if more carrier optimization
is needed; it does not prove a compulsory 1 ms sleep per frame. Increasing
storage slots cannot remove that earlier cost. No SDK production wire or timer
was changed for this comparison.

Commands and full harness retained in NOVOCHAT ignored local
`artifacts/carrier-baseline/RESULTS-2026-10-19.md`; raw evidence:
`artifacts/carrier-release-baseline.log`, `object-release-baseline.log`,
`mailbox-release-compare.log`. SDK existing tests are
`crates/novovm-network/tests/mailbox_native_carrier.rs`; core object tests are
under `src/novorudp/object_link/tests.rs`. This report is not phone throughput,
external Relay performance, crash durability certification or P95.
