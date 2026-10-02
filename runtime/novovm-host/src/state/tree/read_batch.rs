//! Shared traversal of an immutable, already trusted parent. This owns no
//! database or publication authority and does not stage any state updates.

use super::*;

#[cfg(test)]
mod tests;

pub const MAX_BATCH_READ_KEYS: usize = 4096;

#[cfg(test)]
thread_local! {
    static READ_BATCH_STATS: std::cell::Cell<(usize, usize)> = const {
        std::cell::Cell::new((0, 0))
    };
}

#[cfg(test)]
pub(crate) fn read_batch_stats_for_test() -> (usize, usize) {
    READ_BATCH_STATS.with(std::cell::Cell::get)
}

struct Query {
    key: NodeHash,
    position: usize,
}

/// Read at most 4096 keys from an already trusted, structurally validated root.
/// Results retain the supplied order, including duplicate keys. Only queried
/// paths are loaded: missing, malformed, or misdirected accessed nodes fail the
/// whole call, while authenticated absence returns `None` for that query.
/// An empty query set does not validate the root. Success grants no authority.
pub fn read_state_values(
    reader: &dyn StateNodeReader,
    trusted_root: NodeHash,
    keys: &[Vec<u8>],
) -> Result<Vec<Option<Vec<u8>>>> {
    if keys.len() > MAX_BATCH_READ_KEYS {
        bail!("too many state read queries");
    }
    let mut queries = keys
        .iter()
        .enumerate()
        .map(|(position, key)| {
            Ok(Query {
                key: digest_key(key)?,
                position,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    queries.sort_unstable_by_key(|query| query.key);
    let mut results = vec![None; keys.len()];
    Planner::new(reader).read_queries(trusted_root, &queries, 0, &mut results)?;
    #[cfg(test)]
    READ_BATCH_STATS.with(|stats| {
        let (calls, queries) = stats.get();
        stats.set((calls + 1, queries + keys.len()));
    });
    Ok(results)
}

impl Planner<'_> {
    /// Sorted queries share their incoming authenticated path. Branch edges
    /// increase minimum_bit, with at most 256 branches before a leaf.
    fn read_queries(
        &mut self,
        current: NodeHash,
        queries: &[Query],
        minimum_bit: u16,
        results: &mut [Option<Vec<u8>>],
    ) -> Result<()> {
        let Some(first) = queries.first() else {
            return Ok(());
        };
        if current == empty_root() {
            return Ok(());
        }
        let last = &queries[queries.len() - 1];
        let node = self.load(&current)?;
        // Validate the incoming edge BEFORE using a compressed prefix or leaf
        // mismatch as an absence proof. Cache hits never skip this placement
        // check. Sorted interval endpoints cover every intervening query.
        node.validate_path(&first.key, minimum_bit)?;
        node.validate_path(&last.key, minimum_bit)?;
        match node {
            Node::Leaf { key, value } => {
                let start = queries.partition_point(|query| query.key < key);
                for query in queries[start..].iter().take_while(|query| query.key == key) {
                    results[query.position] = Some(value.clone());
                }
                Ok(())
            }
            Node::Branch {
                bit,
                prefix,
                left,
                right,
            } => {
                // The matching prefix is a contiguous interval in digest order.
                // Both endpoints may be absent while queries in between match;
                // do not discard that middle interval on an endpoint mismatch.
                let start = queries.partition_point(|query| query.key < prefix);
                let suffix = &queries[start..];
                let length =
                    suffix.partition_point(|query| common_prefix(&query.key, &prefix) >= bit);
                let matching = &suffix[..length];
                let split = matching.partition_point(|query| !bit_at(&query.key, bit));
                self.read_queries(left, &matching[..split], bit + 1, results)?;
                self.read_queries(right, &matching[split..], bit + 1, results)
            }
        }
    }
}
