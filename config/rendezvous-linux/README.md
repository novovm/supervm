# Owned native mailbox on Linux

This is the Linux adapter for `novovm-rendezvous-service`, not the legacy
`supervm-relay` daemon or a complete consensus/L2 installation. It runs the
existing native UDP, V8/TLS, self-owned identity and SQLite mailbox under one
owner. Windows custody and its migration remain supported separately.

Build on a development machine with the pinned Rust toolchain:

```sh
cargo test -p novovm-relay --bin novovm-rendezvous-service --features tls-transport -- --test-threads=1
cargo build --release -p novovm-relay --bin novovm-rendezvous-service --features tls-transport
```

The first-install helper is
[`scripts/linux/install-owned-mailbox.sh`](../../scripts/linux/install-owned-mailbox.sh).
Stage the verified Linux executable, this unit and three operator-created
public configuration files together. Do not put private keys in staging:

- `node.json`: existing version 2 owner configuration, root
  `/var/lib/novonet-mailbox`, original authorized client public keys,
  `relays: []`, `relay_only: false`, unused QUIC bind `0.0.0.0:43173`.
- `native.json`: existing version 1 native listener, bind `0.0.0.0:43171`,
  actual public advertised address, bounded connections and byte budget.
- `tls.json`: existing version 1 TLS listener, bind `0.0.0.0:43172`,
  actual public advertised address, `native_mailbox_v8: true`.

The helper refuses existing service paths/accounts rather than reprovisioning
an existing identity. It generates a host-protected wrapping credential using
systemd, runs the existing owner configuration commands under the dedicated
service user, then starts the unit. Node signing/storage/transport keys remain
inside the existing encrypted file format; Linux uses `.sealed`, Windows
retains `.dpapi`. The Linux process accepts only a private 32-byte `owner-key`
file supplied via `CREDENTIALS_DIRECTORY`. systemd may retain root ownership
and grant the service read access via an ACL: only its root-owned, read-only
memory-mounted `/run/credentials/<unit>/owner-key` copy qualifies for that
exception. Ordinary state files remain private and service-owned.
A missing or wrong credential,
corrupt state or deleted activated journal is an error, never a fresh identity.

The service has explicit resource bounds for a shared host. They are deployment
limits, not measured throughput guarantees. The helper does not change Nginx,
Docker, database services, host firewall or cloud security groups. Open only
UDP 43171 and TCP 43172 for the native public test; exposing the optional QUIC
port is not necessary for that gate.

Check the signed `self-owned-record.json`, service status, real public
authentication and business receipts. A running process or listening port is
not proof of external reachability. Capture existing application health before
and after deployment. Stop this unit if it adversely affects the shared host:

```sh
systemctl disable --now novonet-mailbox.service
```

Stopping preserves identity and ciphertext. Never delete state as a rollback.
For binary updates, keep the previous release and use a deliberate stopped-owner
switch of `current`; do not rerun the first-install helper.

Each phone still needs an explicit trusted-node update and its own external
relay isolation. Server `direct-only` alone does not disable phone relays.
Additive trust updates preserve previous node sequence/revocation history;
restoring original relay preferences must not roll back the encrypted database.
