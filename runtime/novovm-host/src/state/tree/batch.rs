//! Shared-prefix construction for independent final record puts. This changes
//! neither transaction execution order nor the content-addressed tree format.

use super::*;

#[cfg(test)]
mod tests;

pub(super) struct Put<'a> {
    key: NodeHash,
    value: &'a [u8],
}

/// Only a supplied run of distinct digests may use shared construction. Deletes
/// and duplicate digests are returned to the ordered dispatcher, including any
/// intermediate resource use. Values remain borrowed from the bounded batch.
pub(super) fn unique_puts(changes: &[StateChange]) -> Result<Option<Vec<Put<'_>>>> {
    if changes
        .iter()
        .any(|change| matches!(change, StateChange::Delete { .. }))
    {
        return Ok(None);
    }
    let mut puts = Vec::with_capacity(changes.len());
    for change in changes {
        let StateChange::Put { key, value } = change else {
            unreachable!("delete batches use the ordered planner");
        };
        if value.len() > MAX_VALUE_BYTES {
            bail!("state leaf value exceeds 256 bytes");
        }
        puts.push(Put {
            key: digest_key(key)?,
            value,
        });
    }
    puts.sort_unstable_by_key(|put| put.key);
    if puts.windows(2).any(|pair| pair[0].key == pair[1].key) {
        return Ok(None);
    }
    Ok(Some(puts))
}

impl Planner<'_> {
    /// `puts` is sorted by its distinct digests and all share the authenticated
    /// incoming path. Each recursive edge increases minimum_bit, to at most 256.
    pub(super) fn change_many(
        &mut self,
        current: NodeHash,
        puts: &[Put<'_>],
        minimum_bit: u16,
    ) -> Result<NodeHash> {
        let Some(first) = puts.first() else {
            // No update enters this subtree: do not load or claim to validate it.
            return Ok(current);
        };
        let last = &puts[puts.len() - 1];
        if current == empty_root() {
            return self.build_puts(puts);
        }

        // Always authenticate the existing node before introducing any shallower
        // branch. Sorting/new siblings must not hide a missing or corrupt root.
        // Cache hits do not replace the validation of this particular edge.
        let node = self.load(&current)?;
        node.validate_path(&first.key, minimum_bit)?;
        node.validate_path(&last.key, minimum_bit)?;
        let representative = node.representative();
        let common = common_prefix(&representative, &first.key)
            .min(common_prefix(&representative, &last.key));
        let node_bit = match &node {
            Node::Leaf { .. } => 256,
            Node::Branch { bit, .. } => *bit,
        };

        if common < node_bit {
            // All old leaves belong to one side of this new, shallower branch.
            // Only that side reuses the old root; the other has no parent reads.
            let split = puts.partition_point(|put| !bit_at(&put.key, common));
            let (old_left, old_right) = if bit_at(&representative, common) {
                (empty_root(), current)
            } else {
                (current, empty_root())
            };
            let left = self.change_many(old_left, &puts[..split], common + 1)?;
            let right = self.change_many(old_right, &puts[split..], common + 1)?;
            return self.stage(Node::Branch {
                bit: common,
                prefix: prefix_of(&representative, common),
                left,
                right,
            });
        }

        match node {
            Node::Leaf { key, value } => {
                if puts.len() != 1 || key != first.key {
                    bail!("state batch leaf comparison invariant");
                }
                if value == first.value {
                    Ok(current)
                } else {
                    self.stage(Node::Leaf {
                        key,
                        value: first.value.to_vec(),
                    })
                }
            }
            Node::Branch {
                bit,
                prefix,
                left,
                right,
            } => {
                let split = puts.partition_point(|put| !bit_at(&put.key, bit));
                let next_left = self.change_many(left, &puts[..split], bit + 1)?;
                let next_right = self.change_many(right, &puts[split..], bit + 1)?;
                if next_left == left && next_right == right {
                    return Ok(current);
                }
                self.stage(Node::Branch {
                    bit,
                    prefix,
                    left: next_left,
                    right: next_right,
                })
            }
        }
    }

    /// Build a nonempty sorted slice without intermediate versions. The first
    /// and last digest determine the common prefix for every digest in between.
    fn build_puts(&mut self, puts: &[Put<'_>]) -> Result<NodeHash> {
        let first = &puts[0];
        if puts.len() == 1 {
            return self.stage(Node::Leaf {
                key: first.key,
                value: first.value.to_vec(),
            });
        }
        let bit = common_prefix(&first.key, &puts[puts.len() - 1].key);
        let split = puts.partition_point(|put| !bit_at(&put.key, bit));
        let left = self.build_puts(&puts[..split])?;
        let right = self.build_puts(&puts[split..])?;
        self.stage(Node::Branch {
            bit,
            prefix: prefix_of(&first.key, bit),
            left,
            right,
        })
    }
}
