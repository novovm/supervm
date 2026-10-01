use crate::native_block_seal::service::FreshChainLifecycleV1;
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::time::{Duration, Instant};

const MAX_CONNECTIONS: usize = 8;
const MAX_REQUEST: usize = crate::tx_ingress::fresh_pool::MAX_RAW_BYTES * 2 + 8192;
const DEADLINE: Duration = Duration::from_secs(3);
const IDLE_POLL_PAUSE: Duration = Duration::from_millis(5);

struct Connection {
    stream: TcpStream,
    input: Vec<u8>,
    output: Option<Vec<u8>>,
    written: usize,
    opened: Instant,
}

pub struct FreshRpcServer {
    listener: TcpListener,
    connections: Vec<Connection>,
}

fn body(bytes: &[u8]) -> Result<Option<&[u8]>> {
    let Some(end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") else {
        if bytes.len() > 4096 {
            bail!("HTTP header limit");
        }
        return Ok(None);
    };
    if end > 4096 {
        bail!("HTTP header limit");
    }
    let mut lines = std::str::from_utf8(&bytes[..end])?.split("\r\n");
    if lines.next() != Some("POST / HTTP/1.1") {
        bail!("POST / HTTP/1.1 required");
    }
    let mut length = None;
    for line in lines {
        let (name, value) = line.split_once(':').context("invalid HTTP header")?;
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            bail!("invalid HTTP header name");
        }
        if name.eq_ignore_ascii_case("transfer-encoding") || name.eq_ignore_ascii_case("expect") {
            bail!("unsupported HTTP framing");
        }
        if name.eq_ignore_ascii_case("content-length") {
            if length.is_some() {
                bail!("duplicate content length");
            }
            let value = value.trim();
            if !value.bytes().all(|byte| byte.is_ascii_digit()) {
                bail!("invalid content length");
            }
            length = Some(value.parse::<usize>()?);
        }
    }
    let length = length.context("content length required")?;
    if length == 0 || length > MAX_REQUEST - 4096 {
        bail!("HTTP body limit");
    }
    let expected = end + 4 + length;
    if bytes.len() > expected {
        bail!("HTTP pipelining not supported");
    }
    Ok((bytes.len() == expected).then_some(&bytes[end + 4..]))
}

fn decode_hex(text: &str) -> Result<Vec<u8>> {
    let text = text.strip_prefix("0x").unwrap_or(text);
    if text.is_empty()
        || !text.len().is_multiple_of(2)
        || text.len() > crate::tx_ingress::fresh_pool::MAX_RAW_BYTES * 2
        || !text.is_ascii()
    {
        bail!("invalid raw transaction hex length");
    }
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
        .collect()
}

pub fn handle_fresh_rpc(request: Value, lifecycle: &mut FreshChainLifecycleV1) -> Value {
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let result = (|| -> Result<Value> {
        if request["jsonrpc"] != "2.0" || !(id.is_null() || id.is_string() || id.is_number()) {
            bail!("invalid JSON-RPC request");
        }
        match request["method"].as_str() {
            Some("nov_sendRawTransaction") => {
                let params = request["params"]
                    .as_array()
                    .context("params array required")?;
                if params.len() != 1 {
                    bail!("one raw transaction required");
                }
                let raw = decode_hex(params[0].as_str().context("raw hex required")?)?;
                crate::native_fresh_timing::measure("rpc.submit_raw_transaction", || {
                    lifecycle.submit_raw_transaction(raw)
                })
            }
            Some("nov_getTransactionStatus") => {
                let params = request["params"]
                    .as_array()
                    .context("params array required")?;
                if params.len() != 1 {
                    bail!("one transaction hash required");
                }
                let hash: [u8; 32] = decode_hex(params[0].as_str().context("hash hex required")?)?
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("32-byte hash required"))?;
                crate::native_fresh_timing::measure("rpc.transaction_status", || {
                    lifecycle.transaction_status(hash)
                })
            }
            Some("nov_chainStatus") => Ok(lifecycle.status_json()),
            _ => bail!("method not supported"),
        }
    })();
    match result {
        Ok(result) => json!({"jsonrpc":"2.0","id":id,"result":result}),
        Err(error) => {
            json!({"jsonrpc":"2.0","id":id,"error":{"code":-32000,"message":error.to_string()}})
        }
    }
}

impl FreshRpcServer {
    pub fn bind(address: SocketAddr) -> Result<Self> {
        if !address.ip().is_loopback() {
            bail!(
                "fresh RPC requires loopback; remote access requires an authenticated TLS gateway"
            );
        }
        let listener = TcpListener::bind(address)?;
        listener.set_nonblocking(true)?;
        Ok(Self {
            listener,
            connections: Vec::new(),
        })
    }

    pub fn local_addr(&self) -> Result<SocketAddr> {
        Ok(self.listener.local_addr()?)
    }

    pub fn poll(&mut self, lifecycle: &mut FreshChainLifecycleV1) -> Result<()> {
        self.poll_with(|request| handle_fresh_rpc(request, lifecycle))
    }

    /// Service the existing bounded RPC poll within the owner's idle budget.
    /// The budget never restarts when requests arrive. An individual synchronous
    /// handler may overrun it; no additional poll or sleep then delays consensus.
    pub fn poll_during_idle(
        &mut self,
        lifecycle: &mut FreshChainLifecycleV1,
        idle: Duration,
    ) -> Result<()> {
        let started = Instant::now();
        poll_during_idle_with(
            idle,
            || crate::native_fresh_timing::measure("rpc.idle_poll", || self.poll(lifecycle)),
            || started.elapsed(),
            std::thread::sleep,
        )
    }

    fn poll_with(&mut self, mut handle: impl FnMut(Value) -> Value) -> Result<()> {
        for _ in 0..MAX_CONNECTIONS {
            match self.listener.accept() {
                Ok((stream, _)) if self.connections.len() < MAX_CONNECTIONS => {
                    stream.set_nonblocking(true)?;
                    self.connections.push(Connection {
                        stream,
                        input: Vec::new(),
                        output: None,
                        written: 0,
                        opened: Instant::now(),
                    });
                }
                Ok(_) => (),
                Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                Err(error) => return Err(error.into()),
            }
        }
        self.connections.retain_mut(|connection| {
            if connection.opened.elapsed() > DEADLINE { return false; }
            if connection.output.is_none() {
                let mut buffer = [0u8; 16 * 1024];
                match connection.stream.read(&mut buffer) {
                    Ok(0) => return false,
                    Ok(count) => connection.input.extend_from_slice(&buffer[..count]),
                    Err(error) if error.kind() == ErrorKind::WouldBlock => return true,
                    Err(_) => return false,
                }
                if connection.input.len() > MAX_REQUEST { return false; }
                let request = match body(&connection.input) {
                    Ok(Some(bytes)) => serde_json::from_slice(bytes),
                    Ok(None) => return true,
                    Err(_) => return false,
                };
                let response = match request {
                    Ok(value) => handle(value),
                    Err(_) => json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"invalid JSON"}}),
                };
                let bytes = serde_json::to_vec(&response).expect("JSON value serializes");
                let mut output = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n", bytes.len()).into_bytes();
                output.extend_from_slice(&bytes);
                connection.output = Some(output);
            }
            let output = connection.output.as_ref().expect("response ready");
            match connection.stream.write(&output[connection.written..]) {
                Ok(0) => false,
                Ok(count) => { connection.written += count; connection.written < output.len() },
                Err(error) if error.kind() == ErrorKind::WouldBlock => true,
                Err(_) => false,
            }
        });
        Ok(())
    }
}

// Clock/sleep closures keep budget and error tests deterministic; production
// uses one monotonic start and ordinary sleep, with no new scheduler or thread.
fn poll_during_idle_with(
    idle: Duration,
    mut poll: impl FnMut() -> Result<()>,
    mut elapsed: impl FnMut() -> Duration,
    mut sleep: impl FnMut(Duration),
) -> Result<()> {
    while elapsed() < idle {
        poll()?;
        let remaining = idle.saturating_sub(elapsed());
        if remaining.is_zero() {
            break;
        }
        sleep(remaining.min(IDLE_POLL_PAUSE));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn idle_rpc_zero_budget_does_not_poll_or_sleep() {
        poll_during_idle_with(
            Duration::ZERO,
            || panic!("zero idle budget polled"),
            || Duration::ZERO,
            |_| panic!("zero idle budget slept"),
        )
        .unwrap();
    }

    #[test]
    fn idle_rpc_keeps_one_budget_and_caps_each_pause() {
        let elapsed = Cell::new(Duration::ZERO);
        let mut polls = 0;
        let mut pauses = Vec::new();
        poll_during_idle_with(
            Duration::from_millis(14),
            || {
                polls += 1;
                elapsed.set(elapsed.get() + Duration::from_millis(1));
                Ok(())
            },
            || elapsed.get(),
            |pause| {
                pauses.push(pause);
                elapsed.set(elapsed.get() + pause);
            },
        )
        .unwrap();
        assert_eq!(polls, 3);
        assert_eq!(
            pauses,
            [
                Duration::from_millis(5),
                Duration::from_millis(5),
                Duration::from_millis(1)
            ]
        );
        assert_eq!(elapsed.get(), Duration::from_millis(14));
    }

    #[test]
    fn idle_rpc_slow_poll_exhausts_budget_without_another_poll_or_sleep() {
        let elapsed = Cell::new(Duration::ZERO);
        let mut polls = 0;
        poll_during_idle_with(
            Duration::from_millis(10),
            || {
                polls += 1;
                elapsed.set(Duration::from_millis(20));
                Ok(())
            },
            || elapsed.get(),
            |_| panic!("exhausted budget slept"),
        )
        .unwrap();
        assert_eq!(polls, 1);
    }

    #[test]
    fn idle_rpc_preserves_poll_error_and_accepts_maximum_duration() {
        let mut polls = 0;
        let mut pauses = 0;
        let result = poll_during_idle_with(
            Duration::MAX,
            || {
                polls += 1;
                if polls == 3 {
                    return Err(std::io::Error::other("original poll failure").into());
                }
                Ok(())
            },
            || Duration::ZERO,
            |pause| {
                assert_eq!(pause, IDLE_POLL_PAUSE);
                pauses += 1;
            },
        );
        let error = result.unwrap_err();
        assert!(error.is::<std::io::Error>());
        assert_eq!(error.to_string(), "original poll failure");
        assert_eq!((polls, pauses), (3, 2));
    }

    #[test]
    fn bounded_http_framing_and_hex() {
        assert_eq!(
            body(b"POST / HTTP/1.1\r\nContent-Length: 2\r\n\r\n{}").unwrap(),
            Some(&b"{}"[..])
        );
        assert!(body(b"POST / HTTP/1.1\r\nContent-Length: 2\r\n\r\n{")
            .unwrap()
            .is_none());
        for packet in [
            "POST / HTTP/1.1\r\nContent-Length: 2\r\nContent-Length: 2\r\n\r\n{}",
            "POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n0",
            "POST / HTTP/1.1\r\nContent-Length: 999999999\r\n\r\n",
            "POST / HTTP/1.1\r\nContent-Length: 2\r\n\r\n{}junk",
        ] {
            assert!(body(packet.as_bytes()).is_err());
        }
        assert_eq!(decode_hex("0x12aF").unwrap(), vec![18, 175]);
        for value in ["0", "zz", "啊", "", "0x"] {
            assert!(decode_hex(value).is_err());
        }
        assert!(FreshRpcServer::bind("0.0.0.0:0".parse().unwrap()).is_err());
    }

    #[test]
    fn partial_client_does_not_block_another_and_times_out() {
        let mut server = FreshRpcServer::bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let address = server.local_addr().unwrap();
        let mut partial = TcpStream::connect(address).unwrap();
        partial
            .write_all(b"POST / HTTP/1.1\r\nContent-Length: 100\r\n\r\n{")
            .unwrap();
        server
            .poll_with(|_| panic!("partial request reached handler"))
            .unwrap();
        let mut complete = TcpStream::connect(address).unwrap();
        complete
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        complete
            .write_all(b"POST / HTTP/1.1\r\nContent-Length: 2\r\n\r\n{}")
            .unwrap();
        for _ in 0..5 {
            server.poll_with(|_| json!({"ok":true})).unwrap();
            std::thread::sleep(Duration::from_millis(5));
        }
        let mut response = String::new();
        complete.read_to_string(&mut response).unwrap();
        assert!(response.contains("\"ok\":true"));
        assert_eq!(server.connections.len(), 1);
        server.connections[0].opened = Instant::now() - DEADLINE - Duration::from_secs(1);
        server
            .poll_with(|_| panic!("expired request reached handler"))
            .unwrap();
        assert!(server.connections.is_empty());
    }
}
