//! Authenticated single-owner TLS/WS transport. No protocol admission, queue
//! selection or authority lives here; an in-progress delivery keeps its guard.
use super::*;
use crate::duplex::product_relay::RelayEncodedDeliveryV1;

const QUANTUM: usize = 16 * 1024;
const TLS_WRITE_BUFFER: usize = 64 * 1024;
const MAX_READ_BUFFER: usize = PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1 + 14 + QUANTUM;
const MAX_READ_STAMPS: usize = MAX_READ_BUFFER.div_ceil(QUANTUM) + 1;

type ServerStream =
    rustls::StreamOwned<rustls::ServerConnection, ProductRelayDaemonDeadlineTcpStreamV1>;

// Track record boundaries from the very first (handshake) socket read. Rustls
// does not expose an incomplete encrypted record: a completed preceding WS
// frame must not clear that next record's original arrival deadline.
#[derive(Default)]
pub(super) struct InboundTlsRecords {
    header: [u8; 5],
    header_bytes: usize,
    remaining: usize,
    pending: Option<Instant>,
    observed: Option<Instant>,
}

impl InboundTlsRecords {
    pub(super) fn observe(&mut self, mut bytes: &[u8], started: Instant) -> io::Result<()> {
        self.observed = None;
        while !bytes.is_empty() {
            if self.pending.is_none() {
                self.pending = Some(
                    started
                        .checked_add(Duration::from_millis(PRODUCT_RELAY_FRAME_DEADLINE_MS_V1))
                        .ok_or_else(|| io::Error::other("server TLS record deadline overflow"))?,
                );
            }
            self.observed = earliest(self.observed, self.pending);
            if self.header_bytes < self.header.len() {
                let count = bytes.len().min(self.header.len() - self.header_bytes);
                self.header[self.header_bytes..self.header_bytes + count]
                    .copy_from_slice(&bytes[..count]);
                self.header_bytes += count;
                bytes = &bytes[count..];
                if self.header_bytes < self.header.len() {
                    break;
                }
                self.remaining = u16::from_be_bytes([self.header[3], self.header[4]]) as usize;
            }
            let count = bytes.len().min(self.remaining);
            self.remaining -= count;
            bytes = &bytes[count..];
            if self.remaining == 0 {
                self.header_bytes = 0;
                self.pending = None;
            }
        }
        Ok(())
    }

    pub(super) fn pending_deadline(&self) -> Option<Instant> {
        self.pending
    }
}

pub(super) struct ServerPumpProgress {
    pub(super) frame: Option<(WebSocketFrameV1, Instant)>,
    pub(super) delivery_finished: bool,
    pub(super) progressed: bool,
}

impl ServerPumpProgress {
    fn checked_frame_deadline(self, now: Instant) -> Result<Self> {
        // Taking a frame advances the pump's read timer to its successor. Its
        // own deadline must still hold after the independent write quantum.
        if self
            .frame
            .as_ref()
            .is_some_and(|(_, deadline)| now >= *deadline)
        {
            bail!("server inbound frame deadline exceeded before dispatch");
        }
        Ok(self)
    }
}

enum Payload {
    Delivery(RelayEncodedDeliveryV1),
    Control(Vec<u8>),
}

impl Payload {
    fn bytes(&self) -> &[u8] {
        match self {
            Self::Delivery(delivery) => delivery.wire_bytes(),
            Self::Control(bytes) => bytes,
        }
    }
}

struct Writing {
    header: [u8; 10],
    header_len: usize,
    offset: usize,
    // Snapshot of the FIFO ciphertext through this frame once all its
    // plaintext is accepted. Later TLS protocol output must not hold up the
    // delivery's sent-credit boundary after these bytes reached the socket.
    flush_remaining: Option<usize>,
    payload: Payload,
}

// Each stamp covers at most one plaintext quantum. Once partially consumed it
// cannot acquire new bytes, so continuous small frames cannot pin a new tail
// forever to the timestamp of an already consumed prefix.
struct ReadStamp {
    bytes: usize,
    deadline: Instant,
    sealed: bool,
}

pub(super) struct RelayServerPumpV1 {
    stream: ServerStream,
    writing: Option<Writing>,
    read_buffer: Vec<u8>,
    read_stamps: Vec<ReadStamp>,
    tls_plaintext_bytes: usize,
    tls_plaintext_deadline: Option<Instant>,
    read_deadline: Option<Instant>,
    write_deadline: Option<Instant>,
    write_stall_deadline: Option<Instant>,
    write_timeout: Option<Duration>,
    idle_timeout: Duration,
    idle: bool,
    closed: bool,
}

impl RelayServerPumpV1 {
    pub(super) fn new(mut stream: ServerStream) -> Result<Self> {
        stream.sock.deadline.check_v1()?;
        if stream.conn.is_handshaking() {
            bail!("server pump requires a completed TLS handshake");
        }
        let tls_plaintext_bytes = stream.conn.process_new_packets()?.plaintext_bytes_to_read();
        let read_ahead_deadline = stream
            .sock
            .inner
            .read_ahead_started_at()
            .map(frame_deadline)
            .transpose()?;
        let inherited = {
            let mut state = stream
                .sock
                .deadline
                .state
                .lock()
                .map_err(|_| anyhow::anyhow!("relay deadline lock poisoned"))?;
            stream.sock.deadline.check_state_v1(&mut state)?;
            let inherited = state.deadline;
            // This owner now tracks independent directions. The old object
            // remains the shared shutdown/permanent-error latch, not a second
            // mutable operation timer that can clear another direction.
            state.deadline = None;
            state.buffered_read_deadline = None;
            state.lower_read_progressed = false;
            inherited
        };
        let inherited_read = if tls_plaintext_bytes != 0 || read_ahead_deadline.is_some() {
            earliest(inherited, read_ahead_deadline).or(Some(frame_deadline(Instant::now())?))
        } else {
            None
        };
        let inherited_read = earliest(
            inherited_read,
            stream.sock.inbound_tls_records.pending_deadline(),
        );
        let write_timeout = stream.sock.inner.write_timeout()?;
        let idle_timeout = stream
            .sock
            .inner
            .read_timeout()?
            .unwrap_or(Duration::from_millis(100));
        let write_deadline = if stream.conn.wants_write() {
            Some(inherited.unwrap_or(frame_deadline(Instant::now())?))
        } else {
            None
        };
        // Rustls may add bounded record overhead beyond this plaintext budget.
        stream.conn.set_buffer_limit(Some(TLS_WRITE_BUFFER));
        Ok(Self {
            stream,
            writing: None,
            read_buffer: Vec::new(),
            read_stamps: Vec::new(),
            tls_plaintext_bytes,
            tls_plaintext_deadline: inherited_read,
            read_deadline: inherited_read,
            write_deadline,
            write_stall_deadline: None,
            write_timeout,
            idle_timeout,
            idle: false,
            closed: false,
        })
    }

    pub(super) fn read_waker(&self) -> std::task::Waker {
        self.stream.sock.inner.read_waker()
    }

    pub(super) fn can_write(&self) -> bool {
        !self.closed
            && self.writing.is_none()
            && self.write_deadline.is_none()
            && !self.stream.conn.wants_write()
    }

    pub(super) fn start_delivery(&mut self, delivery: RelayEncodedDeliveryV1) -> Result<()> {
        let result = frame_deadline(Instant::now())
            .and_then(|deadline| self.start(0x2, Payload::Delivery(delivery), deadline));
        self.terminal(result)
    }

    pub(super) fn start_control(
        &mut self,
        opcode: u8,
        payload: Vec<u8>,
        deadline: Instant,
    ) -> Result<()> {
        let result = self.start(opcode, Payload::Control(payload), deadline);
        self.terminal(result)
    }

    fn start(&mut self, opcode: u8, payload: Payload, deadline: Instant) -> Result<()> {
        self.check_deadlines()?;
        if !self.can_write() {
            bail!("server pump already owns an unfinished write");
        }
        if Instant::now() >= deadline {
            bail!("server response deadline expired before send");
        }
        if !matches!(opcode, 0x2 | 0x8 | 0x9 | 0xA) {
            bail!("unsupported server WebSocket opcode");
        }
        let size = payload.bytes().len();
        validate_websocket_payload_size_v1(opcode, size, PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1)?;
        let mut header = [0; 10];
        header[0] = 0x80 | opcode;
        let header_len = match size {
            0..=125 => {
                header[1] = size as u8;
                2
            }
            126..=65_535 => {
                header[1] = 126;
                header[2..4].copy_from_slice(&(size as u16).to_be_bytes());
                4
            }
            _ => {
                header[1] = 127;
                header[2..].copy_from_slice(&(size as u64).to_be_bytes());
                10
            }
        };
        self.writing = Some(Writing {
            header,
            header_len,
            offset: 0,
            flush_remaining: None,
            payload,
        });
        self.write_deadline = Some(deadline);
        self.idle = false;
        Ok(())
    }

    pub(super) fn poll(&mut self) -> Result<ServerPumpProgress> {
        self.idle = false;
        let result = self.poll_inner();
        let result = self.terminal(result)?;
        self.idle = !result.progressed;
        Ok(result)
    }

    fn poll_inner(&mut self) -> Result<ServerPumpProgress> {
        self.check_deadlines()?;
        if self.closed {
            bail!("server pump is closed");
        }
        let mut progressed = false;
        let mut frame = self.take_frame()?;
        let mut budget = QUANTUM;
        if frame.is_none() {
            progressed |= self.read_plaintext(&mut budget)?;
            frame = self.take_frame()?;
        }
        if frame.is_none() && self.stream.conn.wants_read() {
            let read_started = self
                .stream
                .sock
                .inner
                .read_ahead_started_at()
                .unwrap_or_else(Instant::now);
            let result = self
                .stream
                .conn
                .read_tls(&mut IncrementalSocket(&mut self.stream.sock));
            match result {
                Ok(count) => {
                    progressed |= count != 0;
                    let deadline = self
                        .stream
                        .sock
                        .inbound_tls_records
                        .observed
                        .unwrap_or(frame_deadline(read_started)?);
                    if count != 0 {
                        self.read_deadline = earliest(self.read_deadline, Some(deadline));
                    }
                    self.tls_plaintext_bytes = self
                        .stream
                        .conn
                        .process_new_packets()?
                        .plaintext_bytes_to_read();
                    if self.tls_plaintext_bytes != 0 {
                        self.tls_plaintext_deadline = Some(deadline);
                    }
                    self.check_deadlines()?;
                    // A complete TLS-only control record (for example a key
                    // update) is not the start of an unfinished WS frame.
                    // Clear only a genuinely empty inbound direction; never
                    // a partial plaintext frame, encrypted record, or prefetch.
                    if self.read_buffer.is_empty()
                        && self.tls_plaintext_bytes == 0
                        && self
                            .stream
                            .sock
                            .inbound_tls_records
                            .pending_deadline()
                            .is_none()
                        && self.stream.sock.inner.read_ahead_started_at().is_none()
                    {
                        self.read_deadline = None;
                        self.tls_plaintext_deadline = None;
                    }
                    progressed |= self.read_plaintext(&mut budget)?;
                    frame = self.take_frame()?;
                    if count == 0 && frame.is_none() {
                        bail!("relay server TLS stream closed");
                    }
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) => {}
                Err(error) => return Err(error.into()),
            }
        }
        if frame
            .as_ref()
            .is_some_and(|(frame, _)| matches!(frame, WebSocketFrameV1::Close))
        {
            self.closed = true;
        }
        let delivery_finished = if self.closed {
            false
        } else {
            let (written, delivery_finished) = self.write_step()?;
            progressed |= written;
            delivery_finished
        };
        self.check_deadlines()?;
        progressed |= frame.is_some() || delivery_finished;
        ServerPumpProgress {
            frame,
            delivery_finished,
            progressed,
        }
        .checked_frame_deadline(Instant::now())
    }

    fn read_plaintext(&mut self, budget: &mut usize) -> Result<bool> {
        if *budget == 0 {
            return Ok(false);
        }
        let mut bytes = [0; QUANTUM];
        match self.stream.conn.reader().read(&mut bytes[..*budget]) {
            Ok(0) => bail!("relay server TLS close_notify before WebSocket close"),
            Ok(count) => {
                let deadline = self
                    .tls_plaintext_deadline
                    .unwrap_or(frame_deadline(Instant::now())?);
                self.append_plaintext(&bytes[..count], deadline)?;
                self.read_deadline = earliest(self.read_deadline, Some(deadline));
                self.tls_plaintext_bytes = self.tls_plaintext_bytes.saturating_sub(count);
                if self.tls_plaintext_bytes == 0 {
                    self.tls_plaintext_deadline = None;
                }
                *budget -= count;
                Ok(true)
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(false),
            Err(error) => Err(error.into()),
        }
    }

    fn append_plaintext(&mut self, bytes: &[u8], deadline: Instant) -> Result<()> {
        let next = self
            .read_buffer
            .len()
            .checked_add(bytes.len())
            .context("server read buffer overflow")?;
        if next > MAX_READ_BUFFER {
            bail!("server read buffer limit exceeded");
        }
        if next > self.read_buffer.capacity() {
            self.read_buffer
                .try_reserve_exact(next - self.read_buffer.len())?;
        }
        let mut remaining = bytes.len();
        if let Some(last) = self.read_stamps.last_mut().filter(|last| !last.sealed) {
            let count = remaining.min(QUANTUM - last.bytes);
            if count != 0 {
                last.bytes += count;
                last.deadline = last.deadline.min(deadline);
                remaining -= count;
            }
        }
        while remaining != 0 {
            if self.read_stamps.len() == MAX_READ_STAMPS {
                bail!("server read timestamp limit exceeded");
            }
            self.read_stamps.try_reserve_exact(1)?;
            let count = remaining.min(QUANTUM);
            self.read_stamps.push(ReadStamp {
                bytes: count,
                deadline,
                sealed: false,
            });
            remaining -= count;
        }
        self.read_buffer.extend_from_slice(bytes);
        Ok(())
    }

    fn take_frame(&mut self) -> Result<Option<(WebSocketFrameV1, Instant)>> {
        let Some(length) = complete_masked_frame_len(&self.read_buffer)? else {
            return Ok(None);
        };
        let deadline = self
            .read_deadline
            .context("server frame has no original deadline")?;
        if Instant::now() >= deadline {
            bail!("server inbound frame deadline exceeded");
        }
        // The original parser owns the final protocol decisions. This slice
        // contains the entire frame, so its Read cannot block or cross into I/O.
        let frame = read_websocket_frame_with_guard_v1(
            &mut &self.read_buffer[..length],
            true,
            PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1,
            None,
        )?;
        self.read_buffer.drain(..length);
        let mut consumed = length;
        while consumed != 0 {
            let first = self
                .read_stamps
                .first_mut()
                .context("server frame timestamp missing")?;
            let count = consumed.min(first.bytes);
            first.bytes -= count;
            first.sealed = true;
            consumed -= count;
            if first.bytes == 0 {
                self.read_stamps.remove(0);
            }
        }
        self.read_deadline = earliest(
            self.read_stamps.first().map(|stamp| stamp.deadline),
            if self.tls_plaintext_bytes != 0 {
                self.tls_plaintext_deadline
            } else {
                None
            },
        );
        self.read_deadline = earliest(
            self.read_deadline,
            self.stream
                .sock
                .inner
                .read_ahead_started_at()
                .map(frame_deadline)
                .transpose()?,
        );
        self.read_deadline = earliest(
            self.read_deadline,
            self.stream.sock.inbound_tls_records.pending_deadline(),
        );
        // Parsing/unmasking and consuming the buffered frame may cross its
        // deadline. The new read timer no longer covers this consumed frame.
        if Instant::now() >= deadline {
            bail!("server inbound frame deadline exceeded after parsing");
        }
        Ok(Some((frame, deadline)))
    }

    fn write_step(&mut self) -> Result<(bool, bool)> {
        let mut progressed = false;
        let mut socket_progressed = false;
        if let Some(writing) = &mut self.writing {
            let mut budget = QUANTUM;
            if writing.offset < writing.header_len {
                let count = self
                    .stream
                    .conn
                    .writer()
                    .write(&writing.header[writing.offset..writing.header_len])?;
                writing.offset += count;
                budget -= count;
                progressed |= count != 0;
            }
            if writing.offset >= writing.header_len && budget != 0 {
                let offset = writing.offset - writing.header_len;
                let bytes = writing.payload.bytes();
                let end = offset.saturating_add(budget).min(bytes.len());
                if offset < end {
                    let count = self.stream.conn.writer().write(&bytes[offset..end])?;
                    writing.offset += count;
                    progressed |= count != 0;
                }
            }
            if writing.flush_remaining.is_none()
                && writing.offset == writing.header_len + writing.payload.bytes().len()
            {
                writing.flush_remaining =
                    Some(self.stream.conn.process_new_packets()?.tls_bytes_to_write());
            }
        }
        if self.stream.conn.wants_write() {
            if self.write_deadline.is_none() {
                self.write_deadline = Some(frame_deadline(Instant::now())?);
            }
            match self
                .stream
                .conn
                .write_tls(&mut IncrementalSocket(&mut self.stream.sock))
            {
                Ok(0) => bail!("server TLS write made zero progress"),
                Ok(count) => {
                    if let Some(remaining) = self
                        .writing
                        .as_mut()
                        .and_then(|writing| writing.flush_remaining.as_mut())
                    {
                        *remaining = remaining.saturating_sub(count);
                    }
                    socket_progressed = true;
                    progressed = true;
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    if self.write_stall_deadline.is_none() {
                        self.write_stall_deadline = self
                            .write_timeout
                            .map(|timeout| {
                                Instant::now()
                                    .checked_add(timeout)
                                    .context("server write stall overflow")
                            })
                            .transpose()?;
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => return Err(error.into()),
            }
        }
        // Consume actual successful socket counts in rustls BEFORE checking
        // post-I/O deadlines. No failed/expired completion can release a guard
        // as successful or be retransmitted through a fresh TLS record.
        self.check_deadlines()?;
        if socket_progressed {
            self.write_stall_deadline = None;
        }
        let mut delivery_finished = false;
        if self
            .writing
            .as_ref()
            .is_some_and(|writing| writing.flush_remaining == Some(0))
        {
            let writing = self.writing.take().expect("completed server frame");
            delivery_finished = matches!(writing.payload, Payload::Delivery(_));
            drop(writing);
            progressed = true;
        }
        if !self.stream.conn.wants_write() && self.writing.is_none() {
            self.write_deadline = None;
            self.write_stall_deadline = None;
        }
        Ok((progressed, delivery_finished))
    }

    fn check_deadlines(&self) -> Result<()> {
        self.stream.sock.deadline.check_v1()?;
        if [
            self.read_deadline,
            self.write_deadline,
            self.write_stall_deadline,
            self.stream.sock.inbound_tls_records.pending_deadline(),
        ]
        .into_iter()
        .flatten()
        .any(|end| Instant::now() >= end)
        {
            bail!("server incremental read/write deadline exceeded");
        }
        Ok(())
    }

    fn terminal<T>(&mut self, result: Result<T>) -> Result<T> {
        match result {
            Ok(value) => Ok(value),
            Err(error) => {
                let _ = self.stream.sock.deadline.fail_v1(&error);
                self.writing = None;
                self.closed = true;
                Err(error)
            }
        }
    }

    pub(super) fn wait(&mut self, until: Option<Instant>) -> Result<()> {
        let result = self.wait_inner(until);
        self.terminal(result)
    }

    fn wait_inner(&mut self, until: Option<Instant>) -> Result<()> {
        self.check_deadlines()?;
        if until.is_some_and(|end| Instant::now() >= end) {
            bail!("queued server response deadline exceeded");
        }
        if !self.idle || self.closed {
            return Ok(());
        }
        let now = Instant::now();
        let timeout = [
            self.read_deadline,
            self.write_deadline,
            self.write_stall_deadline,
            self.stream.sock.inbound_tls_records.pending_deadline(),
            until,
        ]
        .into_iter()
        .flatten()
        .fold(self.idle_timeout, |timeout, end| {
            timeout.min(end.saturating_duration_since(now))
        });
        let interest = if self.stream.conn.wants_write() {
            mio::Interest::READABLE | mio::Interest::WRITABLE
        } else {
            mio::Interest::READABLE
        };
        self.stream.sock.inner.wait_ready(interest, timeout)?;
        self.check_deadlines()?;
        if until.is_some_and(|end| Instant::now() >= end) {
            bail!("queued server response deadline exceeded");
        }
        Ok(())
    }
}

fn earliest(a: Option<Instant>, b: Option<Instant>) -> Option<Instant> {
    a.into_iter().chain(b).min()
}

fn frame_deadline(start: Instant) -> Result<Instant> {
    start
        .checked_add(Duration::from_millis(PRODUCT_RELAY_FRAME_DEADLINE_MS_V1))
        .context("server frame deadline overflow")
}

fn complete_masked_frame_len(bytes: &[u8]) -> Result<Option<usize>> {
    if bytes.len() < 2 {
        return Ok(None);
    }
    if bytes[0] & 0x80 == 0 {
        bail!("fragmented WebSocket frames are not supported");
    }
    if bytes[0] & 0x70 != 0 {
        bail!("relay WebSocket RSV bits are unsupported");
    }
    if bytes[1] & 0x80 == 0 {
        bail!("relay requires masked client WebSocket frames");
    }
    let (length, header) = match bytes[1] & 0x7f {
        126 => {
            if bytes.len() < 4 {
                return Ok(None);
            }
            (u16::from_be_bytes(bytes[2..4].try_into()?) as u64, 8usize)
        }
        127 => {
            if bytes.len() < 10 {
                return Ok(None);
            }
            (u64::from_be_bytes(bytes[2..10].try_into()?), 14usize)
        }
        length => (u64::from(length), 6usize),
    };
    let length = usize::try_from(length).context("server frame length overflow")?;
    validate_websocket_payload_size_v1(
        bytes[0] & 0x0f,
        length,
        PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1,
    )?;
    let total = header
        .checked_add(length)
        .context("server frame length overflow")?;
    Ok((bytes.len() >= total).then_some(total))
}

struct IncrementalSocket<'a>(&'a mut ProductRelayDaemonDeadlineTcpStreamV1);

impl Read for IncrementalSocket<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.0.deadline.check_v1()?;
        let length = bytes.len().min(QUANTUM);
        let started = self
            .0
            .inner
            .read_ahead_started_at()
            .unwrap_or_else(Instant::now);
        let result = self.0.inner.try_read(&mut bytes[..length]);
        let maintenance = match &result {
            Ok(count) if *count != 0 => self
                .0
                .inbound_tls_records
                .observe(&bytes[..*count], started),
            _ => Ok(()),
        }
        .and_then(|()| self.0.deadline.check_v1());
        self.0.deadline.finish_progress_v1(result, maintenance)
    }
}

impl Write for IncrementalSocket<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.deadline.check_v1()?;
        #[cfg(test)]
        let result = self.test_write(bytes);
        #[cfg(not(test))]
        let result = self.0.inner.try_write(&bytes[..bytes.len().min(QUANTUM)]);
        self.0
            .deadline
            .finish_progress_v1(result, self.0.deadline.check_v1())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.deadline.check_v1()
    }
}

#[cfg(test)]
impl IncrementalSocket<'_> {
    fn test_write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.test_writes.calls += 1;
        let bytes = &bytes[..bytes.len().min(QUANTUM)];
        if self.0.test_writes.capture {
            self.0.test_writes.attempted.push(bytes.to_vec());
        }
        let fault = self.0.test_writes.fault.take();
        if matches!(
            fault,
            Some(ProductRelayDaemonTestWriteFaultV1::ZeroProgressTimeout)
        ) {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "injected incremental zero progress",
            ));
        }
        let limit = match fault {
            Some(ProductRelayDaemonTestWriteFaultV1::TimeoutAfterProgress(limit))
            | Some(ProductRelayDaemonTestWriteFaultV1::ExpireAfterProgress(limit)) => limit,
            _ => bytes.len(),
        };
        let count = self.0.inner.try_write(&bytes[..bytes.len().min(limit)])?;
        if self.0.test_writes.capture {
            self.0.test_writes.sent.push(bytes[..count].to_vec());
        }
        match fault {
            Some(ProductRelayDaemonTestWriteFaultV1::TimeoutAfterProgress(_)) => {
                Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "injected incremental indeterminate send",
                ))
            }
            Some(ProductRelayDaemonTestWriteFaultV1::ExpireAfterProgress(_)) => {
                self.0.deadline.state.lock().unwrap().deadline = Some(Instant::now());
                Ok(count)
            }
            _ => Ok(count),
        }
    }
}

#[cfg(test)]
#[path = "pump/tests.rs"]
mod tests;
