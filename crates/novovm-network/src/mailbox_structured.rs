//! Explicit NVMBOX07 application schema, NOT an in-place change to NVMBOX01-06.
//! APFL shared/dictionary/dense columns reuse the original Transfer machinery.
//! Ciphertext remains opaque; there is no NOV transaction, AOEM opcode or new DB.
//! This is the codec/owner adapter, not automatic mobile routing or admission.
use crate::mailbox_receipt::StoreReceiptV1;
use crate::mailbox_rpc::{
    MailboxNoticeV1, MailboxPrefetchV6, MailboxRejectionV1, MailboxReplyV1 as Reply,
    MailboxRequestV1 as Request, MailboxServiceV1, MailboxWatchV1,
};
use crate::mailbox_store::MailboxCursorV1;
use anyhow::{ensure, Context, Result};
use novovm_protocol::{
    apfl_batch::{self, BatchView, Limits},
    apfl_column::Column,
};
use zeroize::Zeroizing;

pub(crate) mod locator;
pub mod native_requests;
pub mod native_session;

pub const STREAM: u64 = u64::from_be_bytes(*b"NVMBOX07");
pub const REQUEST_SCHEMA: u64 = u64::from_be_bytes(*b"NVMREQ07");
pub const REPLY_SCHEMA: u64 = u64::from_be_bytes(*b"NVMRES07");
pub const LIMITS: Limits = Limits {
    rows: 16,
    columns: 8,
    field_bytes: 65_536,
    wire_bytes: 1_100_000,
    expanded_bytes: 1_100_000,
};
pub const BULK_REQUEST_SCHEMA: u64 = u64::from_be_bytes(*b"NVMBRQ01");
pub const BULK_REPLY_SCHEMA: u64 = u64::from_be_bytes(*b"NVMBRS01");
pub const BULK_LIMITS: Limits = Limits {
    rows: 1,
    columns: 8,
    field_bytes: crate::mailbox_store::MAX_BULK_CIPHERTEXT,
    wire_bytes: crate::mailbox_store::MAX_BULK_CIPHERTEXT + 4096,
    expanded_bytes: crate::mailbox_store::MAX_BULK_CIPHERTEXT + 4096,
};
fn wire_limits(schema: u64) -> Limits {
    if matches!(schema, BULK_REQUEST_SCHEMA | BULK_REPLY_SCHEMA) {
        BULK_LIMITS
    } else {
        LIMITS
    }
}
fn wire_schema(bytes: &[u8], ordinary: u64, bulk: u64) -> Result<u64> {
    let schema = u64::from_le_bytes(fixed(bytes.get(16..24).context("mailbox schema header")?)?);
    ensure!(
        schema == ordinary || schema == bulk,
        "mailbox schema/version"
    );
    Ok(schema)
}
type Fields<'a> = [&'a [u8]; 8];

fn build(schema: u64, context: [u8; 32], rows: &[Fields<'_>]) -> Result<Zeroizing<Vec<u8>>> {
    let limits = wire_limits(schema);
    ensure!(
        context != [0; 32] && !rows.is_empty() && rows.len() <= limits.rows,
        "mailbox structured count/context"
    );
    let columns: Vec<_> = (0..8)
        .map(|i| Column::collect(rows.iter().map(|r| r[i])))
        .collect();
    Ok(Zeroizing::new(
        apfl_batch::encode(schema, context, rows.len(), &columns, limits)
            .map_err(anyhow::Error::msg)?,
    ))
}
fn fields<'a>(batch: &BatchView<'a>, row: usize) -> Result<Fields<'a>> {
    let mut result: Fields<'a> = [&[]; 8];
    for (col, value) in result.iter_mut().enumerate() {
        *value = batch.field(row, col).map_err(anyhow::Error::msg)?;
    }
    Ok(result)
}
fn shape(f: &Fields<'_>, widths: [Option<usize>; 8]) -> Result<()> {
    for (value, width) in f.iter().zip(widths) {
        if let Some(width) = width {
            ensure!(value.len() == width, "mailbox field width/unused field");
        }
    }
    Ok(())
}
fn fixed<const N: usize>(bytes: &[u8]) -> Result<[u8; N]> {
    bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("mailbox field width"))
}
fn i64_value(bytes: &[u8]) -> Result<i64> {
    Ok(i64::from_le_bytes(fixed(bytes)?))
}
fn u64_value(bytes: &[u8]) -> Result<u64> {
    Ok(u64::from_le_bytes(fixed(bytes)?))
}

// Watch descriptors have at most four entries; only this small variable field
// is materialized. Put ciphertext is borrowed into the shared column encoder.
fn watch_bytes(boxes: &[MailboxWatchV1]) -> Result<Zeroizing<Vec<u8>>> {
    ensure!(!boxes.is_empty() && boxes.len() <= 4, "mailbox watch count");
    let mut out = Zeroizing::new(vec![boxes.len() as u8]);
    for b in boxes {
        out.extend_from_slice(&b.mailbox);
        out.extend_from_slice(&b.read);
        out.push(u8::from(b.after.is_some()));
        if let Some(cursor) = b.after {
            out.extend_from_slice(&cursor.generation);
            out.extend_from_slice(&cursor.sequence.to_le_bytes());
        }
    }
    Ok(out)
}
struct Reader<'a>(&'a [u8]);
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let out = self.0.get(..n).context("mailbox truncated field")?;
        self.0 = &self.0[n..];
        Ok(out)
    }
    fn byte(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn end(self) -> Result<()> {
        ensure!(self.0.is_empty(), "mailbox trailing field");
        Ok(())
    }
}
fn watches(bytes: &[u8]) -> Result<Vec<MailboxWatchV1>> {
    let mut r = Reader(bytes);
    let count = r.byte()? as usize;
    ensure!((1..=4).contains(&count), "mailbox watch count");
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let mailbox = fixed(r.take(32)?)?;
        let read = fixed(r.take(32)?)?;
        let after = match r.byte()? {
            0 => None,
            1 => Some(MailboxCursorV1 {
                generation: fixed(r.take(32)?)?,
                sequence: u64_value(r.take(8)?)?,
            }),
            _ => anyhow::bail!("mailbox cursor tag"),
        };
        out.push(MailboxWatchV1 {
            mailbox,
            read,
            after,
        });
    }
    r.end()?;
    Ok(out)
}

pub fn encode_requests(context: [u8; 32], requests: &[Request]) -> Result<Zeroizing<Vec<u8>>> {
    ensure!(
        !requests.is_empty() && requests.len() <= LIMITS.rows,
        "mailbox request count"
    );
    let mut scalars = Vec::with_capacity(requests.len());
    let mut aux = Vec::with_capacity(requests.len());
    let mut tags = Vec::with_capacity(requests.len());
    for req in requests {
        let (tag, number) = match req {
            Request::EndpointDeliverV1 { expires, .. } => (14, *expires),
            Request::ServiceRecordV7 => (9, 0),
            Request::ProbeBulkV1 => (10, 0),
            Request::PutBulkV1 { expires, .. } => (11, *expires),
            Request::FetchBulkV1 { .. } => (12, 0),
            Request::WatchFetchBulkV1 { wait_ms, .. } => (13, i64::from(*wait_ms)),
            Request::Register { expires, .. } => (1, *expires),
            Request::Put { expires, .. } => (2, *expires),
            Request::Fetch { .. } => (3, 0),
            Request::Consume { .. } => (4, 0),
            Request::Renew { ttl, .. } => (5, *ttl),
            Request::Revoke { .. } => (6, 0),
            Request::Watch { wait_ms, .. } => (7, i64::from(*wait_ms)),
            Request::WatchFetchV6 { wait_ms, .. } => (8, i64::from(*wait_ms)),
            _ => anyhow::bail!("native batch does not relabel legacy negotiation"),
        };
        tags.push([tag]);
        scalars.push(number.to_le_bytes());
        aux.push(match req {
            Request::Watch { boxes, .. }
            | Request::WatchFetchV6 { boxes, .. }
            | Request::WatchFetchBulkV1 { boxes, .. } => watch_bytes(boxes)?,
            _ => Zeroizing::new(Vec::new()),
        });
    }
    let mut rows = Vec::with_capacity(requests.len());
    for (i, req) in requests.iter().enumerate() {
        let mut f: Fields<'_> = [&[]; 8];
        f[0] = &tags[i];
        match req {
            Request::ServiceRecordV7 | Request::ProbeBulkV1 => {}
            Request::Register {
                mailbox,
                write,
                read,
                ..
            } => {
                f[1] = mailbox;
                f[2] = write;
                f[3] = read;
                f[5] = &scalars[i];
            }
            Request::Put {
                mailbox,
                write,
                object,
                ciphertext,
                ..
            }
            | Request::EndpointDeliverV1 {
                mailbox,
                write,
                object,
                ciphertext,
                ..
            }
            | Request::PutBulkV1 {
                mailbox,
                write,
                object,
                ciphertext,
                ..
            } => {
                f[1] = mailbox;
                f[2] = write;
                f[4] = object;
                f[5] = &scalars[i];
                f[6] = ciphertext;
            }
            Request::Fetch { mailbox, read }
            | Request::FetchBulkV1 { mailbox, read }
            | Request::Revoke { mailbox, read } => {
                f[1] = mailbox;
                f[2] = read;
            }
            Request::Consume {
                mailbox,
                read,
                object,
                digest,
            } => {
                f[1] = mailbox;
                f[2] = read;
                f[4] = object;
                f[7] = digest;
            }
            Request::Renew { mailbox, read, .. } => {
                f[1] = mailbox;
                f[2] = read;
                f[5] = &scalars[i];
            }
            Request::Watch { .. }
            | Request::WatchFetchV6 { .. }
            | Request::WatchFetchBulkV1 { .. } => {
                f[5] = &scalars[i];
                f[6] = &aux[i];
            }
            _ => unreachable!(),
        }
        validate_request(&f)?;
        rows.push(f);
    }
    build(
        if tags.iter().any(|t| matches!(t[0], 10..=13)) {
            BULK_REQUEST_SCHEMA
        } else {
            REQUEST_SCHEMA
        },
        context,
        &rows,
    )
}
fn validate_request(f: &Fields<'_>) -> Result<()> {
    ensure!(f[0].len() == 1, "mailbox operation tag");
    let s = Some;
    match f[0][0] {
        1 => shape(f, [s(1), s(32), s(32), s(32), s(0), s(8), s(0), s(0)])?,
        2 | 11 | 14 => {
            shape(f, [s(1), s(32), s(32), s(0), s(32), s(8), None, s(0)])?;
            ensure!(
                !f[6].is_empty()
                    && f[6].len()
                        <= if f[0][0] == 11 {
                            BULK_LIMITS.field_bytes
                        } else {
                            65_536
                        },
                "mailbox ciphertext bound"
            );
            if f[0][0] == 14 {
                ensure!(i64_value(f[5])? > 0, "endpoint delivery expiry");
            }
        }
        3 | 6 | 12 => shape(f, [s(1), s(32), s(32), s(0), s(0), s(0), s(0), s(0)])?,
        4 => shape(f, [s(1), s(32), s(32), s(0), s(32), s(0), s(0), s(32)])?,
        5 => shape(f, [s(1), s(32), s(32), s(0), s(0), s(8), s(0), s(0)])?,
        7 | 8 | 13 => {
            shape(f, [s(1), s(0), s(0), s(0), s(0), s(8), None, s(0)])?;
            ensure!(u64_value(f[5])? <= 10_000, "mailbox wait bound");
            watches(f[6])?;
        }
        9 | 10 => shape(f, [s(1), s(0), s(0), s(0), s(0), s(0), s(0), s(0)])?,
        _ => anyhow::bail!("unknown mailbox operation"),
    }
    Ok(())
}
/// Validates ALL row shapes before the owner performs the first operation.
/// Borrowed capabilities live only as long as the caller's protected buffer.
pub struct RequestBatch<'a>(BatchView<'a>);
impl<'a> RequestBatch<'a> {
    pub fn decode(bytes: &'a [u8], context: [u8; 32]) -> Result<Self> {
        let schema = wire_schema(bytes, REQUEST_SCHEMA, BULK_REQUEST_SCHEMA)?;
        let v =
            apfl_batch::decode(bytes, schema, wire_limits(schema)).map_err(anyhow::Error::msg)?;
        ensure!(
            context != [0; 32] && v.context == context && v.columns() == 8,
            "mailbox context/columns"
        );
        for row in 0..v.rows() {
            let f = fields(&v, row)?;
            validate_request(&f)?;
            ensure!(
                matches!(f[0][0], 10..=13) == (schema == BULK_REQUEST_SCHEMA),
                "mailbox operation schema"
            );
        }
        Ok(Self(v))
    }
    pub fn len(&self) -> usize {
        self.0.rows()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Inspect only fully validated row views, without cloning ciphertext.
    pub fn has_waiting_watch(&self) -> Result<bool> {
        for row in 0..self.len() {
            let f = fields(&self.0, row)?;
            if matches!(f[0][0], 7 | 8 | 13) && u64_value(f[5])? != 0 {
                return Ok(true);
            }
        }
        Ok(false)
    }
    /// Retained wire/reply estimate without cloning large ciphertext fields.
    pub fn retained_bytes_estimate(&self) -> Result<usize> {
        let mut total = 4096usize;
        for row in 0..self.len() {
            let f = fields(&self.0, row)?;
            let bytes = match f[0][0] {
                2 | 11 | 14 => f[6].len().saturating_mul(3).saturating_add(4096),
                12 | 13 => BULK_LIMITS.wire_bytes.saturating_mul(2),
                3 | 8 => LIMITS.wire_bytes.saturating_mul(2),
                _ => 8192,
            };
            total = total.saturating_add(bytes);
        }
        Ok(total)
    }
    pub fn request(&self, index: usize) -> Result<Request> {
        let f = fields(&self.0, index)?;
        Ok(match f[0][0] {
            14 => Request::EndpointDeliverV1 {
                mailbox: fixed(f[1])?,
                write: fixed(f[2])?,
                object: fixed(f[4])?,
                expires: i64_value(f[5])?,
                ciphertext: f[6].to_vec(),
            },
            9 => Request::ServiceRecordV7,
            10 => Request::ProbeBulkV1,
            11 => Request::PutBulkV1 {
                mailbox: fixed(f[1])?,
                write: fixed(f[2])?,
                object: fixed(f[4])?,
                expires: i64_value(f[5])?,
                ciphertext: f[6].to_vec(),
            },
            12 => Request::FetchBulkV1 {
                mailbox: fixed(f[1])?,
                read: fixed(f[2])?,
            },
            13 => Request::WatchFetchBulkV1 {
                boxes: watches(f[6])?,
                wait_ms: u64_value(f[5])? as u32,
            },
            1 => Request::Register {
                mailbox: fixed(f[1])?,
                write: fixed(f[2])?,
                read: fixed(f[3])?,
                expires: i64_value(f[5])?,
            },
            2 => Request::Put {
                mailbox: fixed(f[1])?,
                write: fixed(f[2])?,
                object: fixed(f[4])?,
                expires: i64_value(f[5])?,
                ciphertext: f[6].to_vec(),
            },
            3 => Request::Fetch {
                mailbox: fixed(f[1])?,
                read: fixed(f[2])?,
            },
            4 => Request::Consume {
                mailbox: fixed(f[1])?,
                read: fixed(f[2])?,
                object: fixed(f[4])?,
                digest: fixed(f[7])?,
            },
            5 => Request::Renew {
                mailbox: fixed(f[1])?,
                read: fixed(f[2])?,
                ttl: i64_value(f[5])?,
            },
            6 => Request::Revoke {
                mailbox: fixed(f[1])?,
                read: fixed(f[2])?,
            },
            7 => Request::Watch {
                boxes: watches(f[6])?,
                wait_ms: u64_value(f[5])? as u32,
            },
            8 => Request::WatchFetchV6 {
                boxes: watches(f[6])?,
                wait_ms: u64_value(f[5])? as u32,
            },
            _ => unreachable!(),
        })
    }
}

fn notice_bytes(notices: &[MailboxNoticeV1]) -> Result<Vec<u8>> {
    ensure!(notices.len() <= 4, "mailbox notice count");
    let mut out = vec![notices.len() as u8];
    for n in notices {
        out.extend_from_slice(&n.cursor.generation);
        out.extend_from_slice(&n.cursor.sequence.to_le_bytes());
        out.push(u8::from(n.pending));
    }
    Ok(out)
}
fn notices(bytes: &[u8]) -> Result<Vec<MailboxNoticeV1>> {
    let mut r = Reader(bytes);
    let count = r.byte()? as usize;
    ensure!(count <= 4, "mailbox notice count");
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let cursor = MailboxCursorV1 {
            generation: fixed(r.take(32)?)?,
            sequence: u64_value(r.take(8)?)?,
        };
        let pending = match r.byte()? {
            0 => false,
            1 => true,
            _ => anyhow::bail!("mailbox bool"),
        };
        out.push(MailboxNoticeV1 { cursor, pending });
    }
    r.end()?;
    Ok(out)
}
pub fn encode_replies(context: [u8; 32], replies: &[Reply]) -> Result<Zeroizing<Vec<u8>>> {
    ensure!(
        !replies.is_empty() && replies.len() <= LIMITS.rows,
        "mailbox reply count"
    );
    let bulk = replies.iter().any(|r| match r {
        Reply::BulkReady { .. } => true,
        Reply::Object { ciphertext, .. } => ciphertext.len() > 65_536,
        Reply::NoticeObjectV6 {
            object: Some(o), ..
        } => o.ciphertext.len() > 65_536,
        _ => false,
    });
    ensure!(
        !bulk
            || !replies
                .iter()
                .any(|r| matches!(r, Reply::EndpointDeliveredV1 { .. })),
        "endpoint delivery is a small-operation schema"
    );
    let mut tags = Vec::new();
    let mut numbers = Vec::new();
    let mut aux = Vec::new();
    for reply in replies {
        let (tag, num, extra) = match reply {
            Reply::EndpointDeliveredV1 {
                receipt_expires, ..
            } => (13, receipt_expires.to_le_bytes().to_vec(), Vec::new()),
            Reply::BulkReady { max_ciphertext } => {
                (12, max_ciphertext.to_le_bytes().to_vec(), Vec::new())
            }
            Reply::ServiceRecordV7 { record } => (11, Vec::new(), locator::encode(record)?),
            Reply::Registered => (1, Vec::new(), Vec::new()),
            Reply::Stored(r) => {
                let mut n = vec![r.version];
                n.extend_from_slice(&r.bytes.to_le_bytes());
                n.extend_from_slice(&r.expires.to_le_bytes());
                (2, n, Vec::new())
            }
            Reply::Object { expires, .. } => (3, expires.to_le_bytes().to_vec(), Vec::new()),
            Reply::Empty => (4, Vec::new(), Vec::new()),
            Reply::Consumed => (5, Vec::new(), Vec::new()),
            Reply::Renewed => (6, Vec::new(), Vec::new()),
            Reply::Revoked => (7, Vec::new(), Vec::new()),
            Reply::Rejected(reason) => (
                8,
                vec![match reason {
                    MailboxRejectionV1::Unauthorized => 1,
                    MailboxRejectionV1::Invalid => 2,
                    MailboxRejectionV1::Quota => 3,
                    MailboxRejectionV1::Conflict => 4,
                    MailboxRejectionV1::Consumed => 5,
                    MailboxRejectionV1::Storage => 6,
                    MailboxRejectionV1::Unavailable => 7,
                }],
                Vec::new(),
            ),
            Reply::Notice(n) => (9, Vec::new(), notice_bytes(n)?),
            Reply::NoticeObjectV6 { notices, object } => {
                let mut n = Vec::new();
                if let Some(o) = object {
                    n.extend_from_slice(&u32::try_from(o.box_index)?.to_le_bytes());
                    n.extend_from_slice(&o.expires.to_le_bytes());
                }
                (10, n, notice_bytes(notices)?)
            }
            _ => anyhow::bail!("native reply cannot relabel legacy session negotiation"),
        };
        tags.push([tag]);
        numbers.push(num);
        aux.push(extra);
    }
    let mut rows = Vec::with_capacity(replies.len());
    for (i, reply) in replies.iter().enumerate() {
        let mut f: Fields<'_> = [&[]; 8];
        f[0] = &tags[i];
        match reply {
            Reply::EndpointDeliveredV1 {
                object,
                receipt_object,
                receipt,
                ..
            } => {
                f[1] = object;
                f[2] = receipt_object;
                f[3] = &numbers[i];
                f[4] = receipt;
            }
            Reply::Stored(r) => {
                f[1] = &r.node;
                f[2] = &r.mailbox;
                f[3] = &r.object;
                f[4] = &r.digest;
                f[5] = &numbers[i];
                f[6] = &r.signature;
            }
            Reply::Object {
                object,
                digest,
                ciphertext,
                ..
            } => {
                f[1] = object;
                f[2] = digest;
                f[3] = &numbers[i];
                f[4] = ciphertext;
            }
            Reply::Rejected(_) | Reply::BulkReady { .. } => f[1] = &numbers[i],
            Reply::Notice(_) | Reply::ServiceRecordV7 { .. } => f[1] = &aux[i],
            Reply::NoticeObjectV6 { object, .. } => {
                f[1] = &aux[i];
                if let Some(o) = object {
                    f[2] = &o.object;
                    f[3] = &o.digest;
                    f[4] = &o.ciphertext;
                    f[5] = &numbers[i];
                }
            }
            _ => {}
        }
        validate_reply_bound(
            &f,
            if bulk {
                BULK_LIMITS.field_bytes
            } else {
                LIMITS.field_bytes
            },
        )?;
        rows.push(f);
    }
    build(
        if bulk {
            BULK_REPLY_SCHEMA
        } else {
            REPLY_SCHEMA
        },
        context,
        &rows,
    )
}
fn validate_reply_bound(f: &Fields<'_>, object_limit: usize) -> Result<()> {
    ensure!(f[0].len() == 1, "mailbox reply tag");
    let s = Some;
    match f[0][0] {
        1 | 4 | 5 | 6 | 7 => shape(f, [s(1), s(0), s(0), s(0), s(0), s(0), s(0), s(0)])?,
        2 => {
            shape(f, [s(1), s(32), s(32), s(32), s(32), s(17), s(64), s(0)])?;
            ensure!(f[5][0] == 1, "storage receipt version");
        }
        13 => {
            shape(f, [s(1), s(32), s(32), s(8), None, s(0), s(0), s(0)])?;
            ensure!(
                fixed::<32>(f[2])? != [0; 32]
                    && i64_value(f[3])? > 0
                    && !f[4].is_empty()
                    && f[4].len() <= crate::endpoint_delivery::MAX_ENDPOINT_DELIVERY_BYTES,
                "endpoint receipt bounds"
            );
        }
        3 => {
            shape(f, [s(1), s(32), s(32), s(8), None, s(0), s(0), s(0)])?;
            ensure!(
                !f[4].is_empty() && f[4].len() <= object_limit,
                "mailbox object size"
            );
        }
        8 => {
            shape(f, [s(1), s(1), s(0), s(0), s(0), s(0), s(0), s(0)])?;
            ensure!((1..=7).contains(&f[1][0]), "mailbox rejection tag");
        }
        9 => {
            shape(f, [s(1), None, s(0), s(0), s(0), s(0), s(0), s(0)])?;
            notices(f[1])?;
        }
        10 => {
            let n = notices(f[1])?;
            if f[2].is_empty() {
                shape(f, [s(1), None, s(0), s(0), s(0), s(0), s(0), s(0)])?;
            } else {
                shape(f, [s(1), None, s(32), s(32), None, s(12), s(0), s(0)])?;
                ensure!(
                    !f[4].is_empty()
                        && f[4].len() <= object_limit
                        && (u32::from_le_bytes(fixed(&f[5][..4])?) as usize) < n.len(),
                    "mailbox prefetch bounds"
                );
            }
        }
        12 => {
            shape(f, [s(1), s(4), s(0), s(0), s(0), s(0), s(0), s(0)])?;
            ensure!(
                u32::from_le_bytes(fixed(f[1])?) as usize
                    == crate::mailbox_store::MAX_BULK_CIPHERTEXT,
                "bulk capability bound"
            );
        }
        11 => {
            shape(f, [s(1), None, s(0), s(0), s(0), s(0), s(0), s(0)])?;
            locator::decode(f[1])?;
        }
        _ => anyhow::bail!("unknown mailbox reply"),
    }
    Ok(())
}
pub fn decode_replies(bytes: &[u8], context: [u8; 32], expected: usize) -> Result<Vec<Reply>> {
    let schema = wire_schema(bytes, REPLY_SCHEMA, BULK_REPLY_SCHEMA)?;
    let batch =
        apfl_batch::decode(bytes, schema, wire_limits(schema)).map_err(anyhow::Error::msg)?;
    ensure!(
        context != [0; 32]
            && batch.context == context
            && batch.columns() == 8
            && batch.rows() == expected,
        "mailbox reply binding"
    );
    let mut out = Vec::with_capacity(expected);
    for row in 0..expected {
        let f = fields(&batch, row)?;
        validate_reply_bound(&f, wire_limits(schema).field_bytes)?;
        ensure!(
            f[0][0] != 12 || schema == BULK_REPLY_SCHEMA,
            "bulk capability schema"
        );
        ensure!(
            f[0][0] != 13 || schema == REPLY_SCHEMA,
            "endpoint receipt schema"
        );
        out.push(match f[0][0] {
            13 => Reply::EndpointDeliveredV1 {
                object: fixed(f[1])?,
                receipt_object: fixed(f[2])?,
                receipt_expires: i64_value(f[3])?,
                receipt: f[4].to_vec(),
            },
            12 => Reply::BulkReady {
                max_ciphertext: u32::from_le_bytes(fixed(f[1])?),
            },
            11 => Reply::ServiceRecordV7 {
                record: locator::decode(f[1])?,
            },
            1 => Reply::Registered,
            2 => Reply::Stored(StoreReceiptV1 {
                version: 1,
                node: fixed(f[1])?,
                mailbox: fixed(f[2])?,
                object: fixed(f[3])?,
                digest: fixed(f[4])?,
                bytes: u64_value(&f[5][1..9])?,
                expires: i64_value(&f[5][9..17])?,
                signature: f[6].to_vec(),
            }),
            3 => Reply::Object {
                object: fixed(f[1])?,
                digest: fixed(f[2])?,
                expires: i64_value(f[3])?,
                ciphertext: f[4].to_vec(),
            },
            4 => Reply::Empty,
            5 => Reply::Consumed,
            6 => Reply::Renewed,
            7 => Reply::Revoked,
            8 => Reply::Rejected(match f[1][0] {
                1 => MailboxRejectionV1::Unauthorized,
                2 => MailboxRejectionV1::Invalid,
                3 => MailboxRejectionV1::Quota,
                4 => MailboxRejectionV1::Conflict,
                5 => MailboxRejectionV1::Consumed,
                6 => MailboxRejectionV1::Storage,
                7 => MailboxRejectionV1::Unavailable,
                _ => unreachable!(),
            }),
            9 => Reply::Notice(notices(f[1])?),
            10 => Reply::NoticeObjectV6 {
                notices: notices(f[1])?,
                object: if f[2].is_empty() {
                    None
                } else {
                    Some(MailboxPrefetchV6 {
                        box_index: u32::from_le_bytes(fixed(&f[5][..4])?) as usize,
                        object: fixed(f[2])?,
                        digest: fixed(f[3])?,
                        ciphertext: f[4].to_vec(),
                        expires: i64_value(&f[5][4..])?,
                    })
                },
            },
            _ => unreachable!(),
        });
    }
    Ok(out)
}

/// LOCAL adapter of the same owner, not remote admission. The caller must bind
/// context to its authenticated request/session and keep its original identity,
/// revocation and path authority checks before/after each await. No new store or
/// entitlement. Rows execute in order, NOT one atomic batch: cancellation/error
/// after a commit is unknown outcome, not rollback. Original Put/Consume IDs
/// and receipts retain their existing retry/custody contract.
pub async fn execute_local_batch(
    service: &MailboxServiceV1,
    bytes: &[u8],
    context: [u8; 32],
    authority: impl Fn() -> Result<()>,
) -> Result<Zeroizing<Vec<u8>>> {
    execute_local_batch_timed(service, bytes, context, authority, None).await
}

pub(crate) async fn execute_local_batch_timed(
    service: &MailboxServiceV1,
    bytes: &[u8],
    context: [u8; 32],
    authority: impl Fn() -> Result<()>,
    timing: Option<&crate::service_concurrency::ServiceExecutionTimingV1>,
) -> Result<Zeroizing<Vec<u8>>> {
    authority()?;
    let batch = RequestBatch::decode(bytes, context)?;
    let mut replies = Vec::with_capacity(batch.len());
    for row in 0..batch.len() {
        authority()?;
        let reply = service
            .request_local_timed(batch.request(row)?, timing)
            .await?;
        authority()?;
        replies.push(reply);
    }
    encode_replies(context, &replies)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod locator_tests;

#[cfg(test)]
mod bulk_tests;

#[cfg(test)]
mod endpoint_tests;
