use serde::{de::DeserializeOwned, Serialize};

#[inline]
pub fn serialize<T: Serialize>(value: &T) -> Result<Vec<u8>, postcard::Error> {
    postcard::to_allocvec(value)
}

#[inline]
pub fn deserialize<T>(bytes: &[u8]) -> Result<T, postcard::Error>
where
    T: DeserializeOwned,
{
    postcard::from_bytes(bytes)
}
