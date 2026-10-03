//! Locally migrated from legacy/supervm-20261002/crates/novovm-node/src/product_relay_io.rs.
//! Transport only: relay admission is not consensus finality or execution validity.
//!
//! Relay socket I/O with bounded readiness waits and queue-driven read wakeups.
//!
//! Winsock documents a connection as indeterminate after SO_RCVTIMEO or
//! SO_SNDTIMEO expires. Both platforms use nonblocking Mio I/O and wait for
//! readiness instead. Expiration of that wait, or a queued-work notification,
//! is recoverable read idle; an actual Windows socket timeout is terminal.
//! Higher layers retain handshake/frame/operation deadlines and partial-
//! progress accounting. A read wake never interrupts a socket write.

use std::{
    io::{self, Read, Write},
    net::TcpStream,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    task::{Wake, Waker},
    time::{Duration, Instant},
};

const SOCKET_TOKEN: mio::Token = mio::Token(0);
const READ_WAKE_TOKEN: mio::Token = mio::Token(1);
// One bounded maximum wire message plus TLS record overhead, only enabled
// after authentication and flow negotiation. This is an additional transport
// byte budget, not an increase to relay or decrypted client queues.
const MAX_READ_AHEAD_BYTES: usize =
    crate::duplex::product_relay::PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1 + 64 * 1024;
const READ_AHEAD_CHUNK_BYTES: usize = 16 * 1024;
const MAX_READ_AHEAD_CHUNKS: usize = MAX_READ_AHEAD_BYTES / READ_AHEAD_CHUNK_BYTES;

#[derive(Debug)]
struct ReadAheadChunk {
    bytes: Vec<u8>,
    consumed: usize,
    started_at: Instant,
}

#[derive(Debug, Default)]
struct ReadAhead {
    chunks: Vec<ReadAheadChunk>,
    eof: bool,
    enabled: bool,
}

impl ReadAhead {
    fn buffered_len(&self) -> usize {
        self.chunks
            .iter()
            .map(|chunk| chunk.bytes.len() - chunk.consumed)
            .sum()
    }

    fn started_at(&self) -> Option<Instant> {
        self.chunks.first().map(|chunk| chunk.started_at)
    }

    fn tail_room(&self) -> usize {
        self.chunks
            .last()
            .filter(|chunk| chunk.consumed == 0)
            .map_or(0, |chunk| READ_AHEAD_CHUNK_BYTES - chunk.bytes.len())
    }

    fn can_prefetch(&self) -> bool {
        self.enabled
            && !self.eof
            && (self.tail_room() != 0 || self.chunks.len() < MAX_READ_AHEAD_CHUNKS)
    }

    fn take(&mut self, output: &mut [u8]) -> usize {
        let mut taken = 0;
        while taken < output.len() {
            let Some(chunk) = self.chunks.first_mut() else {
                break;
            };
            let count = (output.len() - taken).min(chunk.bytes.len() - chunk.consumed);
            output[taken..taken + count]
                .copy_from_slice(&chunk.bytes[chunk.consumed..chunk.consumed + count]);
            chunk.consumed += count;
            taken += count;
            if chunk.consumed == chunk.bytes.len() {
                // At most 68 small metadata entries; no byte-buffer copying.
                self.chunks.remove(0);
            }
        }
        taken
    }

    fn prefetch(&mut self, socket: &mut mio::net::TcpStream) -> io::Result<bool> {
        debug_assert!(self.can_prefetch());
        let mut buffer = [0; READ_AHEAD_CHUNK_BYTES];
        let tail_room = self.tail_room();
        let count = if tail_room == 0 {
            buffer.len()
        } else {
            tail_room
        };
        let started_at = Instant::now();
        match socket.read(&mut buffer[..count]) {
            Ok(0) => {
                self.eof = true;
                Ok(true)
            }
            Ok(count) => {
                if tail_room == 0 {
                    let mut bytes = Vec::new();
                    bytes
                        .try_reserve_exact(READ_AHEAD_CHUNK_BYTES)
                        .map_err(|_| io::Error::other("relay read-ahead allocation failed"))?;
                    self.chunks.try_reserve_exact(1).map_err(|_| {
                        io::Error::other("relay read-ahead metadata allocation failed")
                    })?;
                    self.chunks.push(ReadAheadChunk {
                        bytes,
                        consumed: 0,
                        started_at,
                    });
                }
                // The tail's timestamp is immutable. Once partially consumed,
                // it is sealed against further appends so a steady tiny stream
                // cannot pin future bytes indefinitely to an already-read
                // prefix. Both allocated bytes and metadata count are bounded.
                self.chunks
                    .last_mut()
                    .expect("new or existing read-ahead chunk")
                    .bytes
                    .extend_from_slice(&buffer[..count]);
                debug_assert!(self.chunks.len() <= MAX_READ_AHEAD_CHUNKS);
                debug_assert!(self
                    .chunks
                    .iter()
                    .all(|chunk| chunk.bytes.capacity() <= READ_AHEAD_CHUNK_BYTES));
                debug_assert!(self.buffered_len() <= MAX_READ_AHEAD_BYTES);
                Ok(true)
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(false),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => Ok(true),
            Err(error) => Err(terminal_socket_timeout(error)),
        }
    }
}

#[derive(Debug)]
struct ReadWake {
    wake: mio::Waker,
    pending: AtomicBool,
}

impl Wake for ReadWake {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        // Publish before notifying Poll. Mio may coalesce readiness, so the
        // durable flag, not an event count, owns the scheduling obligation.
        // Wakes during writes stay pending even if that write consumes the
        // readiness event. A later read observes the flag before parking.
        if !self.pending.swap(true, Ordering::Release) {
            let _ = self.wake.wake();
        }
    }
}

#[derive(Debug)]
#[cfg(test)]
pub(crate) struct ReadWaitProbe {
    pub(crate) polling: AtomicBool,
    pub(crate) entries: std::sync::atomic::AtomicUsize,
}

#[derive(Debug)]
pub(crate) struct ProductRelaySocketV1 {
    inner: mio::net::TcpStream,
    poll: mio::Poll,
    events: mio::Events,
    read_wake: Arc<ReadWake>,
    read_ahead: ReadAhead,
    read_timeout: Option<Duration>,
    write_timeout: Option<Duration>,
    incremental_read_blocked: bool,
    #[cfg(test)]
    read_wait_probe: Arc<ReadWaitProbe>,
    #[cfg(test)]
    write_would_block_probe: Arc<std::sync::atomic::AtomicUsize>,
}

impl ProductRelaySocketV1 {
    pub(crate) fn new(stream: TcpStream) -> io::Result<Self> {
        // Small consensus/relay replies must not wait for an unrelated TCP
        // acknowledgement before their remaining TLS records can be sent.
        // Apply this to both outgoing and accepted streams, not only fixtures.
        // Application framing, backpressure and operation deadlines are intact.
        stream.set_nodelay(true)?;
        // Preserve caller budgets in userspace; kernel blocking timeouts must
        // not own the wait on either platform, because queue work can wake it.
        let read_timeout = stream.read_timeout()?;
        let write_timeout = stream.write_timeout()?;
        stream.set_read_timeout(None)?;
        stream.set_write_timeout(None)?;
        stream.set_nonblocking(true)?;
        let mut inner = mio::net::TcpStream::from_std(stream);
        let poll = mio::Poll::new()?;
        poll.registry()
            .register(&mut inner, SOCKET_TOKEN, mio::Interest::READABLE)?;
        let read_wake = Arc::new(ReadWake {
            wake: mio::Waker::new(poll.registry(), READ_WAKE_TOKEN)?,
            pending: AtomicBool::new(false),
        });
        Ok(Self {
            inner,
            poll,
            events: mio::Events::with_capacity(4),
            read_wake,
            read_ahead: ReadAhead::default(),
            read_timeout,
            write_timeout,
            incremental_read_blocked: false,
            #[cfg(test)]
            read_wait_probe: Arc::new(ReadWaitProbe {
                polling: AtomicBool::new(false),
                entries: std::sync::atomic::AtomicUsize::new(0),
            }),
            #[cfg(test)]
            write_would_block_probe: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        })
    }

    pub(crate) fn read_waker(&self) -> Waker {
        Waker::from(Arc::clone(&self.read_wake))
    }

    #[cfg(test)]
    pub(crate) fn read_wait_probe(&self) -> Arc<ReadWaitProbe> {
        Arc::clone(&self.read_wait_probe)
    }

    #[cfg(test)]
    pub(crate) fn write_would_block_probe(&self) -> Arc<std::sync::atomic::AtomicUsize> {
        Arc::clone(&self.write_would_block_probe)
    }

    #[cfg(test)]
    pub(crate) fn set_test_buffer_sizes(&self, send: usize, receive: usize) -> io::Result<()> {
        let socket = socket2::SockRef::from(&self.inner);
        socket.set_send_buffer_size(send)?;
        socket.set_recv_buffer_size(receive)
    }

    pub(crate) fn enable_duplex_read_ahead(&mut self) {
        self.read_ahead.enabled = true;
    }

    pub(crate) fn read_ahead_started_at(&self) -> Option<Instant> {
        self.read_ahead.started_at()
    }

    /// One real nonblocking read for an incremental TLS owner. Unlike `Read`,
    /// this never waits; the same owner calls `wait_ready` only after both
    /// directions have run out of work. Existing prefetched bytes retain FIFO.
    pub(crate) fn try_read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        let count = self.read_ahead.take(output);
        let result = if count != 0 {
            Ok(count)
        } else if self.read_ahead.eof {
            Ok(0)
        } else {
            self.inner.read(output).map_err(terminal_socket_timeout)
        };
        self.incremental_read_blocked = result
            .as_ref()
            .is_err_and(|error| error.kind() == io::ErrorKind::WouldBlock);
        result
    }

    /// One write, without waiting or secretly reading into another byte queue.
    pub(crate) fn try_write(&mut self, input: &[u8]) -> io::Result<usize> {
        let result = self.inner.write(input).map_err(terminal_socket_timeout);
        #[cfg(test)]
        if result
            .as_ref()
            .is_err_and(|error| error.kind() == io::ErrorKind::WouldBlock)
        {
            self.write_would_block_probe.fetch_add(1, Ordering::Release);
        }
        result
    }

    pub(crate) fn wait_ready(
        &mut self,
        interest: mio::Interest,
        timeout: Duration,
    ) -> io::Result<()> {
        if interest.is_readable() {
            if self.read_ahead.buffered_len() != 0 {
                return Ok(());
            }
            if self.incremental_read_blocked
                && self.read_wake.pending.swap(false, Ordering::Acquire)
            {
                return Ok(());
            }
        }
        self.poll
            .registry()
            .reregister(&mut self.inner, SOCKET_TOKEN, interest)
            .map_err(terminal_socket_timeout)?;
        #[cfg(test)]
        if interest.is_readable() && self.incremental_read_blocked {
            self.read_wait_probe.entries.fetch_add(1, Ordering::Release);
            self.read_wait_probe.polling.store(true, Ordering::Release);
        }
        let result = self.poll.poll(&mut self.events, Some(timeout));
        #[cfg(test)]
        self.read_wait_probe.polling.store(false, Ordering::Release);
        match result {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => Ok(()),
            Err(error) => Err(terminal_socket_timeout(error)),
        }
    }

    pub(crate) fn read_timeout(&self) -> io::Result<Option<Duration>> {
        Ok(self.read_timeout)
    }

    pub(crate) fn write_timeout(&self) -> io::Result<Option<Duration>> {
        Ok(self.write_timeout)
    }

    pub(crate) fn set_read_timeout(&mut self, timeout: Option<Duration>) -> io::Result<()> {
        validate_timeout(timeout)?;
        self.read_timeout = timeout;
        Ok(())
    }

    pub(crate) fn set_write_timeout(&mut self, timeout: Option<Duration>) -> io::Result<()> {
        validate_timeout(timeout)?;
        self.write_timeout = timeout;
        Ok(())
    }

    fn perform_io(
        &mut self,
        interest: mio::Interest,
        timeout: Option<Duration>,
        mut operation: impl FnMut(&mut mio::net::TcpStream) -> io::Result<usize>,
    ) -> io::Result<usize> {
        let deadline = timeout
            .map(|duration| {
                Instant::now().checked_add(duration).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "relay socket timeout overflow")
                })
            })
            .transpose()?;
        // Register only the active direction: an idle read must not spin on a
        // continuously writable socket. All data I/O goes through Mio, whose
        // Windows do_io re-arms readiness on WouldBlock.
        self.poll
            .registry()
            .reregister(&mut self.inner, SOCKET_TOKEN, interest)
            .map_err(terminal_socket_timeout)?;
        let mut registered_interest = interest;
        loop {
            remaining_timeout(deadline)?;
            match operation(&mut self.inner) {
                // Never replace successful progress with a post-I/O error.
                Ok(count) => return Ok(count),
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    #[cfg(test)]
                    if interest.is_writable() {
                        self.write_would_block_probe.fetch_add(1, Ordering::Release);
                    }
                }
                Err(error) => return Err(terminal_socket_timeout(error)),
            }
            if interest.is_readable() && self.read_wake.pending.swap(false, Ordering::Acquire) {
                // Only an actual WouldBlock can yield to queued work. Do not
                // overwrite successful bytes, EOF, or a terminal socket error.
                // Returning idle preserves higher-level partial-frame state.
                return Err(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "relay socket read woken for queued work",
                ));
            }
            let wait_interest = if interest.is_writable() && self.read_ahead.can_prefetch() {
                remaining_timeout(deadline)?;
                // A successful write already returned above. Only genuine
                // write backpressure may drive bounded opposite-direction
                // progress; a read error here cannot overwrite written bytes.
                if self.read_ahead.prefetch(&mut self.inner)? {
                    continue;
                }
                mio::Interest::READABLE | mio::Interest::WRITABLE
            } else {
                // Full buffers and latched EOF must not keep READABLE armed:
                // unread data/EOF would otherwise continuously wake Poll.
                interest
            };
            if wait_interest != registered_interest {
                self.poll
                    .registry()
                    .reregister(&mut self.inner, SOCKET_TOKEN, wait_interest)
                    .map_err(terminal_socket_timeout)?;
                registered_interest = wait_interest;
            }
            let remaining = remaining_timeout(deadline)?;
            // Observe a genuine socket WouldBlock and the actual Poll below;
            // the test hook does not replace I/O or pause the network owner.
            #[cfg(test)]
            if interest.is_readable() {
                self.read_wait_probe.entries.fetch_add(1, Ordering::Release);
                self.read_wait_probe.polling.store(true, Ordering::Release);
            }
            let polled = self.poll.poll(&mut self.events, remaining);
            #[cfg(test)]
            if interest.is_readable() {
                self.read_wait_probe.polling.store(false, Ordering::Release);
            }
            match polled {
                // Close/error readiness can also be spurious. Only the next
                // real socket result establishes EOF or a terminal error.
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => return Err(terminal_socket_timeout(error)),
            }
            // Empty/spurious readiness and Interrupted never reset deadline;
            // readiness is a hint, never a substitute for actual socket I/O.
        }
    }
}

fn validate_timeout(timeout: Option<Duration>) -> io::Result<()> {
    if timeout.is_some_and(|duration| duration.is_zero()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "relay socket timeout must be nonzero",
        ));
    }
    Ok(())
}

fn remaining_timeout(deadline: Option<Instant>) -> io::Result<Option<Duration>> {
    deadline
        .map(|end| {
            end.checked_duration_since(Instant::now())
                .filter(|remaining| !remaining.is_zero())
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::WouldBlock,
                        "relay socket readiness wait expired",
                    )
                })
        })
        .transpose()
}

fn terminal_socket_timeout(error: io::Error) -> io::Error {
    if cfg!(windows) && error.kind() == io::ErrorKind::TimedOut {
        // Do not retain TimedOut as an Error source: callers deliberately
        // search anyhow chains for recoverable read-idle I/O errors.
        io::Error::new(
            io::ErrorKind::ConnectionAborted,
            format!("terminal Windows relay socket timeout: {error}"),
        )
    } else {
        error
    }
}

impl Read for ProductRelaySocketV1 {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        let count = self.read_ahead.take(output);
        if count != 0 {
            return Ok(count);
        }
        if self.read_ahead.eof {
            return Ok(0);
        }
        self.perform_io(mio::Interest::READABLE, self.read_timeout, |stream| {
            stream.read(output)
        })
    }
}

impl Write for ProductRelaySocketV1 {
    fn write(&mut self, input: &[u8]) -> io::Result<usize> {
        if input.is_empty() {
            return Ok(0);
        }
        self.perform_io(mio::Interest::WRITABLE, self.write_timeout, |stream| {
            stream.write(input)
        })
    }

    fn flush(&mut self) -> io::Result<()> {
        // TCP (including Mio's wrapper) has no user-space flush buffer.
        self.inner.flush().map_err(terminal_socket_timeout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{net::TcpListener, sync::mpsc, thread, time::Instant};

    fn pair() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (server, _) = listener.accept().unwrap();
        client.set_nodelay(true).unwrap();
        server.set_nodelay(true).unwrap();
        (client, server)
    }

    fn prefetch_until(socket: &mut ProductRelaySocketV1, bytes: usize, eof: bool) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while socket.read_ahead.buffered_len() < bytes || (eof && !socket.read_ahead.eof) {
            assert!(
                Instant::now() < deadline,
                "real prefetch progress timed out"
            );
            assert!(socket.read_ahead.can_prefetch());
            if !socket.read_ahead.prefetch(&mut socket.inner).unwrap() {
                thread::yield_now();
            }
        }
    }

    #[test]
    fn read_ahead_timestamps_follow_consumed_chunks_and_do_not_refresh_old_bytes() {
        let (client, mut server) = pair();
        let mut socket = ProductRelaySocketV1::new(client).unwrap();
        assert!(!socket.read_ahead.enabled);
        assert_eq!(socket.read_ahead.chunks.capacity(), 0);
        socket.enable_duplex_read_ahead();
        server.write_all(b"first").unwrap();
        prefetch_until(&mut socket, 5, false);
        let first = socket.read_ahead_started_at().unwrap();
        server.write_all(b" same-old-chunk").unwrap();
        prefetch_until(&mut socket, 20, false);
        assert_eq!(socket.read_ahead_started_at(), Some(first));
        assert_eq!(socket.read_ahead.chunks.len(), 1);
        assert_eq!(socket.read(&mut [0; 1]).unwrap(), 1);
        // A consumed prefix seals the tail against future appends. This
        // bounds timestamp conservatism even for continually trickling data.
        server.write_all(b"later").unwrap();
        prefetch_until(&mut socket, 24, false);
        assert_eq!(socket.read_ahead.chunks.len(), 2);
        let later = socket.read_ahead.chunks[1].started_at;
        assert!(later > first);
        assert_eq!(socket.read_ahead_started_at(), Some(first));
        let mut old_remainder = [0; 19];
        socket.read_exact(&mut old_remainder).unwrap();
        assert_eq!(&old_remainder, b"irst same-old-chunk");
        assert_eq!(socket.read_ahead_started_at(), Some(later));
        server.write_all(b" tail").unwrap();
        prefetch_until(&mut socket, 10, false);
        assert_eq!(socket.read_ahead_started_at(), Some(later));
        let mut newer = [0; 10];
        socket.read_exact(&mut newer).unwrap();
        assert_eq!(&newer, b"later tail");
        assert_eq!(socket.read_ahead_started_at(), None);
        assert!(socket.read_ahead.chunks.is_empty());
    }

    #[test]
    fn buffered_read_precedes_wake_and_eof_does_not_close_write_direction() {
        let (client, mut server) = pair();
        let mut socket = ProductRelaySocketV1::new(client).unwrap();
        socket.enable_duplex_read_ahead();
        server.write_all(b"before").unwrap();
        prefetch_until(&mut socket, 6, false);
        socket.read_waker().wake();
        let mut bytes = [0; 6];
        socket.read_exact(&mut bytes).unwrap();
        assert_eq!(&bytes, b"before");
        let idle = socket.read(&mut [0]).unwrap_err();
        assert!(idle.to_string().contains("woken for queued work"));

        server.write_all(b"eof").unwrap();
        server.shutdown(std::net::Shutdown::Write).unwrap();
        prefetch_until(&mut socket, 3, true);
        assert!(!socket.read_ahead.can_prefetch());
        socket.read_waker().wake();
        let mut last = [0; 3];
        socket.read_exact(&mut last).unwrap();
        assert_eq!(&last, b"eof");
        assert_eq!(socket.read(&mut [0]).unwrap(), 0);
        assert_eq!(socket.read_ahead_started_at(), None);
        socket.write_all(b"still-write").unwrap();
        let mut reply = [0; 11];
        server.read_exact(&mut reply).unwrap();
        assert_eq!(&reply, b"still-write");
    }

    #[test]
    #[ignore = "real socket backpressure timing; run release with --include-ignored --test-threads=1"]
    fn read_ahead_byte_and_metadata_limits_stop_read_polling_at_real_backpressure() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        socket2::SockRef::from(&listener)
            .set_recv_buffer_size(4096)
            .unwrap();
        let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut server, _) = listener.accept().unwrap();
        socket2::SockRef::from(&server)
            .set_recv_buffer_size(4096)
            .unwrap();
        socket2::SockRef::from(&client)
            .set_send_buffer_size(4096)
            .unwrap();
        server
            .set_write_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut socket = ProductRelaySocketV1::new(client).unwrap();
        socket.enable_duplex_read_ahead();
        socket
            .set_write_timeout(Some(Duration::from_millis(15)))
            .unwrap();
        let output = [0x37; 16 * 1024];
        let mut blocked = false;
        for _ in 0..512 {
            match socket.write(&output) {
                Ok(count) => assert!(count > 0),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    blocked = true;
                    break;
                }
                Err(error) => panic!("unexpected socket error: {error}"),
            }
        }
        assert!(blocked, "fixture must reach real outbound backpressure");
        let incoming_len = MAX_READ_AHEAD_BYTES + READ_AHEAD_CHUNK_BYTES;
        let (release_tx, release_rx) = mpsc::channel();
        let writer = thread::spawn(move || {
            server.write_all(&vec![0x59; incoming_len]).unwrap();
            release_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        });
        let deadline = Instant::now() + Duration::from_secs(3);
        while socket.read_ahead.buffered_len() < MAX_READ_AHEAD_BYTES {
            assert!(
                Instant::now() < deadline,
                "fixture did not fill read-ahead budget"
            );
            match socket.write(&output) {
                Ok(count) => assert!(count > 0),
                Err(error) => assert_eq!(error.kind(), io::ErrorKind::WouldBlock),
            }
        }
        assert_eq!(socket.read_ahead.chunks.len(), MAX_READ_AHEAD_CHUNKS);
        assert!(socket.read_ahead.chunks.capacity() <= MAX_READ_AHEAD_CHUNKS);
        assert_eq!(
            socket
                .read_ahead
                .chunks
                .iter()
                .map(|chunk| chunk.bytes.capacity())
                .sum::<usize>(),
            MAX_READ_AHEAD_BYTES
        );
        assert!(!socket.read_ahead.can_prefetch());
        let mut attempts = 0;
        let started = Instant::now();
        let error = socket
            .perform_io(
                mio::Interest::WRITABLE,
                Some(Duration::from_millis(15)),
                |stream| {
                    attempts += 1;
                    stream.write(&output)
                },
            )
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
        assert!(
            attempts < 32,
            "full buffer must not spin on unread opposite bytes: {attempts}"
        );
        assert!(started.elapsed() < Duration::from_secs(1));
        assert_eq!(socket.read_ahead.buffered_len(), MAX_READ_AHEAD_BYTES);
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut received = vec![0; incoming_len];
        socket.read_exact(&mut received).unwrap();
        assert!(received.iter().all(|byte| *byte == 0x59));
        release_tx.send(()).unwrap();
        writer.join().unwrap();
    }

    #[test]
    fn both_production_socket_directions_disable_nagle_before_tls() {
        let (client, server) = pair();
        // Unlike other transport fixtures, start from the OS default. The
        // production adapter must set this itself for clients AND accepted
        // daemon streams; a test-only nodelay setting masks the missing path.
        client.set_nodelay(false).unwrap();
        server.set_nodelay(false).unwrap();
        let client = ProductRelaySocketV1::new(client).unwrap();
        let server = ProductRelaySocketV1::new(server).unwrap();
        assert!(client.inner.nodelay().unwrap());
        assert!(server.inner.nodelay().unwrap());
    }

    #[test]
    fn timeout_settings_and_empty_io_preserve_contract() {
        let (client, _server) = pair();
        client
            .set_read_timeout(Some(Duration::from_millis(30)))
            .unwrap();
        client
            .set_write_timeout(Some(Duration::from_millis(70)))
            .unwrap();
        let original_read_timeout = client.read_timeout().unwrap();
        let original_write_timeout = client.write_timeout().unwrap();
        let mut socket = ProductRelaySocketV1::new(client).unwrap();
        assert_eq!(socket.read_timeout().unwrap(), original_read_timeout);
        assert_eq!(socket.write_timeout().unwrap(), original_write_timeout);
        assert_eq!(socket.read(&mut []).unwrap(), 0);
        assert_eq!(socket.write(&[]).unwrap(), 0);
        socket.flush().unwrap();
        assert_eq!(
            socket
                .set_read_timeout(Some(Duration::ZERO))
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(
            socket
                .set_write_timeout(Some(Duration::ZERO))
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
        socket
            .set_read_timeout(Some(Duration::from_millis(10)))
            .unwrap();
        socket.set_write_timeout(None).unwrap();
        assert_eq!(socket.write_timeout().unwrap(), None);
        assert_eq!(
            socket.read_timeout().unwrap(),
            Some(Duration::from_millis(10))
        );
        // Inspect the handle Mio actually owns, not a duplicated Winsock
        // handle whose option state can differ from the original handle.
        let actual_socket = socket2::SockRef::from(&socket.inner);
        assert_eq!(actual_socket.read_timeout().unwrap(), None);
        assert_eq!(actual_socket.write_timeout().unwrap(), None);
    }

    #[test]
    fn repeated_idle_and_direction_switches_preserve_exact_bytes() {
        let (client, mut server) = pair();
        server
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        server
            .set_write_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let (release_tx, release_rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            for byte in 0..4u8 {
                release_rx.recv_timeout(Duration::from_secs(3)).unwrap();
                server.write_all(&[byte]).unwrap();
                let mut reply = [0];
                server.read_exact(&mut reply).unwrap();
                assert_eq!(reply, [byte + 10]);
            }
            server.shutdown(std::net::Shutdown::Write).unwrap();
        });
        let mut socket = ProductRelaySocketV1::new(client).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_millis(10)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        for byte in 0..4u8 {
            let mut data = [0];
            let started = Instant::now();
            let error = socket.read(&mut data).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
            assert!(started.elapsed() < Duration::from_secs(3));
            release_tx.send(()).unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            socket.read_exact(&mut data).unwrap();
            assert_eq!(data, [byte]);
            socket.write_all(&[byte + 10]).unwrap();
            socket
                .set_read_timeout(Some(Duration::from_millis(10)))
                .unwrap();
        }
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        assert_eq!(socket.read(&mut [0]).unwrap(), 0);
        worker.join().unwrap();
    }

    #[test]
    fn queued_bytes_are_read_before_eof() {
        let (client, mut server) = pair();
        server.write_all(b"before-eof").unwrap();
        server.shutdown(std::net::Shutdown::Write).unwrap();
        let mut socket = ProductRelaySocketV1::new(client).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        socket.read_waker().wake();
        let mut bytes = [0; 10];
        socket.read_exact(&mut bytes).unwrap();
        assert_eq!(&bytes, b"before-eof");
        assert_eq!(socket.read(&mut [0]).unwrap(), 0);
    }

    #[test]
    fn queued_work_wakes_a_real_blocked_read_without_waiting_for_timeout() {
        let (client, mut server) = pair();
        let mut socket = ProductRelaySocketV1::new(client).unwrap();
        let waker = socket.read_waker();
        let (blocked_tx, blocked_rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            blocked_rx.recv_timeout(Duration::from_secs(3)).unwrap();
            waker.wake();
        });
        let mut blocked_tx = Some(blocked_tx);
        let mut bytes = [0; 1];
        let started = Instant::now();
        let error = socket
            .perform_io(
                mio::Interest::READABLE,
                Some(Duration::from_secs(5)),
                |stream| {
                    let result = stream.read(&mut bytes);
                    if result
                        .as_ref()
                        .is_err_and(|error| error.kind() == io::ErrorKind::WouldBlock)
                    {
                        if let Some(sender) = blocked_tx.take() {
                            sender.send(()).unwrap();
                        }
                    }
                    result
                },
            )
            .unwrap_err();
        worker.join().unwrap();
        assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
        assert!(error.to_string().contains("woken for queued work"));
        assert!(started.elapsed() < Duration::from_secs(3));
        // Scheduling notification is neither EOF nor a terminal connection
        // failure. The same connection must still transfer exact bytes.
        server.write_all(b"a").unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        socket.read_exact(&mut bytes).unwrap();
        assert_eq!(&bytes, b"a");
    }

    #[test]
    fn pre_read_wakes_coalesce_without_losing_the_next_notification() {
        let (client, _server) = pair();
        let mut socket = ProductRelaySocketV1::new(client).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_millis(15)))
            .unwrap();
        let waker = socket.read_waker();
        for _ in 0..3 {
            waker.wake_by_ref();
        }
        let error = socket.read(&mut [0]).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
        assert!(error.to_string().contains("woken for queued work"));
        // Any leftover OS readiness event is merely a hint: it must not be
        // reported as another queued-work obligation or spin indefinitely.
        let error = socket.read(&mut [0]).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
        assert!(error.to_string().contains("readiness wait expired"));
        waker.wake_by_ref();
        let error = socket.read(&mut [0]).unwrap_err();
        assert!(error.to_string().contains("woken for queued work"));
    }

    #[cfg(windows)]
    #[test]
    fn actual_windows_socket_timeout_is_not_recoverable_idle() {
        let error = terminal_socket_timeout(io::Error::from_raw_os_error(10060));
        assert_eq!(error.kind(), io::ErrorKind::ConnectionAborted);
        assert!(error.to_string().contains("10060"));
        assert!(!error
            .get_ref()
            .is_some_and(|source| source.is::<io::Error>()));
        assert_eq!(
            terminal_socket_timeout(io::Error::from(io::ErrorKind::ConnectionReset)).kind(),
            io::ErrorKind::ConnectionReset
        );
    }

    #[test]
    fn fixed_budget_survives_spurious_and_interrupted_operations() {
        let (client, _server) = pair();
        let mut socket = ProductRelaySocketV1::new(client).unwrap();
        let started = Instant::now();
        let mut attempts = 0;
        let error = socket
            .perform_io(
                mio::Interest::WRITABLE,
                Some(Duration::from_millis(15)),
                |_| {
                    attempts += 1;
                    Err(io::Error::from(if attempts & 1 == 0 {
                        io::ErrorKind::WouldBlock
                    } else {
                        io::ErrorKind::Interrupted
                    }))
                },
            )
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
        assert!(attempts >= 2);
        assert!(started.elapsed() < Duration::from_secs(3));
        assert_eq!(
            remaining_timeout(Some(Instant::now())).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
    }

    #[test]
    #[ignore = "real socket duplex timing; run release with --include-ignored --test-threads=1"]
    fn simultaneous_large_socket_writes_drain_exact_opposite_bytes() {
        // Advertise small receive windows before the handshake so neither
        // direction can hide a complete large frame in loopback OS buffers.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        socket2::SockRef::from(&listener)
            .set_recv_buffer_size(4096)
            .unwrap();
        let client = socket2::Socket::new(
            socket2::Domain::IPV4,
            socket2::Type::STREAM,
            Some(socket2::Protocol::TCP),
        )
        .unwrap();
        client.set_recv_buffer_size(4096).unwrap();
        client.set_send_buffer_size(4096).unwrap();
        client
            .connect(&listener.local_addr().unwrap().into())
            .unwrap();
        let client: TcpStream = client.into();
        let (server, _) = listener.accept().unwrap();
        socket2::SockRef::from(&server)
            .set_recv_buffer_size(4096)
            .unwrap();
        socket2::SockRef::from(&server)
            .set_send_buffer_size(4096)
            .unwrap();
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let run = |stream: TcpStream, sent: u8, received: u8, barrier: Arc<std::sync::Barrier>| {
            thread::spawn(move || {
                let mut socket = ProductRelaySocketV1::new(stream).unwrap();
                socket.enable_duplex_read_ahead();
                socket
                    .set_write_timeout(Some(Duration::from_millis(100)))
                    .unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let output = vec![sent; 1024 * 1024];
                barrier.wait();
                socket.write_all(&output).unwrap();
                let mut input = vec![0; output.len()];
                socket.read_exact(&mut input).unwrap();
                assert!(
                    input.iter().all(|byte| *byte == received),
                    "opposite byte stream must be exact"
                );
            })
        };
        let left = run(client, 0x31, 0x73, Arc::clone(&barrier));
        let right = run(server, 0x73, 0x31, barrier);
        let left = left.join();
        let right = right.join();
        assert!(
            left.is_ok() && right.is_ok(),
            "simultaneous writes must both finish within unchanged per-call budget"
        );
    }

    #[test]
    fn backpressure_readiness_rearms_and_never_duplicates_progress() {
        // Fix the receive window before the handshake; limiting only the
        // sender buffer does not prevent loopback receive auto-growth from
        // accepting the entire fixture before it can exercise readiness.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        socket2::SockRef::from(&listener)
            .set_recv_buffer_size(4096)
            .unwrap();
        let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut server, _) = listener.accept().unwrap();
        socket2::SockRef::from(&server)
            .set_recv_buffer_size(4096)
            .unwrap();
        client.set_nodelay(true).unwrap();
        socket2::SockRef::from(&client)
            .set_send_buffer_size(4096)
            .unwrap();
        server
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut socket = ProductRelaySocketV1::new(client).unwrap();
        socket
            .set_write_timeout(Some(Duration::from_millis(15)))
            .unwrap();
        let payload: Vec<_> = (0..8 * 1024 * 1024usize)
            .map(|index| (index % 251) as u8)
            .collect();
        let mut sent = 0;
        let mut waited = false;
        while sent < payload.len() {
            let end = (sent + 16 * 1024).min(payload.len());
            match socket.write(&payload[sent..end]) {
                Ok(0) => panic!("zero socket write"),
                Ok(count) => sent += count,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    waited = true;
                    break;
                }
                Err(error) => panic!("unexpected write error: {error}"),
            }
        }
        assert!(waited, "fixture must reach real socket backpressure");
        assert!(sent > 0);
        // Wake while a second real write has reached backpressure. That wake
        // must not terminate the write, fabricate progress, duplicate bytes,
        // or disappear when Poll consumes its event in the write direction.
        let waker = socket.read_waker();
        let (blocked_tx, blocked_rx) = mpsc::channel();
        let (finish_tx, finish_rx) = mpsc::channel();
        let expected_len = payload.len();
        let reader = thread::spawn(move || {
            blocked_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            for _ in 0..3 {
                waker.wake_by_ref();
            }
            // Drain promptly after exercising genuine backpressure; tiny
            // windows themselves are not the throughput being tested here.
            socket2::SockRef::from(&server)
                .set_recv_buffer_size(1024 * 1024)
                .unwrap();
            let mut received = vec![0; expected_len];
            server.read_exact(&mut received).unwrap();
            // Keep the read half alive until the sender observes its pending
            // wake; otherwise real EOF would correctly take precedence.
            finish_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            received
        });
        let mut blocked_tx = Some(blocked_tx);
        while sent < payload.len() {
            let end = (sent + 16 * 1024).min(payload.len());
            let count = socket
                .perform_io(
                    mio::Interest::WRITABLE,
                    Some(Duration::from_secs(5)),
                    |stream| {
                        let result = stream.write(&payload[sent..end]);
                        if result
                            .as_ref()
                            .is_err_and(|error| error.kind() == io::ErrorKind::WouldBlock)
                        {
                            if let Some(sender) = blocked_tx.take() {
                                sender.send(()).unwrap();
                            }
                        }
                        result
                    },
                )
                .unwrap();
            assert!(count > 0);
            sent += count;
        }
        assert!(blocked_tx.is_none(), "must wake during genuine write wait");
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let error = socket.read(&mut [0]).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
        assert!(error.to_string().contains("woken for queued work"));
        finish_tx.send(()).unwrap();
        assert_eq!(reader.join().unwrap(), payload);
    }
}
