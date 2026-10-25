# KEYONE-INIT-2026-10-25 — mobile consumer handoff

NOVOCHAT machine fetched origin main `ec0b7c78`; shared implementation reviewed at `c82bbdc551f925af4bd3c395b19cebc319f00026`. No newer receipt was observed. This notice is not a recipient acknowledgment.

The existing B public key (SHA-256 `dc19759fbe9ba86a7eee718e19c345e069086d0ed21cdb5318a51a2d3ebe4953`) passed the existing UcaGenesisManifestV1 compiler and initialization verification in a host-only consumer. Modified budget under original pins was rejected. A modified public key was rejected by the consumer input guard. No node was started, no live grant installed, and no phone-signed transaction submitted. Ephemeral management keys were discarded; the generated candidate is not a deployable funding package.

Evidence: [NOVOCHAT report and boundary](https://github.com/XujueKing/NOVOCHAT/blob/main/docs/implementation/KEYONE_ACCOUNT_INIT_HANDOFF_2026-10-25.md).

## Ownership and required handoff

- SUPERVM owner: confirm external-device public-key test initialization contract; retain ownership of management grants, revocation, limits and atomic execution. Provide/identify Android-consumable packaging of the existing NativeApplicationClient (currently inside full novovm-node). Do not invent a second wire/transport or execution path.
- NOVOCHAT/KEY ONE owner: implement existing Android custody lifecycle allowing key provisioning before final genesis binding, then explicit authenticated final binding. Current owner binds before key generation; genesis commitment includes those public keys. Existing fixture binding must not be silently rewritten. Unbound keys must not sign asset intents. Preserve old sealed records and fail closed across interrupted binding.
- The packaging boundary has been identified by source review, not claimed as an Android build failure. Lifecycle changes are not yet implemented.

Next end-to-end acceptance remains B signature for the correct initialized domain, A native submission, final receipt plus matching balances/fees, recipient respend, replay/restart consistency. Authenticated query views are node observations, not standalone light-client proofs. Dynamic revocation requires its own delivered contract and test; local UI rejection is insufficient.

Isolated test NOV execution is authorized. Formal assets, production billing and replacement of chat identity/history remain prohibited. No shared production source changed in this round.

Pre-push refresh: received c4ae4f6c and its acknowledgment of NOVOCHAT 879b5c2 / NOVOKEYONE d723cea. Reviewed the new native operator CLI handoff; it reuses NativeApplicationClient and does not create authority. The compile evidence above remains on c82bbdc5 implementation. This update supersedes the earlier no-new-receipt observation; this new KEYONE-INIT notice itself is not yet acknowledged.
