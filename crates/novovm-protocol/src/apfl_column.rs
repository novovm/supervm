//! Shared APFL column representation extracted without behavior changes from
//! novovm-node/native_pipeline/ingress/apfl.rs (3a9dfcce). No business policy.
//! Public storage is NOT validation or authorization; wire decoders must check
//! bounds, unique dictionary values and canonical first-use indexes.
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Column<T> {
    pub values: Vec<T>,
    // Empty for shared or dense columns; otherwise exactly one u32 per row.
    pub indexes: Vec<u32>,
}

impl<T> Column<T> {
    pub fn value(&self, index: usize) -> &T {
        let slot = if self.values.len() == 1 {
            0
        } else if self.indexes.is_empty() {
            index
        } else {
            self.indexes[index] as usize
        };
        &self.values[slot]
    }
    pub fn map<U>(self, convert: impl Fn(T) -> U) -> Column<U> {
        Column {
            values: self.values.into_iter().map(convert).collect(),
            indexes: self.indexes,
        }
    }
}

impl<T: Clone + Ord> Column<T> {
    pub fn collect(input: impl Iterator<Item = T>) -> Self {
        let mut known = BTreeMap::new();
        let mut values = Vec::new();
        let mut indexes = Vec::new();
        for value in input {
            let index = *known.entry(value.clone()).or_insert_with(|| {
                let index = values.len() as u32;
                values.push(value);
                index
            });
            indexes.push(index);
        }
        if values.len() == 1 || values.len() == indexes.len() {
            indexes.clear();
        }
        Self { values, indexes }
    }
}
