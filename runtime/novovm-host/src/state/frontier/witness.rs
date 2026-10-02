//! Canonical, bounded transport of the original parent frontier. This carries
//! no authority to select a parent, replace a business plan, or publish effects.
//! Import replays the existing authenticated path capture against independently
//! supplied root/access pins, including deletion's required sibling edges.

use super::*;
use anyhow::ensure;

const MAGIC: &[u8; 8] = b"NVFRNT01";
const HEADER_BYTES: usize = 8 + 32 + 4 + 4;
// V1 encodes the existing Patricia codec, whose raw keys/values are <=256 B;
// its largest node is a leaf's 35 B framing plus its 256 B value.
const MAX_KEY_BYTES: usize = 256;
const MAX_NODE_BYTES: usize = 35 + 256;
const ACCESS_OVERHEAD: usize = 2 + 1;
const NODE_OVERHEAD: usize = 32 + 2;

fn wire_limit(budget: CaptureBudget) -> Result<usize> {
    HEADER_BYTES
        .checked_add(
            budget
                .keys
                .checked_mul(ACCESS_OVERHEAD + MAX_KEY_BYTES)
                .context("frontier witness access framing overflow")?,
        )
        .and_then(|bytes| bytes.checked_add(budget.nodes.checked_mul(NODE_OVERHEAD)?))
        .and_then(|bytes| bytes.checked_add(budget.bytes))
        .context("frontier witness encoded budget overflow")
}

impl OwnedStateInput {
    /// Encode only the captured PARENT nodes, never candidate update nodes.
    /// `budget.bytes` continues to count unique node bytes. Key/hash/length
    /// framing has a separately checked bound derived from keys/nodes limits.
    /// Whole-frontier encoding belongs on an input/proof owner, not consensus poll.
    pub fn encode_witness(&self, budget: CaptureBudget) -> Result<Vec<u8>> {
        let maximum = wire_limit(budget)?;
        ensure!(
            !self.access.is_empty() && self.access.len() <= budget.keys,
            "frontier witness declaration budget exceeded or empty"
        );
        ensure!(
            self.nodes.len() <= budget.nodes,
            "frontier witness node budget exceeded"
        );
        let keys = u32::try_from(self.access.len())?;
        let nodes = u32::try_from(self.nodes.len())?;
        let mut total = HEADER_BYTES;
        for key in self.access.keys() {
            super::super::tree::state_key_hash(key)?;
            total = total
                .checked_add(ACCESS_OVERHEAD + key.len())
                .context("frontier witness encoded length overflow")?;
        }
        let mut node_bytes = 0usize;
        for (hash, bytes) in &self.nodes {
            validate_state_node_bytes(hash, bytes)?;
            node_bytes = node_bytes
                .checked_add(bytes.len())
                .context("frontier witness node byte count overflow")?;
            ensure!(
                node_bytes <= budget.bytes,
                "frontier witness node byte budget exceeded"
            );
            total = total
                .checked_add(NODE_OVERHEAD + bytes.len())
                .context("frontier witness encoded length overflow")?;
        }
        ensure!(
            node_bytes == self.bytes,
            "frontier witness cached byte count mismatch"
        );
        ensure!(total <= maximum, "frontier witness encoded budget exceeded");
        let mut wire = Vec::with_capacity(total);
        wire.extend_from_slice(MAGIC);
        wire.extend_from_slice(&self.root);
        wire.extend_from_slice(&keys.to_be_bytes());
        for (key, &(put, delete)) in &self.access {
            wire.extend_from_slice(&u16::try_from(key.len())?.to_be_bytes());
            wire.extend_from_slice(key);
            wire.push(u8::from(put) | (u8::from(delete) << 1));
        }
        wire.extend_from_slice(&nodes.to_be_bytes());
        for (hash, bytes) in &self.nodes {
            wire.extend_from_slice(hash);
            wire.extend_from_slice(&u16::try_from(bytes.len())?.to_be_bytes());
            wire.extend_from_slice(bytes);
        }
        ensure!(
            wire.len() == total,
            "frontier witness encoded length mismatch"
        );
        Ok(wire)
    }

    /// Decode against an independently pinned parent and exact compiler-derived
    /// access declarations. These arguments MUST NOT be copied from this wire.
    /// Content authentication is not parent finality/full-tree validation.
    /// Missing bytes are errors; only a valid path or the trusted empty root can
    /// establish absence. No low-level reader escapes the resulting permission gate.
    pub fn from_witness(
        expected_parent: NodeHash,
        declarations: &[DeclaredAccess],
        wire: &[u8],
        budget: CaptureBudget,
    ) -> Result<Self> {
        ensure!(
            wire.len() >= HEADER_BYTES && wire.len() <= wire_limit(budget)?,
            "frontier witness encoded byte budget exceeded or truncated"
        );
        ensure!(
            !declarations.is_empty() && declarations.len() <= budget.keys,
            "frontier witness declaration budget exceeded or empty"
        );
        // Reconstruct the exact independent access map. Never merge duplicates
        // or let wire-provided flags widen these permissions.
        let mut expected = BTreeMap::new();
        for declaration in declarations {
            super::super::tree::state_key_hash(&declaration.key)?;
            ensure!(
                expected
                    .insert(
                        declaration.key.as_slice(),
                        (declaration.may_put, declaration.may_delete)
                    )
                    .is_none(),
                "frontier witness expected declarations must have unique keys"
            );
        }
        let mut reader = Reader { remaining: wire };
        ensure!(
            reader.take(MAGIC.len())? == MAGIC,
            "frontier witness version mismatch"
        );
        ensure!(
            reader.hash()? == expected_parent,
            "frontier witness parent root mismatch"
        );
        let key_count = reader.count()?;
        ensure!(
            key_count == expected.len() && key_count <= budget.keys,
            "frontier witness access set size mismatch"
        );
        ensure!(
            key_count <= reader.remaining.len() / (ACCESS_OVERHEAD + 1),
            "frontier witness truncated access list"
        );
        // Comparing with BTreeMap's unique sorted sequence also rejects wire
        // duplicates and noncanonical ordering before any node allocations.
        for (key, &(put, delete)) in &expected {
            let length = reader.length()?;
            ensure!(
                (1..=MAX_KEY_BYTES).contains(&length),
                "frontier witness key length exceeds bound"
            );
            ensure!(
                reader.take(length)? == *key,
                "frontier witness access key/order mismatch"
            );
            let flags = reader.take(1)?[0];
            ensure!(
                flags <= 3 && flags == (u8::from(put) | (u8::from(delete) << 1)),
                "frontier witness access permission mismatch"
            );
        }
        let node_count = reader.count()?;
        ensure!(
            node_count <= budget.nodes,
            "frontier witness node budget exceeded"
        );
        ensure!(
            node_count <= reader.remaining.len() / (NODE_OVERHEAD + 35),
            "frontier witness truncated node list"
        );
        let mut nodes = BTreeMap::new();
        let mut previous = None;
        let mut node_bytes = 0usize;
        for _ in 0..node_count {
            let hash = reader.hash()?;
            ensure!(
                previous.is_none_or(|old| old < hash),
                "frontier witness node hashes are not unique and ordered"
            );
            let length = reader.length()?;
            ensure!(
                (35..=MAX_NODE_BYTES).contains(&length),
                "frontier witness node length exceeds bound"
            );
            node_bytes = node_bytes
                .checked_add(length)
                .context("frontier witness node byte count overflow")?;
            ensure!(
                node_bytes <= budget.bytes,
                "frontier witness node byte budget exceeded"
            );
            let bytes = reader.take(length)?;
            nodes.insert(hash, bytes.to_vec());
            previous = Some(hash);
        }
        ensure!(
            reader.remaining.is_empty(),
            "frontier witness trailing bytes"
        );
        // Reuse the same root-authenticated edge walk as live capture. Merely
        // checking each node hash would accept detached or misdirected nodes.
        let input = Self::capture(
            &WitnessReader(&nodes),
            expected_parent,
            declarations,
            budget,
        )?;
        ensure!(
            input.nodes == nodes && input.bytes == node_bytes,
            "frontier witness contains unused or extraneous nodes"
        );
        Ok(input)
    }
}

struct WitnessReader<'a>(&'a BTreeMap<NodeHash, Vec<u8>>);
impl StateNodeReader for WitnessReader<'_> {
    fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>> {
        Ok(self.0.get(hash).cloned())
    }
}

struct Reader<'a> {
    remaining: &'a [u8],
}
impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> Result<&'a [u8]> {
        ensure!(count <= self.remaining.len(), "truncated frontier witness");
        let (head, tail) = self.remaining.split_at(count);
        self.remaining = tail;
        Ok(head)
    }
    fn hash(&mut self) -> Result<NodeHash> {
        Ok(self.take(32)?.try_into()?)
    }
    fn count(&mut self) -> Result<usize> {
        usize::try_from(u32::from_be_bytes(self.take(4)?.try_into()?))
            .context("frontier witness count exceeds platform bound")
    }
    fn length(&mut self) -> Result<usize> {
        Ok(usize::from(u16::from_be_bytes(self.take(2)?.try_into()?)))
    }
}

#[cfg(test)]
mod tests;
