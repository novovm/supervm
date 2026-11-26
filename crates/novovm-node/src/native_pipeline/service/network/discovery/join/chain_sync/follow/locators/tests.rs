use super::*;
use crate::native_pipeline::service::network::publish;
use novovm_network::discovery_control::NATIVE_LOCATOR_V2_RELAY_BYTES_PER_MINUTE;
use novovm_relay::transport_identity::ephemeral_transport_identity;

// Called by the installed release preflight regression with real AOEM signatures.
pub(in super::super::super::super) fn check_locator_role_transition(
    root: &Path,
    original: &Profile,
    fixed: &TransportIdentity,
    public: &TransportIdentity,
    local: &TransportIdentity,
    original_fixed: &PeerSignedRelayRecordV1,
) -> Result<()> {
    let mut profile: Profile = serde_json::from_slice(&serde_json::to_vec(original)?)?;
    profile.relays = vec!["https://relay.example".into()];
    let path = root.join("cached-reader-purposes.json");
    let now = wall_ms()?;
    let mut route = IrohServiceEndpointV1::from_record(original_fixed)?;
    route.address.direct.clear();
    route.address.relays = vec!["https://relay.example/".into()];
    let mut endpoint = route.endpoint()?;
    endpoint.max_bytes_per_minute = NATIVE_LOCATOR_V2_RELAY_BYTES_PER_MINUTE;
    let pins = profile.pins();
    let make_public = |provider: &TransportIdentity, sequence, ttl| -> Result<_> {
        Ok(sign_relay_record_v1(
            provider,
            public_read_locator_id_v1(pins),
            vec![endpoint.clone()],
            now - 2000,
            now - 2000 + ttl,
            sequence,
        )?)
    };
    let report = |records: &[PeerSignedRelayRecordV1]| DiscoveryReport {
        sources: Vec::new(),
        locators_by_source: BTreeMap::new(),
        referral_status: BTreeMap::new(),
        conflicting_public_keys: BTreeMap::new(),
        records: records
            .iter()
            .cloned()
            .map(|r| (r.relay_public_key.clone(), r))
            .collect(),
    };
    let mut public_record = make_public(public, 3, 100_000)?;
    let fixed_withdrawn = sign_relay_record_v1(
        public,
        native_locator_id_v1(pins),
        original_fixed.endpoints.clone(),
        now - 2000,
        now + 60_000,
        10,
    )?;
    profile
        .lookup_transport_keys
        .push(public.public_key().to_vec());
    publish(&path, &[original_fixed, &fixed_withdrawn])?;
    let mut cache = Locators::open(path.clone(), &profile, local)?;
    assert_eq!(cache.candidates(&profile, now).count(), 2);
    profile.lookup_transport_keys.clear();
    cache = Locators::open(path.clone(), &profile, local)?;
    assert_eq!(cache.candidates(&profile, now).count(), 0);
    assert!(ReadRoute::new(&fixed_withdrawn, &profile).is_err());
    let observed = cache.observe(
        report(std::slice::from_ref(&public_record)),
        &profile,
        local.public_key(),
    )?;
    assert_eq!(
        observed.len(),
        1,
        "ordinary and public have separate sequence floors"
    );
    assert_eq!(cache.candidates(&profile, now).count(), 1);
    assert!(ReadRoute::new(&public_record, &profile).is_ok());
    profile.relays = vec!["https://another.example/".into()];
    assert!(ReadRoute::new(&public_record, &profile).is_err());
    profile.relays = route.address.relays.clone();
    cache = Locators::open(path.clone(), &profile, local)?;
    assert_eq!(
        cache.candidates(&profile, now).count(),
        0,
        "no public dial from cache alone"
    );
    for rejected in [
        make_public(public, 2, 100_000)?,
        make_public(public, 3, 95_000)?,
        make_public(public, 4, 500)?,
    ] {
        assert!(cache
            .observe(report(&[rejected]), &profile, local.public_key())?
            .is_empty());
        assert_eq!(cache.candidates(&profile, now).count(), 0);
    }
    // A same-sequence conflict needs a later signed sequence before this
    // process can dial the publisher again.
    cache.observe(
        report(std::slice::from_ref(&public_record)),
        &profile,
        local.public_key(),
    )?;
    assert_eq!(cache.candidates(&profile, now).count(), 0);
    public_record = make_public(public, 4, 100_000)?;
    cache.observe(
        report(std::slice::from_ref(&public_record)),
        &profile,
        local.public_key(),
    )?;
    let (other, _) = ephemeral_transport_identity(&profile.library, &profile.library_sha256)?;
    let second_sample = make_public(&other, 1, 20_000)?;
    cache.observe(
        report(std::slice::from_ref(&second_sample)),
        &profile,
        local.public_key(),
    )?;
    assert_eq!(
        cache.candidates(&profile, now).count(),
        2,
        "two distinct samples survive"
    );
    cache.observe(report(&[]), &profile, local.public_key())?;
    assert_eq!(
        cache
            .candidates(&profile, second_sample.expires_at_ms)
            .collect::<Vec<_>>(),
        vec![&public_record]
    );
    assert_eq!(
        cache
            .candidates(&profile, public_record.expires_at_ms)
            .count(),
        0
    );
    let mut conflict = report(&[]);
    conflict
        .conflicting_public_keys
        .insert(other.public_key().to_vec(), 2);
    cache.observe(conflict, &profile, local.public_key())?;
    cache.observe(report(&[second_sample]), &profile, local.public_key())?;
    assert_eq!(
        cache.candidates(&profile, now).count(),
        1,
        "two-source conflict cannot revive the old entry"
    );
    profile
        .lookup_transport_keys
        .push(public.public_key().to_vec());
    cache = Locators::open(path.clone(), &profile, local)?;
    assert_eq!(
        cache.candidates(&profile, now).next(),
        Some(&fixed_withdrawn)
    );
    assert!(ReadRoute::new(&public_record, &profile).is_err());
    profile.lookup_transport_keys = vec![fixed.public_key().to_vec()];
    let mut candidates = vec![public_record.clone(), make_public(&other, 3, 100_000)?];
    for _ in 0..MAX_PUBLIC_READERS {
        let (provider, _) =
            ephemeral_transport_identity(&profile.library, &profile.library_sha256)?;
        candidates.push(make_public(&provider, 1, 100_000)?);
    }
    cache.observe(report(&candidates), &profile, local.public_key())?;
    assert_eq!(cache.public.len(), MAX_PUBLIC_READERS);
    assert_eq!(cache.fixed(&profile).next(), Some(original_fixed));
    assert_eq!(
        cache.candidates(&profile, now).count(),
        MAX_PUBLIC_READERS + 1
    );
    let ordered = order_installed_candidates(cache.candidates(&profile, now).cloned(), &profile);
    assert_eq!(ordered.first(), Some(original_fixed));
    let restored = Locators::open(path.clone(), &profile, local)?;
    assert_eq!(restored.candidates(&profile, now).count(), 1);
    let mut invalid = public_record;
    invalid.signature[0] ^= 1;
    publish(&path, &[invalid])?;
    assert!(Locators::open(path.clone(), &profile, local).is_err());
    publish(&path, &[original_fixed])?;
    profile.chain_id += 1;
    assert!(Locators::open(path, &profile, local).is_err());
    Ok(())
}
