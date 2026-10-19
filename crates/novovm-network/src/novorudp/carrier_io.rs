//! Scoped IO for the original native owner. No worker, task or retry scheduler.
//! A queued write returns WouldBlock until the actual framed write completes.
use super::{
    carrier::{NativeCarrierIoFailure, NativeDatagramIo, NativeRoute, VerifiedCarrierRoute},
    datagram::MAX_DATAGRAM_BYTES,
    endpoint::NativeEndpoint,
};
use crate::{
    carrier_stream::{CarrierReadHalfV1, CarrierStreamV1, CarrierWriteHalfV1},
    transport_binding::BoundSecureChannelV1,
};
use anyhow::Result;
use std::{
    future::{poll_fn, Future},
    io,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll, Waker},
};

enum WriteState {
    Idle,
    Queued(Arc<[u8]>),
    Writing(Arc<[u8]>),
    Complete(Arc<[u8]>),
}
struct State {
    write: WriteState,
    capacity_released: bool,
    incoming: Option<Vec<u8>>,
    failed: bool,
    wake: Option<Waker>,
}
struct CarrierIo {
    route: NativeRoute,
    state: Mutex<State>,
}
impl CarrierIo {
    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|error| {
            let mut state = error.into_inner();
            state.failed = true;
            state
        })
    }
    fn wake(state: &mut State) {
        if let Some(wake) = state.wake.take() {
            wake.wake();
        }
    }
}
impl NativeDatagramIo for CarrierIo {
    fn deferred_writes(&self) -> bool {
        true
    }
    fn try_receive(&self, target: &mut [u8]) -> io::Result<Option<(usize, NativeRoute)>> {
        let mut state = self.state();
        if state.failed {
            return Err(io::ErrorKind::ConnectionAborted.into());
        }
        let bytes = state.incoming.as_ref().ok_or(io::ErrorKind::WouldBlock)?;
        if bytes.len() > target.len() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let count = bytes.len();
        target[..count].copy_from_slice(bytes);
        state.incoming = None;
        Self::wake(&mut state);
        Ok(Some((count, self.route)))
    }
    fn try_send(&self, bytes: &[u8], route: NativeRoute) -> io::Result<usize> {
        let mut state = self.state();
        if state.failed {
            return Err(io::ErrorKind::ConnectionAborted.into());
        }
        if route != self.route || bytes.is_empty() || bytes.len() > MAX_DATAGRAM_BYTES {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        match &state.write {
            WriteState::Idle => {
                state.write = WriteState::Queued(bytes.into());
                state.capacity_released = false;
                Self::wake(&mut state);
                Err(io::ErrorKind::WouldBlock.into())
            }
            WriteState::Queued(old) | WriteState::Writing(old) if old.as_ref() == bytes => {
                Err(io::ErrorKind::WouldBlock.into())
            }
            WriteState::Complete(old) if old.as_ref() == bytes => {
                state.write = WriteState::Idle;
                state.capacity_released = true;
                Self::wake(&mut state);
                Ok(bytes.len())
            }
            _ => Err(io::ErrorKind::InvalidInput.into()),
        }
    }
    fn cancel_send(&self, bytes: &[u8], route: NativeRoute) -> io::Result<usize> {
        let mut state = self.state();
        if state.failed {
            return Err(io::ErrorKind::ConnectionAborted.into());
        }
        if route != self.route {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let completed = match &state.write {
            WriteState::Queued(old) if old.as_ref() == bytes => 0,
            WriteState::Complete(old) if old.as_ref() == bytes => bytes.len(),
            WriteState::Writing(old) if old.as_ref() == bytes => {
                state.failed = true;
                Self::wake(&mut state);
                return Err(io::ErrorKind::ConnectionAborted.into());
            }
            _ => return Err(io::ErrorKind::InvalidInput.into()),
        };
        state.write = WriteState::Idle;
        state.capacity_released = true;
        Self::wake(&mut state);
        Ok(completed)
    }
}

type Reading<'a> =
    Pin<Box<dyn Future<Output = (CarrierReadHalfV1<'a>, Result<Vec<u8>>)> + Send + 'a>>;
type Writing<'a> = Pin<Box<dyn Future<Output = (CarrierWriteHalfV1<'a>, Result<()>)> + Send + 'a>>;

/// Poll alongside the SAME NativeObjectLink/application owner. `progress()` may
/// be selected with application events: incomplete frame futures remain here,
/// not in the temporary select branch. Dropping this scope retires the carrier.
/// `attach` retains the finite IO deadline. `attach_continuous_v7` explicitly
/// selects idle-safe native framing in an existing continuous owner. Each frame
/// and mailbox request retains its own bound; neither mode renews business TTL.
pub struct NativeCarrierPump<'a> {
    io: Arc<CarrierIo>,
    reading: Option<Reading<'a>>,
    writer: Option<CarrierWriteHalfV1<'a>>,
    writing: Option<Writing<'a>>,
}
impl<'a> NativeCarrierPump<'a> {
    /// Passive current carrier path and smoothed RTT. Missing/busy/closed or
    /// unsupported observations remain None, never a zero-latency path. No
    /// probe, cloned connection, admission decision or extra task is created.
    pub fn carrier_observation_v1(&self) -> Option<(&'static str, std::time::Duration)> {
        if self.io.state().failed {
            return None;
        }
        self.writer.as_ref()?.selected_rtt()
    }

    pub fn attach(
        stream: &'a mut CarrierStreamV1<'_>,
        channel: BoundSecureChannelV1,
        streams: Vec<u64>,
    ) -> Result<(NativeEndpoint, Self)> {
        Self::attach_mode(stream, channel, streams, false)
    }

    /// Called after explicit native service selection. The channel must prove
    /// Finished on THIS stream; the transport must still have fresh continuous
    /// owner scope. This does not authorize mailbox operations or renew their TTL.
    /// The channel proves binding, not the selected service/handshake codec;
    /// the enclosing native service entry must enforce that explicit selection.
    #[cfg(feature = "iroh-transport")]
    pub fn attach_continuous_v7(
        stream: &'a mut CarrierStreamV1<'_>,
        channel: BoundSecureChannelV1,
        streams: Vec<u64>,
    ) -> Result<(NativeEndpoint, Self)> {
        Self::attach_mode(stream, channel, streams, true)
    }

    fn attach_mode(
        stream: &'a mut CarrierStreamV1<'_>,
        channel: BoundSecureChannelV1,
        streams: Vec<u64>,
        continuous: bool,
    ) -> Result<(NativeEndpoint, Self)> {
        let binding = stream.binding()?;
        let io = Arc::new(CarrierIo {
            route: NativeRoute::Carrier(VerifiedCarrierRoute::from_binding(&binding)),
            state: Mutex::new(State {
                write: WriteState::Idle,
                capacity_released: false,
                incoming: None,
                failed: false,
                wake: None,
            }),
        });
        let endpoint = NativeEndpoint::from_bound_channel(io.clone(), channel, &binding, streams)?;
        #[cfg(feature = "iroh-transport")]
        let (writer, reader) = if continuous {
            stream.split_native_continuous_v7()?
        } else {
            stream.split_io()?
        };
        #[cfg(not(feature = "iroh-transport"))]
        let (writer, reader) = {
            anyhow::ensure!(!continuous, "continuous carrier unavailable");
            stream.split_io()?
        };
        Ok((
            endpoint,
            Self {
                io,
                reading: Some(reading(reader)),
                writer: Some(writer),
                writing: None,
            },
        ))
    }
    pub async fn progress(&mut self) -> Result<()> {
        poll_fn(|cx| self.poll_progress(cx)).await
    }
    fn poll_progress(&mut self, cx: &mut Context<'_>) -> Poll<Result<()>> {
        let mut progressed = false;
        {
            let mut state = self.io.state();
            if state.failed {
                drop(state);
                self.writing = None;
                self.reading = None;
                return Poll::Ready(Err(NativeCarrierIoFailure(
                    io::ErrorKind::ConnectionAborted.into(),
                )
                .into()));
            }
            state.wake = Some(cx.waker().clone());
            // Completing a deferred write and consuming that completion are
            // separate owner turns. After consumption, expose the freed slot
            // once so the same owner can fill it without an unrelated incoming
            // ACK or a fallback timer. Idle alone never counts as progress.
            progressed |= std::mem::take(&mut state.capacity_released);
            if let WriteState::Queued(bytes) = &state.write {
                let Some(writer) = self.writer.take() else {
                    drop(state);
                    return self.fail(anyhow::anyhow!("native carrier writer missing"));
                };
                let bytes = bytes.clone();
                state.write = WriteState::Writing(bytes.clone());
                self.writing = Some(Box::pin(async move {
                    let mut writer = writer;
                    let result = writer.write_frame(&bytes).await;
                    (writer, result)
                }));
            }
        }
        if let Some(write) = self.writing.as_mut() {
            if let Poll::Ready((writer, result)) = write.as_mut().poll(cx) {
                self.writing = None;
                self.writer = Some(writer);
                if let Err(error) = result {
                    return self.fail(error);
                }
                let mut state = self.io.state();
                if state.failed {
                    drop(state);
                    return self.fail(
                        NativeCarrierIoFailure(io::ErrorKind::ConnectionAborted.into()).into(),
                    );
                }
                if let WriteState::Writing(bytes) = &state.write {
                    state.write = WriteState::Complete(bytes.clone());
                } else {
                    drop(state);
                    return self.fail(anyhow::anyhow!("native carrier write state changed"));
                }
                progressed = true;
            }
        }
        let can_read = self.io.state().incoming.is_none();
        if can_read {
            if let Some(read) = self.reading.as_mut() {
                if let Poll::Ready((reader, result)) = read.as_mut().poll(cx) {
                    self.reading = None;
                    let bytes = match result {
                        Ok(bytes) => bytes,
                        Err(error) => return self.fail(error),
                    };
                    if bytes.is_empty() || bytes.len() > MAX_DATAGRAM_BYTES {
                        return self.fail(anyhow::anyhow!(
                            "native carrier frame exceeds datagram bound"
                        ));
                    }
                    self.io.state().incoming = Some(bytes);
                    self.reading = Some(reading(reader));
                    progressed = true;
                }
            }
        }
        if progressed {
            Poll::Ready(Ok(()))
        } else {
            Poll::Pending
        }
    }
    fn fail(&mut self, error: anyhow::Error) -> Poll<Result<()>> {
        self.io.state().failed = true;
        self.writing = None;
        self.reading = None;
        Poll::Ready(Err(error))
    }
}
impl Drop for NativeCarrierPump<'_> {
    fn drop(&mut self) {
        let mut state = self.io.state();
        state.failed = true;
        CarrierIo::wake(&mut state);
    }
}
fn reading<'a>(mut reader: CarrierReadHalfV1<'a>) -> Reading<'a> {
    Box::pin(async move {
        let result = reader.read_frame().await;
        (reader, result)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> CarrierIo {
        CarrierIo {
            route: NativeRoute::Udp("127.0.0.1:12345".parse().unwrap()),
            state: Mutex::new(State {
                write: WriteState::Idle,
                capacity_released: false,
                incoming: None,
                failed: false,
                wake: None,
            }),
        }
    }

    #[test]
    fn queued_is_not_success_and_cancel_releases_only_unstarted_frame() {
        let io = fixture();
        let bytes = [1, 2, 3];
        assert_eq!(
            io.try_send(&bytes, io.route).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        assert_eq!(
            io.try_send(&[4], io.route).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(io.cancel_send(&bytes, io.route).unwrap(), 0);
        assert!(matches!(io.state().write, WriteState::Idle));
        assert_eq!(
            io.try_send(&bytes, io.route).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        io.state().write = WriteState::Complete(bytes.as_slice().into());
        assert_eq!(io.try_send(&bytes, io.route).unwrap(), bytes.len());
        assert!(matches!(io.state().write, WriteState::Idle));
    }

    #[test]
    fn released_capacity_wakes_owner_once_without_idle_spin() {
        let io = Arc::new(fixture());
        let mut pump = NativeCarrierPump {
            io: io.clone(),
            reading: None,
            writer: None,
            writing: None,
        };
        let mut cx = Context::from_waker(Waker::noop());
        assert!(pump.poll_progress(&mut cx).is_pending());
        io.state().write = WriteState::Complete([1, 2, 3].as_slice().into());
        assert_eq!(io.try_send(&[1, 2, 3], io.route).unwrap(), 3);
        assert!(matches!(pump.poll_progress(&mut cx), Poll::Ready(Ok(()))));
        assert!(pump.poll_progress(&mut cx).is_pending());
        // Even an unconsumed capacity edge cannot revive a failed carrier.
        io.state().capacity_released = true;
        io.state().failed = true;
        assert!(matches!(pump.poll_progress(&mut cx), Poll::Ready(Err(_))));
    }

    #[test]
    fn cancelling_started_frame_is_terminal_and_never_accepts_later_work() {
        let io = fixture();
        io.state().write = WriteState::Writing([1, 2, 3].as_slice().into());
        assert_eq!(
            io.cancel_send(&[1, 2, 3], io.route).unwrap_err().kind(),
            io::ErrorKind::ConnectionAborted
        );
        assert!(io.try_send(&[4], io.route).is_err());
        assert!(io.try_receive(&mut [0; 10]).is_err());
    }

    #[test]
    fn poisoned_state_cannot_recover_as_healthy() {
        let io = fixture();
        let _ = std::panic::catch_unwind(|| {
            let _guard = io.state.lock().unwrap();
            panic!("injected owner failure");
        });
        assert_eq!(
            io.try_send(&[1], io.route).unwrap_err().kind(),
            io::ErrorKind::ConnectionAborted
        );
    }
}
