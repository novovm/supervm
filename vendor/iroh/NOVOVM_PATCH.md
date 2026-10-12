# Fixed Iroh 1.3.0 shutdown patch

Source: crates.io `iroh` 1.3.0, package checksum
`885787b892b5e2507c701f132ecbd45d2bad4bbb75157a19427087c16dadb833`.
Upstream Git revision: `0072d7d84b233f9e7185eb676f049beaf557ac03`, path `iroh`.
Original BSD-3-Clause license and source are retained. Registry installation
marker and upstream package lock are omitted. `UPSTREAM_SHA256.json` records
the original imported file hashes. Source changes are confined to
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
