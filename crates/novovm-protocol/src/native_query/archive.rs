use super::{Hash, ARCHIVE_CHUNK_BYTES, MAX_ARCHIVE_PROOF_BYTES};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveSelector {
    pub height: u64,
    pub offset: u32,
    /// All slices after offset zero must pin the proof digest learned at zero.
    pub digest: Hash,
}

impl ArchiveSelector {
    pub(super) fn is_valid(&self) -> bool {
        let offset = self.offset as usize;
        self.height != 0
            && offset < MAX_ARCHIVE_PROOF_BYTES
            && offset.is_multiple_of(ARCHIVE_CHUNK_BYTES)
            && ((offset == 0) == (self.digest == [0; 32]))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveChunk {
    pub digest: Hash,
    pub total_bytes: u32,
    #[serde(with = "bytes")]
    pub data: Vec<u8>,
}

impl ArchiveChunk {
    pub(super) fn matches(&self, request: &ArchiveSelector) -> bool {
        let total = self.total_bytes as usize;
        let offset = request.offset as usize;
        self.digest != [0; 32]
            && (offset == 0 || request.digest == self.digest)
            && total <= MAX_ARCHIVE_PROOF_BYTES
            && total > offset
            && self.data.len() == (total - offset).min(ARCHIVE_CHUNK_BYTES)
    }
}

pub(super) mod bytes {
    use super::*;

    pub fn serialize<S: serde::Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(bytes)
    }

    pub fn deserialize<'de, D: serde::Deserializer<'de>>(decoder: D) -> Result<Vec<u8>, D::Error> {
        deserialize_bounded::<D, 1, ARCHIVE_CHUNK_BYTES>(decoder)
    }

    pub(crate) fn deserialize_bounded<
        'de,
        D: serde::Deserializer<'de>,
        const MIN: usize,
        const MAX: usize,
    >(
        decoder: D,
    ) -> Result<Vec<u8>, D::Error> {
        struct BytesVisitor<const MIN: usize, const MAX: usize>;

        impl<'de, const MIN: usize, const MAX: usize> serde::de::Visitor<'de> for BytesVisitor<MIN, MAX> {
            type Value = Vec<u8>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "between {MIN} and {MAX} native query bytes")
            }

            fn visit_bytes<E: serde::de::Error>(self, data: &[u8]) -> Result<Self::Value, E> {
                if !(MIN..=MAX).contains(&data.len()) {
                    return Err(E::custom("native query bytes length"));
                }
                Ok(data.to_vec())
            }
        }

        decoder.deserialize_bytes(BytesVisitor::<MIN, MAX>)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_query::{
        decode, decode_response, encode, Answer, Frame, Query, Request, Response,
    };

    #[test]
    fn archive_chunks_bind_height_offset_and_digest_without_expanding_the_query_budget() {
        let digest = [3; 32];
        let selector = ArchiveSelector {
            height: 141,
            offset: ARCHIVE_CHUNK_BYTES as u32,
            digest,
        };
        let request = Request {
            session: [2; 32],
            sequence: 1,
            query: Query::ReceiptProof(selector),
        };
        let frame = Frame::Request(request.clone());
        let bytes = encode(frame.clone()).unwrap();
        assert_eq!(decode(&bytes).unwrap(), frame);

        let mut bad_header = bytes;
        bad_header[..8].copy_from_slice(b"NVQUERY1");
        assert!(decode(&bad_header).is_err());

        let mut chunk = ArchiveChunk {
            digest,
            total_bytes: (ARCHIVE_CHUNK_BYTES * 2) as u32,
            data: vec![77; ARCHIVE_CHUNK_BYTES],
        };
        let encode_reply = |chunk: ArchiveChunk| {
            encode(Frame::Response(Response {
                request: request.clone(),
                answer: Answer::ReceiptProof(chunk),
            }))
        };
        let packet = encode_reply(chunk.clone()).unwrap();
        assert!(packet.len() <= super::super::MAX_BYTES);
        assert_eq!(
            decode_response(&packet, &request).unwrap().unwrap().answer,
            Answer::ReceiptProof(chunk.clone())
        );
        let other = Request {
            query: Query::ReceiptProof(ArchiveSelector {
                height: 142,
                ..selector
            }),
            ..request.clone()
        };
        assert!(decode_response(&packet, &other).unwrap().is_none());
        chunk.data.push(0);
        assert!(encode_reply(chunk.clone()).is_err());
        chunk.data.pop();
        chunk.digest[0] ^= 1;
        assert!(encode_reply(chunk).is_err());

        let unpinned = Request {
            query: Query::ReceiptProof(ArchiveSelector {
                digest: [0; 32],
                ..selector
            }),
            ..request
        };
        assert!(encode(Frame::Request(unpinned)).is_err());
    }
}
