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
    request: Option<Value>,
    pending: Option<u64>,
    output: Option<Vec<u8>>,
    written: usize,
    opened: Instant,
    // Reset each poll; only actual socket bytes set this bit.
    io_progress: bool,
}

pub struct FreshRpcServer {
    listener: TcpListener,
    connections: Vec<Connection>,
    max_request: usize,
    last_deferred_token: Option<u64>,
}

/// Deferred ownership is local to this HTTP server, not an execution receipt.
/// Implementations must allocate strictly increasing tokens (never connection
/// indexes), consume each reply once, and keep start/poll/cancel nonblocking.
/// Cancellation abandons delivery, not necessarily already accepted AOEM work.
pub(crate) trait DeferredRpcHandler {
    fn start(&mut self, request: Value) -> DeferredRpcReply;
    fn poll(&mut self, token: u64) -> Option<Value>;
    fn cancel(&mut self, token: u64);
}

pub(crate) enum DeferredRpcReply {
    Ready(Value),
    Pending(u64),
}

impl Connection {
    // The two dispatcher modes share framing, allocation and socket bounds.
    // Once dispatched, a pending connection never reads/parses its request again.
    fn read_request(&mut self, max_request: usize) -> bool {
        if self.output.is_some() || self.request.is_some() || self.pending.is_some() {
            return true;
        }
        let mut buffer = [0u8; 16 * 1024];
        match self.stream.read(&mut buffer) {
            Ok(0) => return false,
            Ok(count) => {
                self.input.extend_from_slice(&buffer[..count]);
                self.io_progress = true;
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => return true,
            Err(_) => return false,
        }
        if self.input.len() > max_request {
            return false;
        }
        let request = match body_bounded(&self.input, max_request) {
            Ok(Some(bytes)) => serde_json::from_slice(bytes),
            Ok(None) => return true,
            Err(_) => return false,
        };
        match request {
            Ok(value) => self.request = Some(value),
            Err(_) => {
                self.output = Some(http_response(
                    &json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"invalid JSON"}}),
                ));
                self.input = Vec::new();
            }
        }
        true
    }

    fn take_request(&mut self) -> Option<Value> {
        let request = self.request.take()?;
        // Value owns the parsed bytes. Do not retain a second up-to-4MiB raw
        // request while the execution owner holds a deferred authentication job.
        self.input = Vec::new();
        Some(request)
    }

    fn write_response(&mut self) -> bool {
        let Some(output) = self.output.as_ref() else {
            return true;
        };
        match self.stream.write(&output[self.written..]) {
            Ok(0) => false,
            Ok(count) => {
                self.written += count;
                self.io_progress = true;
                self.written < output.len()
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => true,
            Err(_) => false,
        }
    }
}

#[cfg(test)]
fn body(bytes: &[u8]) -> Result<Option<&[u8]>> {
    body_bounded(bytes, MAX_REQUEST)
}

fn body_bounded(bytes: &[u8], max_request: usize) -> Result<Option<&[u8]>> {
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
    if length == 0 || length > max_request - 4096 {
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
            Some("nov_getAssetBalance") => {
                let account = nov_balance_account(&request["params"])?;
                lifecycle.finalized_nov_balance(&account)
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

// Reuse the product method and its named parameters, but not the old Host
// projection reader. This first finalized wallet query covers public NOV only.
fn nov_balance_account(params: &Value) -> Result<String> {
    let params = params
        .as_object()
        .context("balance params object required")?;
    for name in params.keys() {
        if !matches!(name.as_str(), "account" | "asset" | "asset_id") {
            bail!("unsupported finalized balance parameter: {name}");
        }
    }
    for name in ["asset", "asset_id"] {
        if let Some(asset) = params.get(name) {
            if !asset
                .as_str()
                .is_some_and(|asset| asset.eq_ignore_ascii_case("NOV"))
            {
                bail!("fresh finalized balance query supports public NOV only");
            }
        }
    }
    let account = params
        .get("account")
        .and_then(Value::as_str)
        .context("account is required")?;
    let account = account.strip_prefix("0x").unwrap_or(account);
    if !matches!(account.len(), 40 | 64) {
        bail!("account must be a 20-byte or 32-byte hex address");
    }
    decode_hex(account)?;
    Ok(format!("0x{}", account.to_ascii_lowercase()))
}

// Only adjacent submissions share an admission turn. A status/query request
// is an ordering barrier: later submissions cannot become visible to it early.
fn coalesce_ready_requests(
    requests: Vec<Value>,
    mut handle: impl FnMut(Vec<Value>) -> Vec<Value>,
) -> Vec<Value> {
    let mut requests = requests.into_iter().peekable();
    let mut responses = Vec::new();
    while let Some(request) = requests.next() {
        let submit = request["method"] == "nov_sendRawTransaction";
        let mut group = vec![request];
        if submit {
            while requests
                .peek()
                .is_some_and(|next| next["method"] == "nov_sendRawTransaction")
            {
                group.push(requests.next().expect("peeked submission"));
            }
        }
        responses.extend(handle(group));
    }
    responses
}

fn handle_fresh_rpc_requests(
    requests: Vec<Value>,
    lifecycle: &mut FreshChainLifecycleV1,
) -> Vec<Value> {
    coalesce_ready_requests(requests, |requests| {
        if requests[0]["method"] != "nov_sendRawTransaction" {
            return requests
                .into_iter()
                .map(|request| handle_fresh_rpc(request, lifecycle))
                .collect();
        }
        handle_submit_requests(requests, |raws| {
            crate::native_fresh_timing::measure("rpc.submit_raw_transactions", || {
                lifecycle.submit_raw_transactions(raws)
            })
        })
    })
}

fn handle_submit_requests(
    requests: Vec<Value>,
    submit: impl FnOnce(Vec<Vec<u8>>) -> Vec<Result<Value>>,
) -> Vec<Value> {
    let mut parsed = Vec::with_capacity(requests.len());
    let mut raws = Vec::with_capacity(requests.len());
    for request in requests {
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let result = (|| -> Result<()> {
            if request["jsonrpc"] != "2.0" || !(id.is_null() || id.is_string() || id.is_number()) {
                bail!("invalid JSON-RPC request");
            }
            let params = request["params"]
                .as_array()
                .context("params array required")?;
            if params.len() != 1 {
                bail!("one raw transaction required");
            }
            raws.push(decode_hex(params[0].as_str().context("raw hex required")?)?);
            Ok(())
        })();
        parsed.push((id, result));
    }
    let mut admitted = submit(raws).into_iter();
    parsed.into_iter().map(|(id, parsed)| {
            let result = parsed.and_then(|()| admitted.next().expect("one result per parsed raw"));
            match result {
                Ok(result) => json!({"jsonrpc":"2.0","id":id,"result":result}),
                Err(error) => json!({"jsonrpc":"2.0","id":id,"error":{"code":-32000,"message":error.to_string()}}),
            }
        }).collect()
}

fn http_response(response: &Value) -> Vec<u8> {
    let bytes = serde_json::to_vec(response).expect("JSON value serializes");
    let mut output = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n", bytes.len()).into_bytes();
    output.extend_from_slice(&bytes);
    output
}

impl FreshRpcServer {
    pub fn bind(address: SocketAddr) -> Result<Self> {
        Self::bind_with_max_request(address, MAX_REQUEST)
    }

    /// Explicit bounded request envelope for a selected batch-capable profile.
    /// The original profile retains MAX_REQUEST; connection count/deadlines
    /// and strict HTTP framing are unchanged.
    pub(crate) fn bind_with_max_request(address: SocketAddr, max_request: usize) -> Result<Self> {
        if !(MAX_REQUEST..=4 * 1024 * 1024).contains(&max_request) {
            bail!("RPC request envelope outside supported bounded range");
        }
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
            max_request,
            last_deferred_token: None,
        })
    }

    pub fn local_addr(&self) -> Result<SocketAddr> {
        Ok(self.listener.local_addr()?)
    }

    pub fn poll(&mut self, lifecycle: &mut FreshChainLifecycleV1) -> Result<()> {
        self.poll_batch_when(!lifecycle.candidate_storage_busy(), |requests| {
            handle_fresh_rpc_requests(requests, lifecycle)
        })
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
            || {
                crate::native_fresh_timing::measure("rpc.idle_poll", || self.poll(lifecycle))?;
                Ok(lifecycle.candidate_completion_ready())
            },
            || started.elapsed(),
            std::thread::park_timeout,
        )
    }

    /// Reuse the product HTTP transport with an explicitly selected execution
    /// lifecycle. This does not select a signer, database, or execution policy.
    #[cfg(test)]
    pub(crate) fn poll_with(&mut self, mut handle: impl FnMut(Value) -> Value) -> Result<()> {
        self.poll_batch_with(|requests| requests.into_iter().map(&mut handle).collect())
    }

    #[cfg(test)]
    fn poll_batch_with(&mut self, handle: impl FnMut(Vec<Value>) -> Vec<Value>) -> Result<()> {
        self.poll_batch_when(true, handle)
    }

    fn poll_batch_when(
        &mut self,
        storage_available: bool,
        mut handle: impl FnMut(Vec<Value>) -> Vec<Value>,
    ) -> Result<()> {
        if self
            .connections
            .iter()
            .any(|connection| connection.pending.is_some())
        {
            bail!("pending deferred RPC requires its original handler");
        }
        self.accept_connections()?;
        self.read_connections(|_| unreachable!("batch dispatcher has no deferred token"));
        let (positions, requests): (Vec<_>, Vec<_>) = self
            .connections
            .iter_mut()
            .enumerate()
            .filter_map(|(index, connection)| {
                // Keep complete requests owned here within the existing count,
                // byte and deadline budgets. Never report queued before fsync.
                // A pure control query does not call workspace/owner storage.
                if !storage_available
                    && connection
                        .request
                        .as_ref()
                        .is_some_and(|request| request["method"] != "nov_chainStatus")
                {
                    return None;
                }
                connection.take_request().map(|request| (index, request))
            })
            .unzip();
        if !requests.is_empty() {
            let responses = handle(requests);
            if responses.len() != positions.len() {
                bail!("RPC admission response count mismatch");
            }
            for (index, response) in positions.into_iter().zip(responses) {
                self.connections[index].output = Some(http_response(&response));
            }
        }
        self.connections.retain_mut(Connection::write_response);
        Ok(())
    }

    /// One bounded turn for every live connection. Slow owner replies remain
    /// pending without preventing unrelated control/query requests from running.
    /// The original connection count, request envelope and absolute deadline
    /// still apply; polling or completing work never renews the deadline.
    /// True reports real socket/dispatch/completion/retirement progress, not a
    /// pending job or WouldBlock. Partial request reads count without increasing
    /// the original single-16KiB-read quota per connection per turn.
    pub(crate) fn poll_deferred(&mut self, handler: &mut impl DeferredRpcHandler) -> Result<bool> {
        let mut progressed = self.accept_connections()?;
        progressed |= self.read_connections(|token| handler.cancel(token));
        for connection in &mut self.connections {
            if let Some(token) = connection.pending {
                if let Some(response) = handler.poll(token) {
                    progressed = true;
                    connection.pending = None;
                    connection.output = Some(http_response(&response));
                }
            } else if let Some(request) = connection.take_request() {
                progressed = true;
                match handler.start(request) {
                    DeferredRpcReply::Ready(response) => {
                        connection.output = Some(http_response(&response));
                    }
                    DeferredRpcReply::Pending(token) => {
                        if self
                            .last_deferred_token
                            .is_some_and(|previous| token <= previous)
                        {
                            handler.cancel(token);
                            bail!("deferred RPC token reused or out of order");
                        }
                        self.last_deferred_token = Some(token);
                        connection.pending = Some(token);
                    }
                }
            }
        }
        self.connections.retain_mut(|connection| {
            let keep = connection.write_response();
            progressed |= connection.io_progress || !keep;
            keep
        });
        Ok(progressed)
    }

    fn accept_connections(&mut self) -> Result<bool> {
        let mut progressed = false;
        for _ in 0..MAX_CONNECTIONS {
            match self.listener.accept() {
                Ok((stream, _)) if self.connections.len() < MAX_CONNECTIONS => {
                    stream.set_nonblocking(true)?;
                    self.connections.push(Connection {
                        stream,
                        input: Vec::new(),
                        request: None,
                        pending: None,
                        output: None,
                        written: 0,
                        opened: Instant::now(),
                        io_progress: false,
                    });
                    progressed = true;
                }
                Ok(_) => progressed = true,
                Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                Err(error) => return Err(error.into()),
            }
        }
        Ok(progressed)
    }

    fn read_connections(&mut self, mut cancel: impl FnMut(u64)) -> bool {
        let mut progressed = false;
        self.connections.retain_mut(|connection| {
            connection.io_progress = false;
            let keep = connection.opened.elapsed() <= DEADLINE
                && connection.read_request(self.max_request);
            progressed |= connection.io_progress || !keep;
            if !keep {
                if let Some(token) = connection.pending.take() {
                    cancel(token);
                }
            }
            keep
        });
        progressed
    }
}

// Clock/sleep closures keep budget and error tests deterministic; production
// uses one monotonic start. A real queued candidate completion wakes the owner
// without paying another full idle interval; RPC arrivals never reset a budget.
fn poll_during_idle_with(
    idle: Duration,
    mut poll: impl FnMut() -> Result<bool>,
    mut elapsed: impl FnMut() -> Duration,
    mut sleep: impl FnMut(Duration),
) -> Result<()> {
    while elapsed() < idle {
        if poll()? {
            break;
        }
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

    #[derive(Default)]
    struct DeferredFixture {
        next_token: u64,
        starts: Vec<Value>,
        polls: Vec<u64>,
        cancelled: Vec<u64>,
        responses: std::collections::BTreeMap<u64, Value>,
    }

    impl DeferredRpcHandler for DeferredFixture {
        fn start(&mut self, request: Value) -> DeferredRpcReply {
            self.starts.push(request.clone());
            if request["method"] == "nov_chainStatus" {
                return DeferredRpcReply::Ready(
                    json!({"jsonrpc":"2.0","id":request["id"],"result":{"responsive":true}}),
                );
            }
            self.next_token += 1;
            DeferredRpcReply::Pending(self.next_token)
        }

        fn poll(&mut self, token: u64) -> Option<Value> {
            self.polls.push(token);
            self.responses.remove(&token)
        }

        fn cancel(&mut self, token: u64) {
            self.cancelled.push(token);
            self.responses.remove(&token);
        }
    }

    fn deferred_client(address: SocketAddr, method: &str, id: u64) -> TcpStream {
        let mut stream = TcpStream::connect(address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let body =
            serde_json::to_vec(&json!({"jsonrpc":"2.0","id":id,"method":method,"params":[]}))
                .unwrap();
        let mut packet =
            format!("POST / HTTP/1.1\r\nContent-Length: {}\r\n\r\n", body.len()).into_bytes();
        packet.extend_from_slice(&body);
        stream.write_all(&packet).unwrap();
        stream
    }

    fn deferred_response(stream: &mut TcpStream) -> Value {
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        serde_json::from_str(response.split_once("\r\n\r\n").unwrap().1).unwrap()
    }

    #[test]
    fn deferred_owner_response_does_not_block_other_http_connections_or_dispatch_twice() {
        let mut server = FreshRpcServer::bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let address = server.local_addr().unwrap();
        let mut handler = DeferredFixture::default();
        let mut submit = deferred_client(address, "nov_sendRawTransaction", 11);
        server.poll_deferred(&mut handler).unwrap();
        assert_eq!(server.connections.len(), 1);
        assert_eq!(server.connections[0].pending, Some(1));
        assert!(server.connections[0].request.is_none() && server.connections[0].output.is_none());
        assert_eq!(
            server.connections[0].input.capacity(),
            0,
            "raw request retained after owner takeover"
        );

        let mut control = deferred_client(address, "nov_chainStatus", 22);
        server.poll_deferred(&mut handler).unwrap();
        assert_eq!(
            deferred_response(&mut control),
            json!({"jsonrpc":"2.0","id":22,"result":{"responsive":true}})
        );
        assert_eq!(handler.starts.len(), 2);
        assert_eq!(handler.polls, [1]);
        assert_eq!(server.connections.len(), 1);
        assert!(
            server.connections[0].output.is_none(),
            "pending admission fabricated a response"
        );
        let response = json!({"jsonrpc":"2.0","id":11,"result":{"status":"received"}});
        handler.responses.insert(1, response.clone());
        server.poll_deferred(&mut handler).unwrap();
        assert_eq!(deferred_response(&mut submit), response);
        assert_eq!(
            handler.starts.len(),
            2,
            "original request dispatched again while pending"
        );
        assert!(handler.cancelled.is_empty());
        assert!(server.connections.is_empty());
    }

    #[test]
    fn deferred_timeout_cancels_once_and_recycled_connection_slot_cannot_receive_old_reply() {
        let mut server = FreshRpcServer::bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let address = server.local_addr().unwrap();
        let mut handler = DeferredFixture::default();
        let mut old = deferred_client(address, "nov_sendRawTransaction", 11);
        server.poll_deferred(&mut handler).unwrap();
        server.connections[0].opened = Instant::now() - DEADLINE - Duration::from_secs(1);
        server.poll_deferred(&mut handler).unwrap();
        assert_eq!(handler.cancelled, [1]);
        assert!(
            handler.polls.is_empty(),
            "expired job was polled before cancellation"
        );
        assert!(server.connections.is_empty());
        let mut expired_reply = String::new();
        old.read_to_string(&mut expired_reply).unwrap();
        assert!(expired_reply.is_empty());

        // An execution may finish after delivery cancellation. Its old token
        // must not become the next connection's index or response authority.
        handler
            .responses
            .insert(1, json!({"id":11,"result":"late old completion"}));
        let mut new = deferred_client(address, "nov_sendRawTransaction", 33);
        server.poll_deferred(&mut handler).unwrap();
        assert_eq!(server.connections[0].pending, Some(2));
        server.poll_deferred(&mut handler).unwrap();
        assert_eq!(handler.polls, [2]);
        assert!(server.connections[0].output.is_none());
        let response = json!({"jsonrpc":"2.0","id":33,"result":{"status":"received"}});
        handler.responses.insert(2, response.clone());
        server.poll_deferred(&mut handler).unwrap();
        assert_eq!(deferred_response(&mut new), response);
        assert!(
            handler.responses.contains_key(&1),
            "new connection consumed old completion"
        );
        assert_eq!(handler.cancelled, [1]);
        assert!(server.connections.is_empty());
    }

    #[test]
    fn deferred_handler_cannot_reuse_an_expired_token() {
        let mut server = FreshRpcServer::bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let address = server.local_addr().unwrap();
        let mut handler = DeferredFixture::default();
        let _old = deferred_client(address, "nov_sendRawTransaction", 11);
        server.poll_deferred(&mut handler).unwrap();
        server.connections[0].opened = Instant::now() - DEADLINE - Duration::from_secs(1);
        server.poll_deferred(&mut handler).unwrap();
        handler.next_token = 0; // Deliberately broken handler, not an allowed sequence.
        let _new = deferred_client(address, "nov_sendRawTransaction", 22);
        let error = server.poll_deferred(&mut handler).unwrap_err();
        assert!(error.to_string().contains("token reused"));
        assert_eq!(handler.cancelled, [1, 1]);
        assert!(server
            .connections
            .iter()
            .all(|connection| connection.pending.is_none() && connection.output.is_none()));
    }

    #[test]
    fn deferred_waiting_is_not_progress_but_start_and_ready_are() {
        let mut server = FreshRpcServer::bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let mut handler = DeferredFixture::default();
        assert!(!server.poll_deferred(&mut handler).unwrap());
        let mut client = deferred_client(server.local_addr().unwrap(), "nov_sendRawTransaction", 1);
        assert!(server.poll_deferred(&mut handler).unwrap());
        assert_eq!(server.connections[0].pending, Some(1));
        for _ in 0..3 {
            assert!(
                !server.poll_deferred(&mut handler).unwrap(),
                "waiting owner job fabricated IO progress"
            );
        }
        assert_eq!(handler.starts.len(), 1);
        handler
            .responses
            .insert(1, json!({"id":1,"result":"completed"}));
        assert!(server.poll_deferred(&mut handler).unwrap());
        assert_eq!(deferred_response(&mut client)["result"], "completed");
        assert!(!server.poll_deferred(&mut handler).unwrap());
    }

    #[test]
    fn deferred_partial_http_reads_report_real_progress_without_expanding_read_quota() {
        let mut server = FreshRpcServer::bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let mut handler = DeferredFixture::default();
        let mut client = TcpStream::connect(server.local_addr().unwrap()).unwrap();
        client.set_nodelay(true).unwrap();
        assert!(
            server.poll_deferred(&mut handler).unwrap(),
            "accept is real progress"
        );
        assert!(server.connections[0].input.is_empty());
        assert!(!server.poll_deferred(&mut handler).unwrap());

        let mut partial = b"POST / HTTP/1.1\r\nContent-Length: 65536\r\n\r\n{".to_vec();
        partial.extend(std::iter::repeat_n(b' ', 32 * 1024));
        client.write_all(&partial).unwrap();
        let started = Instant::now();
        let mut read_turns = 0;
        while server.connections[0].input.len() < partial.len() {
            assert!(started.elapsed() < Duration::from_secs(1));
            let before = server.connections[0].input.len();
            let progress = server.poll_deferred(&mut handler).unwrap();
            let read = server.connections[0].input.len() - before;
            assert!(
                read <= 16 * 1024,
                "one poll drained beyond the original per-connection quota"
            );
            assert_eq!(
                progress,
                read > 0,
                "partial read progress differs from actual bytes"
            );
            read_turns += usize::from(read > 0);
        }
        assert!(read_turns >= 3);
        assert!(
            handler.starts.is_empty(),
            "incomplete request reached execution"
        );
        assert!(
            !server.poll_deferred(&mut handler).unwrap(),
            "WouldBlock fabricated progress"
        );
    }

    #[test]
    fn explicit_batch_envelope_keeps_original_http_limit_and_framing() {
        let payload = vec![b' '; MAX_REQUEST];
        let mut request = format!(
            "POST / HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
            payload.len()
        )
        .into_bytes();
        request.extend_from_slice(&payload);
        assert!(body(&request).is_err());
        assert_eq!(
            body_bounded(&request, 4 * 1024 * 1024)
                .unwrap()
                .unwrap()
                .len(),
            payload.len()
        );
        request.push(b'x');
        assert!(body_bounded(&request, 4 * 1024 * 1024).is_err());
        assert!(FreshRpcServer::bind_with_max_request(
            "127.0.0.1:0".parse().unwrap(),
            8 * 1024 * 1024
        )
        .is_err());
    }

    #[test]
    fn finalized_balance_params_reuse_public_nov_method_without_store_override() {
        for length in [20, 32] {
            let account = "AB".repeat(length);
            for prefix in ["", "0x"] {
                assert_eq!(
                    nov_balance_account(&json!({"account":format!("{prefix}{account}")})).unwrap(),
                    format!("0x{}", account.to_ascii_lowercase())
                );
            }
        }
        let account = "0x".to_owned() + &"12".repeat(20);
        assert!(nov_balance_account(&json!({"account":account,"asset_id":"nov"})).is_ok());
        for params in [
            Value::Null,
            json!([account]),
            json!({}),
            json!({"account":7}),
            json!({"account":"zz".repeat(20)}),
            json!({"account":"12".repeat(19)}),
            json!({"account":"12".repeat(33)}),
            json!({"account":"啊".repeat(20)}),
            json!({"account":account,"asset":"NUSD"}),
            json!({"account":account,"asset":"NOV","asset_id":"ETH"}),
            json!({"account":account,"asset":null}),
            json!({"account":account,"store_path":"foreign.json"}),
            json!({"account":account,"asset_view_authorized":true}),
        ] {
            assert!(nov_balance_account(&params).is_err(), "accepted {params}");
        }
    }

    #[test]
    fn ready_submission_groups_preserve_query_barriers_and_response_ids() {
        let request = |method, id| json!({"jsonrpc":"2.0","method":method,"id":id,"params":["12"]});
        let requests = vec![
            request("nov_sendRawTransaction", Value::Null),
            request("nov_sendRawTransaction", json!("second")),
            request("nov_chainStatus", json!(3)),
            request("nov_sendRawTransaction", json!(4)),
        ];
        let mut groups = Vec::new();
        let mut admitted = 0;
        let responses = coalesce_ready_requests(requests, |group| {
            groups.push(group.len());
            if group[0]["method"] == "nov_sendRawTransaction" {
                admitted += group.len();
            }
            group
                .into_iter()
                .map(|request| json!({"id":request["id"],"admitted":admitted}))
                .collect()
        });
        assert_eq!(groups, [2, 1, 1]);
        assert_eq!(
            responses
                .iter()
                .map(|response| response["id"].clone())
                .collect::<Vec<_>>(),
            [Value::Null, json!("second"), json!(3), json!(4)]
        );
        assert_eq!(
            responses[2]["admitted"], 2,
            "query must not observe the later submit"
        );
        assert_eq!(responses[3]["admitted"], 3);
    }

    #[test]
    fn coalesced_submission_parse_errors_keep_per_item_ids_and_outcomes() {
        let requests = vec![
            json!({"jsonrpc":"2.0","method":"nov_sendRawTransaction","id":"a","params":["12"]}),
            json!({"jsonrpc":"1.0","method":"nov_sendRawTransaction","id":2,"params":["12"]}),
            json!({"jsonrpc":"2.0","method":"nov_sendRawTransaction","id":3,"params":["12","34"]}),
            json!({"jsonrpc":"2.0","method":"nov_sendRawTransaction","id":4,"params":["zz"]}),
            json!({"jsonrpc":"2.0","method":"nov_sendRawTransaction","id":5,"params":["34"]}),
        ];
        let responses = handle_submit_requests(requests, |raws| {
            assert_eq!(raws, [vec![0x12], vec![0x34]]);
            vec![
                Ok(json!({"status":"queued"})),
                Err(anyhow::anyhow!(
                    "transaction pool capacity or signer nonce conflict"
                )),
            ]
        });
        assert_eq!(responses[0]["id"], "a");
        assert_eq!(responses[0]["result"]["status"], "queued");
        for (index, message) in [
            (1, "invalid JSON-RPC request"),
            (2, "one raw transaction required"),
            (4, "transaction pool capacity or signer nonce conflict"),
        ] {
            assert_eq!(responses[index]["error"]["message"], message);
        }
        for (index, response) in responses.iter().enumerate().skip(1) {
            assert_eq!(response["id"], index + 1);
            assert_eq!(response["error"]["code"], -32000);
            assert!(response.get("result").is_none());
        }
    }

    #[test]
    fn ready_http_connections_share_one_dispatch_and_keep_response_routing() {
        let mut server = FreshRpcServer::bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let address = server.local_addr().unwrap();
        let mut clients = Vec::new();
        for id in 0..4 {
            let mut stream = TcpStream::connect(address).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(1)))
                .unwrap();
            let request = serde_json::to_vec(
                &json!({"jsonrpc":"2.0","method":"nov_sendRawTransaction","id":id,"params":["12"]}),
            )
            .unwrap();
            let mut packet = format!(
                "POST / HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
                request.len()
            )
            .into_bytes();
            packet.extend_from_slice(&request);
            stream.write_all(&packet).unwrap();
            clients.push(stream);
        }
        let mut calls = 0;
        server.poll_batch_with(|requests| {
            calls += 1;
            assert_eq!(requests.len(), 4);
            requests.into_iter().map(|request| json!({"jsonrpc":"2.0","id":request["id"],"result":{"status":"queued"}})).collect()
        }).unwrap();
        assert_eq!(calls, 1);
        for (id, stream) in clients.iter_mut().enumerate() {
            let mut response = String::new();
            stream.read_to_string(&mut response).unwrap();
            let response: Value =
                serde_json::from_str(response.split_once("\r\n\r\n").unwrap().1).unwrap();
            assert_eq!(response["id"], id);
            assert_eq!(response["result"]["status"], "queued");
        }
        assert!(server.connections.is_empty());
    }

    #[test]
    fn durable_stage_defers_state_requests_but_serves_control_without_early_ack() {
        let mut server = FreshRpcServer::bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let address = server.local_addr().unwrap();
        let send = |method: &str, id: u64| {
            let mut stream = TcpStream::connect(address).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(1)))
                .unwrap();
            let body = serde_json::to_vec(
                &json!({"jsonrpc":"2.0","id":id,"method":method,"params":["12"]}),
            )
            .unwrap();
            let mut packet =
                format!("POST / HTTP/1.1\r\nContent-Length: {}\r\n\r\n", body.len()).into_bytes();
            packet.extend_from_slice(&body);
            stream.write_all(&packet).unwrap();
            stream
        };
        let mut submit = send("nov_sendRawTransaction", 1);
        let mut query = send("nov_getTransactionStatus", 2);
        let mut control = send("nov_chainStatus", 3);
        let mut controls = 0;
        server
            .poll_batch_when(false, |requests| {
                assert_eq!(requests.len(), 1);
                assert_eq!(requests[0]["method"], "nov_chainStatus");
                controls += 1;
                vec![
                    json!({"jsonrpc":"2.0","id":3,"result":{"candidate_durability_inflight":true}}),
                ]
            })
            .unwrap();
        assert_eq!(controls, 1);
        assert_eq!(server.connections.len(), 2);
        assert!(server
            .connections
            .iter()
            .all(|c| c.request.is_some() && c.output.is_none()));
        let mut response = String::new();
        control.read_to_string(&mut response).unwrap();
        assert!(response.contains("candidate_durability_inflight"));
        server
            .poll_batch_when(false, |_| {
                panic!("state requests escaped durable backpressure")
            })
            .unwrap();
        server
            .poll_batch_when(true, |requests| {
                assert_eq!(
                    requests
                        .iter()
                        .map(|r| r["id"].as_u64().unwrap())
                        .collect::<Vec<_>>(),
                    [1, 2]
                );
                requests
                    .into_iter()
                    .map(|r| json!({"jsonrpc":"2.0","id":r["id"],"result":{"status":"queued"}}))
                    .collect()
            })
            .unwrap();
        for stream in [&mut submit, &mut query] {
            let mut response = String::new();
            stream.read_to_string(&mut response).unwrap();
            assert!(response.contains("queued"));
        }
        assert!(server.connections.is_empty());
    }

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
                Ok(false)
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
                Ok(false)
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
                Ok(false)
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
    fn real_candidate_completion_ends_idle_before_another_pause() {
        let mut polls = 0;
        poll_during_idle_with(
            Duration::from_secs(30),
            || {
                polls += 1;
                Ok(true)
            },
            || Duration::ZERO,
            |_| panic!("ready candidate delayed by idle budget"),
        )
        .unwrap();
        assert_eq!(polls, 1);
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
