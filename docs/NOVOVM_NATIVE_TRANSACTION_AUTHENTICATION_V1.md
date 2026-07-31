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
Encoder output uses wire version `2`.

`signature` is a 96-byte payload:

```text
ed25519_public_key[32] || ed25519_signature[64]
```

The signing message uses the `novovm_adapter_tx_sig_v2` domain and includes the
chain ID, transaction type, nonce, value, gas fields, signer identity, account
ownership fields, target, data, execution policy, access list, cross-chain
hints, and transaction hash.

The decoder can still parse wire version `1` so the ingress reports an explicit
legacy-authentication rejection. Version `1` authentication is never accepted
as production input.

## Host semantic operations

High-level host operations such as `nov_swap` and `nov_buyAsset` are converted
to authenticated native transactions before they enter the same hard ingress
gate.

Production hosts must configure a dedicated 32-byte Ed25519 signing seed:

```text
NOVOVM_NATIVE_HOST_SIGNING_SEED=0x<64 lowercase or uppercase hex characters>
```

The seed is a secret. It must be supplied through the machine's secret
management mechanism and must not be committed to this repository.

An externally supplied `signature` takes precedence and must already contain a
valid 96-byte authentication payload.

`NOVOVM_NATIVE_CHAIN_ID`, when configured, pins the accepted native chain
domain. A signed transaction for another chain is rejected.

## Nonce boundary

NOVOVM account nonce `0` is valid as the initial nonce.

This milestone prevents conflicting reuse of `(chain, signed nonce owner,
nonce)` for the lifetime of the running host. Host-generated semantic
transactions allocate the next available runtime nonce when the request omits
one.

Durable nonce ownership across restart remains part of the AOEM-owned state
persistence and restart-recovery milestone. The process-local reservation is
not claimed as the final durable nonce ledger.

## Code entry points

- Wire codec: `crates/novovm-protocol/src/tx_wire.rs`
- Signing message and Ed25519 verification:
  `crates/novovm-adapter-novovm/src/lib.rs`
- Hard ingress gate and host signing:
  `crates/novovm-node/src/tx_ingress.rs`
