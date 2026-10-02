//! A single authenticated TLS owner with bounded, genuinely incremental I/O.
//! No socket operation in `poll` or submission waits for readiness or an ACK.
//! The worker retains the original plaintext and its existing queue charge.

use super::*;
use std::collections::BTreeMap;

const MAX_FORWARDS: usize = 8;
const IO_QUANTUM: usize = 16 * 1024;
const TLS_WRITE_BUFFER_BYTES: usize = 64 * 1024;
const MAX_READ_BUFFER: usize = PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1 + 14 + IO_QUANTUM;

#[derive(Debug)]
pub(crate) enum PipelineProgress {
    Event(Box<ProductRelayClientEventV1>),
    Forward {
        ticket: u64,
        outcome: RelayForwardOutcomeV1,
    },
    Progress,
    Idle,
}

struct PendingForward {
    source: String,
    target: String,
    session: Option<[u8; 16]>,
    sequence: Option<u64>,
    wire_bytes: usize,
    // Starts only after this complete WS frame's TLS output is flushed.
    deadline: Option<Instant>,
}

impl PendingForward {
    fn matches(&self, outcome: &RelayForwardOutcomeV1) -> bool {
        self.source == outcome.source_peer_id
            && self.target == outcome.target_peer_id
            && self.session == outcome.envelope_session_id
            && self.sequence == outcome.envelope_sequence
    }
}

enum WritePurpose {
    Forward(u64),
    Credit(u64),
    Heartbeat,
    Pong,
}

struct WritingFrame {
    bytes: Vec<u8>,
    offset: usize,
    purpose: WritePurpose,
}

pub(crate) struct ProductRelayPipelineV1 {
    client: ProductRelayClientV1,
    pending: BTreeMap<u64, PendingForward>,
    next_ticket: u64,
    writing: Option<WritingFrame>,
    // A control frame cannot be inserted in the middle of an existing WS frame.
    // This is at most 64 * 125 bytes, not a second application payload queue.
    pongs: VecDeque<Vec<u8>>,
    tls_plaintext_bytes: usize,
    control_frames: usize,
    write_stall_deadline: Option<Instant>,
    idle: bool,
    closed: bool,
}

impl ProductRelayClientV1 {
    pub(crate) fn into_pipeline(mut self) -> Result<ProductRelayPipelineV1> {
        self.stream.sock.check_io_deadlines_v1()?;
        if self.stream.sock.handshake_deadline.is_some()
            || self.stream.sock.write_deadline.is_some()
            || self.stream.sock.retry_idle_reads
            || self.stream.sock.read_operation_deadline.is_some()
        {
            bail!("relay pipeline requires an idle authenticated operation boundary");
        }
        self.stream
            .conn
            .set_buffer_limit(Some(TLS_WRITE_BUFFER_BYTES));
        let tls_plaintext_bytes = self
            .stream
            .conn
            .process_new_packets()?
            .plaintext_bytes_to_read();
        Ok(ProductRelayPipelineV1 {
            client: self,
            pending: BTreeMap::new(),
            next_ticket: 0,
            writing: None,
            pongs: VecDeque::new(),
            tls_plaintext_bytes,
            control_frames: 0,
            write_stall_deadline: None,
            idle: false,
            closed: false,
        })
    }
}

impl ProductRelayPipelineV1 {
    pub(crate) fn read_waker(&self) -> std::task::Waker {
        self.client.read_waker()
    }

    #[cfg(test)]
    pub(crate) fn read_wait_probe(&self) -> Arc<crate::product_relay_io::ReadWaitProbe> {
        self.client.read_wait_probe()
    }

    pub(crate) fn can_submit(&self) -> bool {
        !self.closed
            && self.client.stream.sock.terminal_error.is_none()
            && self.writing.is_none()
            && self.client.stream.sock.write_deadline.is_none()
            && self.pongs.is_empty()
            && !self.credit_due()
            && self.pending.len() < MAX_FORWARDS
    }

    pub(crate) fn can_submit_handshake(&self) -> bool {
        self.can_submit() && !self.pending.values().any(|item| item.session.is_none())
    }

    pub(crate) fn try_submit_envelope(
        &mut self,
        envelope: SecureNovoRudpEnvelopeV1,
    ) -> Result<Option<u64>> {
        self.check_deadlines()?;
        if !self.can_submit() {
            return Ok(None);
        }
        let expected = PendingForward {
            source: envelope.sender_peer_id.clone(),
            target: envelope.recipient_peer_id.clone(),
            session: Some(envelope.session_id),
            sequence: Some(envelope.sequence),
            wire_bytes: 0,
            deadline: None,
        };
        self.submit(ProductRelayWireMessageV1::Data(envelope), expected)
    }

    pub(crate) fn try_submit_handshake(
        &mut self,
        target: impl Into<String>,
        handshake: RelayPeerHandshakeV1,
    ) -> Result<Option<u64>> {
        self.check_deadlines()?;
        if !self.can_submit_handshake() {
            return Ok(None);
        }
        let target = target.into();
        let source = match &handshake {
            RelayPeerHandshakeV1::Offer(offer) => offer.initiator_peer_id.clone(),
            RelayPeerHandshakeV1::Response(response) => response.responder_peer_id.clone(),
        };
        let expected = PendingForward {
            source,
            target: target.clone(),
            session: None,
            sequence: None,
            wire_bytes: 0,
            deadline: None,
        };
        self.submit(
            ProductRelayWireMessageV1::PeerHandshake {
                target_peer_id: target,
                handshake,
            },
            expected,
        )
    }

    fn submit(
        &mut self,
        message: ProductRelayWireMessageV1,
        mut expected: PendingForward,
    ) -> Result<Option<u64>> {
        if self.pending.values().any(|old| {
            old.source == expected.source
                && old.target == expected.target
                && old.session == expected.session
                && old.sequence == expected.sequence
        }) {
            bail!("relay forward correlation key is already in flight");
        }
        let payload = serde_json::to_vec(&message)?;
        expected.wire_bytes = payload.len();
        let bytes = encode_masked_frame_v1(0x2, &payload)?;
        let ticket = self.next_ticket;
        let next = ticket
            .checked_add(1)
            .context("relay ticket counter overflow")?;
        self.client.stream.sock.begin_authenticated_write_v1()?;
        // Install the obligation before even the first plaintext TLS write.
        self.pending.insert(ticket, expected);
        self.next_ticket = next;
        self.writing = Some(WritingFrame {
            bytes,
            offset: 0,
            purpose: WritePurpose::Forward(ticket),
        });
        self.idle = false;
        Ok(Some(ticket))
    }

    pub(crate) fn try_heartbeat(&mut self) -> Result<bool> {
        self.check_deadlines()?;
        if self.closed
            || self.writing.is_some()
            || self.client.stream.sock.write_deadline.is_some()
            || !self.pongs.is_empty()
            || self.credit_due()
            || self.client.heartbeat_health.sent_at.is_some()
        {
            return Ok(false);
        }
        self.start_control(
            ProductRelayWireMessageV1::Heartbeat,
            WritePurpose::Heartbeat,
        )?;
        self.client.heartbeat_health.sent_at = Some(Instant::now());
        Ok(true)
    }

    pub(crate) fn poll(&mut self) -> Result<PipelineProgress> {
        self.idle = false;
        let result = self.poll_inner();
        match result {
            Ok(PipelineProgress::Idle) => {
                self.idle = true;
                Ok(PipelineProgress::Idle)
            }
            Ok(progress) => Ok(progress),
            Err(error) => {
                self.client.stream.sock.poison_v1(&error);
                Err(error.context(ProductRelayTerminalErrorV1(
                    "incremental relay pump failed".into(),
                )))
            }
        }
    }

    fn poll_inner(&mut self) -> Result<PipelineProgress> {
        self.check_deadlines()?;
        if self.closed {
            bail!("relay pipeline is closed");
        }
        let mut progressed = false;
        let result = if let Some(event) = pop_pending_relay_event_v1(
            &mut self.client.pending_events,
            &mut self.client.pending_event_bytes,
        ) {
            Some(PipelineProgress::Event(Box::new(event)))
        } else {
            let mut frame = self.take_frame()?;
            if frame.is_none() {
                progressed |= self.read_plaintext()?;
                frame = self.take_frame()?;
            }
            if frame.is_none() && self.client.stream.conn.wants_read() {
                let stream = &mut self.client.stream;
                match stream
                    .conn
                    .read_tls(&mut IncrementalSocket(&mut stream.sock))
                {
                    Ok(count) => {
                        progressed |= count > 0;
                        self.tls_plaintext_bytes =
                            stream.conn.process_new_packets()?.plaintext_bytes_to_read();
                        progressed |= self.read_plaintext()?;
                        frame = self.take_frame()?;
                        if count == 0 && frame.is_none() {
                            bail!("relay TLS stream closed");
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
            match frame {
                Some(frame) => {
                    progressed = true;
                    self.handle_frame(frame)?
                }
                None => None,
            }
        };
        if let Some(PipelineProgress::Event(event)) = &result {
            if matches!(
                event.as_ref(),
                ProductRelayClientEventV1::Delivery(_)
                    | ProductRelayClientEventV1::PeerHandshake(_)
            ) {
                self.client.delivery_consumed = self
                    .client
                    .delivery_consumed
                    .checked_add(1)
                    .context("product relay delivery consumption counter overflow")?;
            }
        }
        // One read/parser quantum AND one write quantum each call. Inbound
        // activity cannot turn this into write-N-then-read or starve the writer.
        if !self.closed {
            progressed |= self.start_due_control()?;
            progressed |= self.write_step()?;
        }
        self.check_deadlines()?;
        Ok(result.unwrap_or(if progressed {
            PipelineProgress::Progress
        } else {
            PipelineProgress::Idle
        }))
    }

    fn read_plaintext(&mut self) -> Result<bool> {
        let mut bytes = [0; IO_QUANTUM];
        match self.client.stream.conn.reader().read(&mut bytes) {
            Ok(0) => bail!("relay TLS close_notify before WebSocket close"),
            Ok(count) => {
                let next = self
                    .client
                    .read_buffer
                    .len()
                    .checked_add(count)
                    .context("relay read buffer overflow")?;
                if next > MAX_READ_BUFFER {
                    bail!("relay incremental read buffer capacity exceeded");
                }
                if next > self.client.read_buffer.capacity() {
                    self.client
                        .read_buffer
                        .try_reserve_exact(next - self.client.read_buffer.len())?;
                }
                self.client.read_buffer.extend_from_slice(&bytes[..count]);
                self.tls_plaintext_bytes = self.tls_plaintext_bytes.saturating_sub(count);
                self.client.stream.sock.ensure_frame_deadline_v1()?;
                Ok(true)
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(false),
            Err(error) => Err(error.into()),
        }
    }

    fn take_frame(&mut self) -> Result<Option<RelayClientFrameV1>> {
        let unread = &self.client.read_buffer[self.client.read_buffer_offset..];
        let Some((frame, consumed)) = decode_buffered_frame_v1(unread)? else {
            return Ok(None);
        };
        self.client.read_buffer_offset += consumed;
        let buffered = self.client.read_buffer_offset < self.client.read_buffer.len();
        if !buffered {
            self.client.read_buffer.clear();
            self.client.read_buffer_offset = 0;
        } else {
            self.client
                .read_buffer
                .drain(..self.client.read_buffer_offset);
            self.client.read_buffer_offset = 0;
        }
        self.client
            .stream
            .sock
            .finish_frame_v1(buffered || self.tls_plaintext_bytes != 0)?;
        Ok(Some(frame))
    }

    fn handle_frame(&mut self, frame: RelayClientFrameV1) -> Result<Option<PipelineProgress>> {
        match frame {
            RelayClientFrameV1::Binary(bytes) => {
                self.control_frames = 0;
                self.client.stream.sock.read_operation_deadline = None;
                match decode_protocol_item_v1(&bytes, &mut self.client.heartbeat_health)? {
                    ProductRelayClientProtocolItemV1::Event { event, .. } => {
                        Ok(Some(PipelineProgress::Event(event)))
                    }
                    ProductRelayClientProtocolItemV1::ForwardOutcome(outcome) => {
                        let (ticket, pending) = self
                            .pending
                            .iter()
                            .find(|(_, item)| item.matches(&outcome))
                            .context("relay returned an unknown or duplicate forward outcome")?;
                        let ticket = *ticket;
                        if pending.deadline.is_none() {
                            bail!("relay returned outcome before its complete frame was flushed");
                        }
                        validate_forward_outcome_v1(
                            &outcome,
                            &pending.source,
                            &pending.target,
                            pending.session,
                            pending.sequence,
                            pending.wire_bytes,
                        )?;
                        self.pending.remove(&ticket);
                        Ok(Some(PipelineProgress::Forward { ticket, outcome }))
                    }
                }
            }
            RelayClientFrameV1::Ping(payload) => {
                self.control_frame()?;
                if self.pongs.len() >= PRODUCT_RELAY_MAX_CONTROL_FRAMES_PER_PROTOCOL_ITEM_V1 {
                    bail!("relay pending Pong capacity exceeded");
                }
                self.pongs.push_back(payload);
                Ok(None)
            }
            RelayClientFrameV1::Pong => {
                self.control_frame()?;
                Ok(None)
            }
            RelayClientFrameV1::Close => {
                self.closed = true;
                if !self.pending.is_empty() {
                    bail!("relay closed with unresolved forward obligations");
                }
                Ok(Some(PipelineProgress::Event(Box::new(
                    ProductRelayClientEventV1::Closed,
                ))))
            }
        }
    }

    fn control_frame(&mut self) -> Result<()> {
        self.control_frames += 1;
        let deadline = match self.client.stream.sock.read_operation_deadline {
            Some(deadline) => deadline,
            None => {
                let deadline = Instant::now()
                    .checked_add(Duration::from_millis(
                        PRODUCT_RELAY_PROTOCOL_ITEM_DEADLINE_MS_V1,
                    ))
                    .context("relay protocol deadline overflow")?;
                self.client.stream.sock.read_operation_deadline = Some(deadline);
                deadline
            }
        };
        ensure_protocol_item_progress_v1(deadline, self.control_frames)
    }

    fn credit_due(&self) -> bool {
        self.client.delivery_consumed - self.client.delivery_consumed_reported
            >= PRODUCT_RELAY_DELIVERY_WINDOW_V1 / 2
    }

    fn start_control(
        &mut self,
        wire: ProductRelayWireMessageV1,
        purpose: WritePurpose,
    ) -> Result<()> {
        let bytes = encode_masked_frame_v1(0x2, &serde_json::to_vec(&wire)?)?;
        self.start_frame(bytes, purpose)
    }

    fn start_frame(&mut self, bytes: Vec<u8>, purpose: WritePurpose) -> Result<()> {
        self.client.stream.sock.begin_authenticated_write_v1()?;
        self.writing = Some(WritingFrame {
            bytes,
            offset: 0,
            purpose,
        });
        self.idle = false;
        Ok(())
    }

    fn start_due_control(&mut self) -> Result<bool> {
        if self.writing.is_some() || self.client.stream.sock.write_deadline.is_some() {
            return Ok(false);
        }
        if let Some(payload) = self.pongs.pop_front() {
            self.start_frame(encode_masked_frame_v1(0xA, &payload)?, WritePurpose::Pong)?;
            return Ok(true);
        }
        if self.credit_due() {
            let through = self.client.delivery_consumed;
            self.start_control(
                ProductRelayWireMessageV1::DeliveryConsumedV1 { through },
                WritePurpose::Credit(through),
            )?;
            return Ok(true);
        }
        Ok(false)
    }

    fn write_step(&mut self) -> Result<bool> {
        let mut progressed = false;
        if let Some(writing) = &mut self.writing {
            let end = writing
                .offset
                .saturating_add(IO_QUANTUM)
                .min(writing.bytes.len());
            if writing.offset < end {
                let count = self
                    .client
                    .stream
                    .conn
                    .writer()
                    .write(&writing.bytes[writing.offset..end])?;
                writing.offset += count;
                progressed |= count != 0;
            }
        }
        if self.client.stream.conn.wants_write() {
            if self.client.stream.sock.write_deadline.is_none() {
                self.client.stream.sock.begin_authenticated_write_v1()?;
            }
            let stream = &mut self.client.stream;
            match stream
                .conn
                .write_tls(&mut IncrementalSocket(&mut stream.sock))
            {
                Ok(0) => bail!("relay TLS socket write made zero progress"),
                Ok(_) => {
                    self.write_stall_deadline = None;
                    progressed = true;
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    if self.write_stall_deadline.is_none() {
                        self.write_stall_deadline = stream
                            .sock
                            .write_timeout
                            .map(|timeout| {
                                Instant::now()
                                    .checked_add(timeout)
                                    .context("relay write readiness deadline overflow")
                            })
                            .transpose()?;
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => return Err(error.into()),
            }
        }
        if !self.client.stream.conn.wants_write() {
            if self
                .writing
                .as_ref()
                .is_some_and(|writing| writing.offset == writing.bytes.len())
            {
                let writing = self.writing.take().expect("completed frame");
                self.client.stream.sock.finish_authenticated_write_v1()?;
                match writing.purpose {
                    WritePurpose::Forward(ticket) => {
                        self.pending
                            .get_mut(&ticket)
                            .context("lost forward obligation")?
                            .deadline = Some(
                            Instant::now()
                                .checked_add(Duration::from_millis(
                                    PRODUCT_RELAY_PROTOCOL_ITEM_DEADLINE_MS_V1,
                                ))
                                .context("relay outcome deadline overflow")?,
                        );
                    }
                    WritePurpose::Credit(through) => {
                        self.client.delivery_consumed_reported = through
                    }
                    WritePurpose::Heartbeat | WritePurpose::Pong => {}
                }
                progressed = true;
            } else if self.writing.is_none() && self.client.stream.sock.write_deadline.is_some() {
                self.client.stream.sock.finish_authenticated_write_v1()?;
            }
        }
        Ok(progressed)
    }

    fn check_deadlines(&mut self) -> Result<()> {
        self.client.stream.sock.check_io_deadlines_v1()?;
        self.client.heartbeat_health.check(Instant::now())?;
        if self
            .pending
            .values()
            .filter_map(|item| item.deadline)
            .chain(self.write_stall_deadline)
            .any(|deadline| Instant::now() >= deadline)
        {
            let error =
                absolute_deadline_error_v1("relay forward/write absolute deadline exceeded");
            self.client.stream.sock.poison_v1(&error);
            return Err(error.into());
        }
        Ok(())
    }

    /// Only the owner sleeps here, after `poll` returned Idle and after it has
    /// considered new work. Readiness is a hint; the next poll retries actual I/O.
    pub(crate) fn wait(&mut self) -> Result<()> {
        let result = self.wait_inner();
        if let Err(error) = &result {
            self.client.stream.sock.poison_v1(error);
        }
        result
    }

    fn wait_inner(&mut self) -> Result<()> {
        self.check_deadlines()?;
        if !self.idle || self.closed {
            return Ok(());
        }
        let now = Instant::now();
        let socket = &self.client.stream.sock;
        let mut timeout = socket.read_timeout.unwrap_or(Duration::from_millis(
            PRODUCT_RELAY_PROTOCOL_ITEM_DEADLINE_MS_V1,
        ));
        for deadline in [
            socket.frame_deadline,
            socket.write_deadline,
            socket.read_operation_deadline,
            self.write_stall_deadline,
        ]
        .into_iter()
        .flatten()
        .chain(self.pending.values().filter_map(|item| item.deadline))
        .chain(
            self.client
                .heartbeat_health
                .sent_at
                .and_then(|sent| sent.checked_add(Duration::from_secs(15))),
        ) {
            timeout = timeout.min(deadline.saturating_duration_since(now));
        }
        let interest = if self.client.stream.conn.wants_write() {
            mio::Interest::READABLE | mio::Interest::WRITABLE
        } else {
            mio::Interest::READABLE
        };
        self.client
            .stream
            .sock
            .inner
            .wait_ready(interest, timeout)?;
        self.check_deadlines()
    }
}

/// The explicit rustls API, unlike StreamOwned::write/read, never calls
/// complete_io behind the pump. Successful byte counts always reach rustls,
/// even when post-I/O deadline maintenance permanently poisons the connection.
struct IncrementalSocket<'a>(&'a mut ProductRelayDeadlineTcpStreamV1);

impl Read for IncrementalSocket<'_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        self.0.check_io_deadlines_v1()?;
        let started = self.0.inner.read_ahead_started_at();
        let length = output.len().min(IO_QUANTUM);
        let result = self.0.inner.try_read(&mut output[..length]);
        let maintenance = if result.as_ref().is_ok_and(|count| *count > 0) {
            self.0
                .ensure_frame_deadline_v1()
                .and_then(|()| self.0.retain_read_ahead_deadline_v1(started))
        } else {
            self.0.check_io_deadlines_v1()
        };
        self.0.finish_io_progress_v1(result, maintenance)
    }
}

impl Write for IncrementalSocket<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.check_io_deadlines_v1()?;
        #[cfg(test)]
        self.0
            .test_io_calls
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        #[cfg(test)]
        let result = match self.0.test_write_fault.take() {
            Some(ProductRelayClientWriteFaultV1::Timeout) => Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "injected incremental write timeout",
            )),
            Some(ProductRelayClientWriteFaultV1::ExpireAfterProgress(limit)) => {
                let result = self
                    .0
                    .inner
                    .try_write(&bytes[..bytes.len().min(limit).min(IO_QUANTUM)]);
                if result.as_ref().is_ok_and(|written| *written > 0) {
                    self.0.write_deadline = Some(Instant::now() - Duration::from_millis(1));
                }
                result
            }
            None => self
                .0
                .inner
                .try_write(&bytes[..bytes.len().min(IO_QUANTUM)]),
        };
        #[cfg(not(test))]
        let result = self
            .0
            .inner
            .try_write(&bytes[..bytes.len().min(IO_QUANTUM)]);
        let maintenance = self.0.check_io_deadlines_v1();
        self.0.finish_io_progress_v1(result, maintenance)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.check_io_deadlines_v1()
    }
}

#[cfg(test)]
mod tests;
