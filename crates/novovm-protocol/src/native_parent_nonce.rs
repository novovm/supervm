//! Read nonce from the exact full-state wire committed by the native V3 root.
//! The expected root must come from an independently selected/validated parent.
//! This does not validate consensus finality or authenticate a caller-selected root.
use crate::native_nonce::{
    nonce_identity_digest_v1, signer_nonce_identity_v2, NATIVE_AUTH_NONCE_IDENTITY_SCHEME_V2,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const MAX_PARENT_WIRE_BYTES: usize = 1024 * 1024;
const MAX_DEPTH: usize = 64;
const MAX_VALUES: usize = 65536;

pub fn native_state_wire_root_v3(wire: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"novovm-native-aoem-semantic-ledger-state-digest-v3\0");
    h.update(wire);
    h.finalize().into()
}

enum Value<'a> {
    Other,
    Number(&'a str),
    Text(&'a str),
    Object(BTreeMap<&'a str, Value<'a>>),
}
impl<'a> Value<'a> {
    fn field(&self, key: &str) -> Result<&Value<'a>, &'static str> {
        match self {
            Self::Object(map) => map.get(key).ok_or("missing parent field"),
            _ => Err("parent field is not object"),
        }
    }
    fn text(&self) -> Result<&str, &'static str> {
        match self {
            Self::Text(s) => Ok(s),
            _ => Err("parent field is not text"),
        }
    }
}
struct Reader<'a> {
    bytes: &'a [u8],
    values: usize,
}
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], &'static str> {
        let out = self.bytes.get(..n).ok_or("truncated parent wire")?;
        self.bytes = &self.bytes[n..];
        Ok(out)
    }
    fn length(&mut self) -> Result<usize, &'static str> {
        let bytes = self.take(8)?;
        usize::try_from(u64::from_be_bytes(bytes.try_into().unwrap()))
            .map_err(|_| "parent length overflow")
    }
    fn text(&mut self) -> Result<&'a str, &'static str> {
        let len = self.length()?;
        std::str::from_utf8(self.take(len)?).map_err(|_| "invalid parent UTF8")
    }
    fn value(&mut self, depth: usize) -> Result<Value<'a>, &'static str> {
        if depth > MAX_DEPTH || self.values >= MAX_VALUES {
            return Err("parent structure limit");
        }
        self.values += 1;
        match self.take(1)?[0] {
            0..=2 => Ok(Value::Other),
            3 => Ok(Value::Number(self.text()?)),
            4 => Ok(Value::Text(self.text()?)),
            5 => {
                let n = self.length()?;
                if n > MAX_VALUES || n > self.bytes.len() {
                    return Err("parent array limit");
                }
                for _ in 0..n {
                    self.value(depth + 1)?;
                }
                Ok(Value::Other)
            }
            6 => {
                let n = self.length()?;
                if n > MAX_VALUES || n > self.bytes.len() / 9 {
                    return Err("parent object limit");
                }
                let mut map = BTreeMap::new();
                let mut previous: Option<&str> = None;
                for _ in 0..n {
                    let key = self.text()?;
                    if previous.is_some_and(|old| old >= key) {
                        return Err("unsorted or duplicate parent key");
                    }
                    previous = Some(key);
                    let value = self.value(depth + 1)?;
                    map.insert(key, value);
                }
                Ok(Value::Object(map))
            }
            _ => Err("unknown parent wire tag"),
        }
    }
}

pub fn parent_nonce_v3(
    wire: &[u8],
    expected_root: &[u8; 32],
    chain: u64,
    public_key: &[u8; 32],
) -> Result<u64, &'static str> {
    if wire.len() > MAX_PARENT_WIRE_BYTES || chain == 0 {
        return Err("invalid parent bounds or chain");
    }
    if native_state_wire_root_v3(wire) != *expected_root {
        return Err("parent root mismatch");
    }
    let mut reader = Reader {
        bytes: wire,
        values: 0,
    };
    let root = reader.value(0)?;
    if !reader.bytes.is_empty() {
        return Err("trailing parent wire");
    }
    if root.field("schema")?.text()? != "novovm-consensus-native-state-projection/v3" {
        return Err("parent schema mismatch");
    }
    let execution = root
        .field("module_state_shards")?
        .field("native_execution")?;
    if execution
        .field("native_auth_nonce_identity_scheme")?
        .text()?
        != NATIVE_AUTH_NONCE_IDENTITY_SCHEME_V2
    {
        return Err("parent identity scheme mismatch");
    }
    let Value::Object(nonces) = execution.field("native_auth_next_nonces")? else {
        return Err("nonce table is not object");
    };
    let digest = nonce_identity_digest_v1(chain, &signer_nonce_identity_v2(public_key));
    let key: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    match nonces.get(key.as_str()) {
        None => Ok(0),
        Some(Value::Number(text)) => {
            let nonce: u64 = text.parse().map_err(|_| "invalid parent nonce")?;
            if nonce.to_string() != *text {
                return Err("noncanonical parent nonce");
            }
            Ok(nonce)
        }
        _ => Err("parent nonce is not integer"),
    }
}
