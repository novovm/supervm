# NOVOVM Generic Native Transaction Authentication v1

Status: active ingress contract.

## Boundary

Every `nov_sendRawTransaction`, structured `nov_sendTransaction`, and native
batch item must pass authentication before it is written to the native pending
runtime.

The host rejects:

- legacy 32-byte native authentication values;
- malformed Ed25519 public keys or signatures;
- a signer that does not match the transaction `from` / `caller` / `proposer`;
- a signed chain domain that conflicts with the requested or configured chain;
- any mutation of signed account, fee-owner, nonce-owner, target, policy, data,
  nonce, gas, value, or cross-chain fields;
- a different transaction that reuses the same signed nonce-owner and nonce in
  the current runtime.

Rejected transactions are recorded as rejected observations and are not
admitted as pending work.

## Wire contract

The native postcard envelope keeps the `NNX1` magic for family detection.
Encoder output uses wire version `3`. Version `3` marks the complete-wire
signed-intent commitment and is a coordinated network upgrade boundary.

`signature` is a 96-byte payload:

```text
ed25519_public_key[32] || ed25519_signature[64]
```

The signing message uses the `novovm_adapter_tx_sig_v2` domain. Its transaction
hash derives from a domain-separated SHA-256 commitment of the complete
version-3 native wire after clearing the authentication bytes. This binds fee
policy, execution mode, privacy mode, verification mode, governance proposal
type, and all other wire intent fields in addition to the normalized adapter
fields.

The decoder can still parse version `1` only so ingress can return an explicit
legacy-authentication rejection. Version `2` is rejected by the decoder as a
version mismatch. Neither old format is accepted as production input. All four
machines must upgrade together; mixed v2/v3 transaction producers are
intentionally not compatible.

## Host semantic operations

High-level host operations such as `nov_swap` and `nov_buyAsset` are converted
to authenticated native transactions before they enter the same hard ingress
gate.

Public structured operations must supply both `nonce` and `signature`. The
signature must already contain the user's valid 96-byte authentication payload;
the host never substitutes its own signer or chooses the user's nonce. Internal
system authority, if added later, requires a separate non-RPC capability and is
not a fallback of the public user path.

`NOVOVM_NATIVE_CHAIN_ID`, when configured, pins the accepted native chain
domain. A signed transaction for another chain is rejected.

## Nonce boundary

NOVOVM account nonce `0` is valid as the initial nonce.

The account nonce is not a NovoRUDP delivery sequence. Reliable transport and
repair use the independently authenticated frame sequence; a repair frame with
no verified sequence-to-transaction mapping fails closed instead of deriving a
sequence from transaction business data.

The hard ingress keeps a process-local pending reservation for `(chain, signed
nonce owner, nonce)`. Once an executable native transaction reaches its
committed execution-store boundary, the same key and its signed-intent ID are
atomically stored with business state and the receipt. It is included in the
semantic state root and the RocksDB `native_execution` shard; the production
AOEM-owned batch path also includes it in the AOEM state envelope.

After restart, an exact committed replay returns the existing receipt without
executing again. A different signed intent using the committed nonce is
rejected before pending admission. Test-only semantic fixtures may allocate a
nonce, but public signed transactions must carry their nonce explicitly.

`Transfer` and `Governance` wire kinds do not yet have a generic execution
request in this pipeline. Authenticated ingress therefore rejects both before
pending admission; this prevents an unsupported item from repeatedly poisoning
an otherwise executable batch. They can be enabled only after they share the
same AOEM state, receipt, nonce, and durable-finalize boundary as `Execute`.

## Code entry points

- Wire codec: `crates/novovm-protocol/src/tx_wire.rs`
- Signing message and Ed25519 verification:
  `crates/novovm-adapter-novovm/src/lib.rs`
- Hard ingress gate and test/client signing helper:
  `crates/novovm-node/src/tx_ingress.rs`
