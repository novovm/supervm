use super::*;
use crate::native_pipeline::service::network::publish;
use novovm_network::discovery_control::{decode_node_locator_v1, encode_node_locator_v1};

/// Keep signed sequence floors across client restarts. An expired record can
/// keep a floor but does not supply a dialable address.
pub(super) struct Locators {
    path: PathBuf,
    pub(super) records: BTreeMap<Vec<u8>, PeerSignedRelayRecordV1>,
}

impl Locators {
    pub(super) fn open(
        path: PathBuf,
        profile: &Profile,
        identity: &TransportIdentity,
    ) -> Result<Self> {
        let entries: Vec<PeerSignedRelayRecordV1> = if path.try_exists()? {
            read_document(&path)?
        } else {
            Vec::new()
        };
        ensure!(
            entries.len() <= profile.lookup_transport_keys.len(),
            "installed locator checkpoint inventory"
        );
        let now = wall_ms()?;
        let mut records = BTreeMap::new();
        for record in entries {
            ensure!(
                profile
                    .lookup_transport_keys
                    .contains(&record.relay_public_key)
                    && record.issued_at_ms <= now,
                "installed locator checkpoint key or clock differs"
            );
            let bytes = encode_node_locator_v1(&record, profile.pins())?;
            decode_node_locator_v1(
                &bytes,
                &record.relay_public_key,
                profile.pins(),
                record.issued_at_ms,
                identity,
            )
            .context("authenticating installed locator checkpoint")?;
            ensure!(
                records
                    .insert(record.relay_public_key.clone(), record)
                    .is_none(),
                "duplicate installed locator pin"
            );
        }
        Ok(Self { path, records })
    }

    pub(super) fn observe(&mut self, found: DiscoveryReport) -> Result<()> {
        let mut advanced = self.records.clone();
        for (key, candidate) in found.records {
            if let Some(current) = advanced.get(&key) {
                if candidate.sequence < current.sequence
                    || candidate.issued_at_ms < current.issued_at_ms
                {
                    continue;
                }
                ensure!(
                    candidate.sequence != current.sequence || candidate == *current,
                    "conflicting locator at a stored signed sequence"
                );
            }
            advanced.insert(key, candidate);
        }
        if advanced != self.records {
            publish(&self.path, &advanced.values().collect::<Vec<_>>())?;
            self.records = advanced;
        }
        Ok(())
    }
}
