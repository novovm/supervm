# Fixed Iroh 1.3.0 shutdown patch

Source: crates.io `iroh` 1.3.0, package checksum
`885787b892b5e2507c701f132ecbd45d2bad4bbb75157a19427087c16dadb833`.
Upstream Git revision: `0072d7d84b233f9e7185eb676f049beaf557ac03`, path `iroh`.
Original BSD-3-Clause license and source are retained. Registry installation
marker and upstream package lock are omitted. `UPSTREAM_SHA256.json` records
the original imported file hashes. Transport source changes are confined to
`src/socket.rs`, `src/endpoint.rs`, `src/socket/transports.rs` and
`src/socket/transports/ip.rs`; no wire, crypto, route or relay change.

The original `close()` waits for remote QUIC draining before stopping local
drivers. Its `is_closing` flag also suppresses abort after a cancelled close.
NOVOVM adds an optional remote drain deadline, then still joins the local
actor and QUIC task tracker. Actor timeout now aborts **and awaits** that actor.
A guard requests abort if any close future is cancelled. The new API returns
`Some(true)` for drained, `Some(false)` for remote drain timeout after local
completion, and `None` for a close already started by another owner. None is
not a local completion guarantee. Dropping/cancelling is not a joined barrier.

The pinned `netwatch` 0.19.3 `UdpSocket::Drop` spawns an unjoined blocking OS
socket close. Stopping the drivers alone can therefore return before the port
is released. The endpoint retains one close handle per configured IP socket,
then calls and awaits `UdpSocket::close()` after actor rebinding and QUIC
polling have stopped. This does not add sockets or change their configuration.
The same outer cleanup deadline applies to this wait. Cancellation remains a
failed/aborted close, never a local-completion result. Tests hold Tokio's sole
blocking worker to prove close cannot finish before the OS close runs, and
require immediate port reuse while endpoint/connecting handles remain alive.

The NOVOVM scope uses four seconds for remote drain inside its existing five
second total cleanup bound. It owns cleanup inline, never in a detached task.
On total timeout, cancellation or local failure it still fails closed. On a
completed forced drain it records a redacted process counter and preserves the
operation's independently verified result; it does not fabricate a receipt.
Strict bidirectional stream finish and post-operation authority remain required.

This patch does not imply all upstream tasks/resources or platforms have been
audited. Regression tests live with `iroh_transport` in `novovm-network`, use
actual QUIC and UDP port release, and include cancellation and an unreachable
peer. Re-evaluate and retire this fork when an upstream version provides the
same cancellation and bounded-drain guarantees; do not update it implicitly.


## Native dependency artifact isolation (2026-10-14)

This vendored native dependency builds only `rlib`. Upstream additionally
selects `cdylib` for its standalone wasm output (see unchanged `Cargo.toml.orig`).
NOVOVM and NOVOCHAT consume Rust APIs; their own executable/JNI/FFI leaf crates
own the deliverable ABI. This patch does not remove their shared-library output
or alter Iroh protocols. Standalone upstream wasm packaging is not provided by
this native vendor manifest; it must request its own leaf artifact explicitly.

On Windows Rust/Cargo 1.94 with two workspaces sharing a target directory,
`lib` + `cdylib` emitted `debug/deps/libiroh.rlib` without a metadata suffix.
The workspaces have different locked socket2/syn dependencies. Building the
other workspace overwrote that rlib while the first workspace's fingerprint
still pointed to its previous graph. The NOVOCHAT test linked two Tokio 1.50
artifacts with separate thread-local runtime contexts: direct Tokio UDP bind
worked, but Iroh/netwatch's `from_std` panicked with "no reactor running".
Disabling incremental compilation or clearing caches is not the correction.
Building an ordinary rlib preserves graph-specific artifact names; alternating
workspaces must retain distinct artifacts and pass real endpoint operations.
The original failure binary, map and logs remain in ignored local artifacts.

Cargo tracks the general artifact collision class in
https://github.com/rust-lang/cargo/issues/6313 . This incident is established by
our actual linker map and endpoint regression, not by assuming that issue
alone diagnoses all reactor errors. See the production tracker for validation.
