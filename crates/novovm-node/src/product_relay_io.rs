//! Relay socket I/O without reusable Windows blocking-socket timeouts.
//!
//! Winsock documents a connection as indeterminate after SO_RCVTIMEO or
//! SO_SNDTIMEO expires. Windows therefore uses nonblocking Mio I/O and waits
//! for readiness instead. Only expiration of that wait is recoverable idle;
//! an actual socket timeout is terminal. Higher layers retain their existing
//! handshake/frame/operation deadlines and partial-progress accounting.

use std::{
    io::{self, Read, Write},
    net::TcpStream,
    time::Duration,
};

#[cfg(windows)]
use std::time::Instant;

#[derive(Debug)]
pub(crate) struct ProductRelaySocketV1 {
    #[cfg(not(windows))]
    inner: TcpStream,
    #[cfg(windows)]
    inner: mio::net::TcpStream,
    #[cfg(windows)]
    poll: mio::Poll,
    #[cfg(windows)]
    events: mio::Events,
    #[cfg(windows)]
    read_timeout: Option<Duration>,
    #[cfg(windows)]
    write_timeout: Option<Duration>,
}

impl ProductRelaySocketV1 {
    pub(crate) fn new(stream: TcpStream) -> io::Result<Self> {
        // Capture caller budgets before removing Windows kernel timeouts.
        #[cfg(windows)]
        {
            let read_timeout = stream.read_timeout()?;
            let write_timeout = stream.write_timeout()?;
            stream.set_read_timeout(None)?;
            stream.set_write_timeout(None)?;
            stream.set_nonblocking(true)?;
            let mut inner = mio::net::TcpStream::from_std(stream);
            let poll = mio::Poll::new()?;
            poll.registry()
                .register(&mut inner, mio::Token(0), mio::Interest::READABLE)?;
            Ok(Self {
                inner,
                poll,
                events: mio::Events::with_capacity(4),
                read_timeout,
                write_timeout,
            })
        }
        #[cfg(not(windows))]
        {
            stream.set_nonblocking(false)?;
            Ok(Self { inner: stream })
        }
    }

    pub(crate) fn read_timeout(&self) -> io::Result<Option<Duration>> {
        #[cfg(windows)]
        {
            Ok(self.read_timeout)
        }
        #[cfg(not(windows))]
        self.inner.read_timeout()
    }

    pub(crate) fn write_timeout(&self) -> io::Result<Option<Duration>> {
        #[cfg(windows)]
        {
            Ok(self.write_timeout)
        }
        #[cfg(not(windows))]
        self.inner.write_timeout()
    }

    pub(crate) fn set_read_timeout(&mut self, timeout: Option<Duration>) -> io::Result<()> {
        validate_timeout(timeout)?;
        #[cfg(not(windows))]
        self.inner.set_read_timeout(timeout)?;
        #[cfg(windows)]
        {
            self.read_timeout = timeout;
        }
        Ok(())
    }

    pub(crate) fn set_write_timeout(&mut self, timeout: Option<Duration>) -> io::Result<()> {
        validate_timeout(timeout)?;
        #[cfg(not(windows))]
        self.inner.set_write_timeout(timeout)?;
        #[cfg(windows)]
        {
            self.write_timeout = timeout;
        }
        Ok(())
    }

    #[cfg(windows)]
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
            .reregister(&mut self.inner, mio::Token(0), interest)
            .map_err(windows_terminal_timeout)?;
        loop {
            remaining_timeout(deadline)?;
            match operation(&mut self.inner) {
                // Never replace successful progress with a post-I/O error.
                Ok(count) => return Ok(count),
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(error) => return Err(windows_terminal_timeout(error)),
            }
            let remaining = remaining_timeout(deadline)?;
            match self.poll.poll(&mut self.events, remaining) {
                // Close/error readiness can also be spurious. Only the next
                // real socket result establishes EOF or a terminal error.
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => return Err(windows_terminal_timeout(error)),
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

#[cfg(windows)]
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

#[cfg(windows)]
fn windows_terminal_timeout(error: io::Error) -> io::Error {
    if error.kind() == io::ErrorKind::TimedOut {
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
        #[cfg(windows)]
        {
            self.perform_io(mio::Interest::READABLE, self.read_timeout, |stream| {
                stream.read(output)
            })
        }
        #[cfg(not(windows))]
        self.inner.read(output)
    }
}

impl Write for ProductRelaySocketV1 {
    fn write(&mut self, input: &[u8]) -> io::Result<usize> {
        if input.is_empty() {
            return Ok(0);
        }
        #[cfg(windows)]
        {
            self.perform_io(mio::Interest::WRITABLE, self.write_timeout, |stream| {
                stream.write(input)
            })
        }
        #[cfg(not(windows))]
        self.inner.write(input)
    }

    fn flush(&mut self) -> io::Result<()> {
        // TCP (including Mio's wrapper) has no user-space flush buffer.
        #[cfg(windows)]
        {
            self.inner.flush().map_err(windows_terminal_timeout)
        }
        #[cfg(not(windows))]
        self.inner.flush()
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
        #[cfg(not(windows))]
        let observer = client.try_clone().unwrap();
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
        #[cfg(windows)]
        {
            assert_eq!(
                socket.read_timeout().unwrap(),
                Some(Duration::from_millis(10))
            );
            // A Windows duplicated socket handle is not the option state of
            // the handle that Mio actually owns. Inspect that handle directly.
            let actual_socket = socket2::SockRef::from(&socket.inner);
            assert_eq!(actual_socket.read_timeout().unwrap(), None);
            assert_eq!(actual_socket.write_timeout().unwrap(), None);
        }
        #[cfg(not(windows))]
        {
            assert_eq!(
                socket.read_timeout().unwrap(),
                observer.read_timeout().unwrap()
            );
            assert_eq!(
                socket.write_timeout().unwrap(),
                observer.write_timeout().unwrap()
            );
        }
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
            #[cfg(windows)]
            assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
            #[cfg(not(windows))]
            assert!(matches!(
                error.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
            ));
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
        let mut bytes = [0; 10];
        socket.read_exact(&mut bytes).unwrap();
        assert_eq!(&bytes, b"before-eof");
        assert_eq!(socket.read(&mut [0]).unwrap(), 0);
    }

    #[cfg(windows)]
    #[test]
    fn actual_windows_socket_timeout_is_not_recoverable_idle() {
        let error = windows_terminal_timeout(io::Error::from_raw_os_error(10060));
        assert_eq!(error.kind(), io::ErrorKind::ConnectionAborted);
        assert!(error.to_string().contains("10060"));
        assert!(!error
            .get_ref()
            .is_some_and(|source| source.is::<io::Error>()));
        assert_eq!(
            windows_terminal_timeout(io::Error::from(io::ErrorKind::ConnectionReset)).kind(),
            io::ErrorKind::ConnectionReset
        );
    }

    #[cfg(windows)]
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

    #[cfg(windows)]
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
        // The blocked state is now established. Let the real reader drain
        // promptly rather than making tiny receive windows a throughput test.
        socket2::SockRef::from(&server)
            .set_recv_buffer_size(1024 * 1024)
            .unwrap();
        let expected_len = payload.len();
        let reader = thread::spawn(move || {
            let mut received = vec![0; expected_len];
            server.read_exact(&mut received).unwrap();
            received
        });
        socket
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        socket.write_all(&payload[sent..]).unwrap();
        assert_eq!(reader.join().unwrap(), payload);
    }
}
