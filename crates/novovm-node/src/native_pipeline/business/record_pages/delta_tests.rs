use super::*;
use crate::native_pipeline::state::tree::{
    empty_root, read_state_value, stage_state_update, NodeHash, StateNodeReader,
};
use std::collections::BTreeMap;

#[derive(Default)]
struct Memory(BTreeMap<NodeHash, Vec<u8>>);
impl StateNodeReader for Memory {
    fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>> {
        Ok(self.0.get(hash).cloned())
    }
}

#[test]
fn parent_record_delta_keeps_full_unknown_updates_and_real_tail_deletions() -> Result<()> {
    let prefix = b"native-record-delta";
    for (before, after) in [
        (vec![7; 100], vec![8; 100]),
        (vec![7; 100], vec![8; 512]),
        (vec![7; 512], vec![8; 100]),
        (vec![7; 100], vec![7; 100]),
    ] {
        let full = record_changes(prefix, &after, 512)?;
        let unknown = record_changes_from_parent(prefix, &after, 512, None)?;
        let delta = record_changes_from_parent(prefix, &after, 512, Some(&before))?;
        let mut memory = Memory::default();
        let initial = stage_state_update(
            &memory,
            empty_root(),
            &record_changes(prefix, &before, 512)?,
        )?;
        let root = initial.root();
        memory.0.extend(initial.nodes().clone());
        let expected = stage_state_update(&memory, root, &full)?;
        let no_parent = stage_state_update(&memory, root, &unknown)?;
        let actual = stage_state_update(&memory, root, &delta)?;
        assert_eq!(unknown.len(), full.len());
        assert_eq!(actual.root(), expected.root());
        assert_eq!(no_parent.root(), expected.root());
        assert_eq!(actual.nodes(), expected.nodes());
        if after.len() < before.len() {
            assert!(delta
                .iter()
                .any(|change| matches!(change, StateChange::Delete { .. })));
        } else {
            assert!(delta
                .iter()
                .all(|change| matches!(change, StateChange::Put { .. })));
        }
        if before == after {
            assert!(delta.is_empty());
        }
        memory.0.extend(actual.nodes().clone());
        assert_eq!(
            read_with(
                |key| read_state_value(&memory, actual.root(), key),
                prefix,
                512
            )?,
            after
        );
    }
    assert!(changed_record_pages(
        record_changes(prefix, &[9; 100], 256)?,
        record_changes(prefix, &[7; 512], 512)?,
    )
    .is_err());
    assert!(changed_record_pages(
        record_changes(prefix, &[9; 100], 512)?,
        record_changes(b"other-native-record", &[7; 100], 512)?,
    )
    .is_err());
    Ok(())
}
