use anyhow::Result;
use novovm_adapter_api::{
    AccountAction, AccountAuditEvent, AccountPolicy, AccountRole, NonceScope, PersonaAddress,
    PersonaType, ProtocolKind, RouteDecision, RouteRequest, Type4PolicyMode, KycPolicyMode, UnifiedAccountError,
    UnifiedAccountRouter,
};

fn evm_persona(chain_id: u64, seed: u8) -> PersonaAddress {
    PersonaAddress {
        persona_type: PersonaType::Evm,
        chain_id,
        external_address: vec![seed; 20],
    }
}

#[test]
fn binding_conflict_is_rejected_globally() -> Result<()> {
    let mut router = UnifiedAccountRouter::new();
    router.create_uca("uca-a", vec![1; 32], 1)?;
    router.create_uca("uca-b", vec![2; 32], 1)?;

    let persona = evm_persona(1, 0xAA);
    router.add_binding("uca-a", AccountRole::Owner, persona.clone(), 2)?;

    let err = router
        .add_binding("uca-b", AccountRole::Owner, persona.clone(), 3)
        .expect_err("conflict should fail");
    assert!(matches!(err, UnifiedAccountError::BindingConflict { .. }));

    let has_conflict_event = router.events().iter().any(|event| {
        matches!(
            event,
            AccountAuditEvent::BindingConflictRejected {
                request_uca_id,
                existing_uca_id,
                ..
            } if request_uca_id == "uca-b" && existing_uca_id == "uca-a"
        )
    });
    assert!(has_conflict_event);

    Ok(())
}

#[test]
fn revoke_enters_cooldown_and_rebind_after_expiry() -> Result<()> {
    let mut router = UnifiedAccountRouter::new();
    router.create_uca("uca-a", vec![1; 32], 1)?;
    let persona = evm_persona(1, 0xBB);

    router.add_binding("uca-a", AccountRole::Owner, persona.clone(), 2)?;
    router.revoke_binding("uca-a", AccountRole::Owner, persona.clone(), 60, 10)?;

    let early = router
        .add_binding("uca-a", AccountRole::Owner, persona.clone(), 20)
        .expect_err("cooldown must reject");
    assert!(matches!(early, UnifiedAccountError::CooldownActive { .. }));

    router.add_binding("uca-a", AccountRole::Owner, persona.clone(), 80)?;
    assert_eq!(router.resolve_binding_owner(&persona), Some("uca-a"));

    Ok(())
}

#[test]
fn route_checks_domain_and_nonce_scope_persona() -> Result<()> {
    let mut router = UnifiedAccountRouter::new();
    router.create_uca("uca-a", vec![1; 32], 1)?;
    let persona = evm_persona(1, 0xCC);
    router.add_binding("uca-a", AccountRole::Owner, persona.clone(), 2)?;

    let ok = router.route(RouteRequest {
        uca_id: "uca-a".to_string(),
        persona: persona.clone(),
        role: AccountRole::Owner,
        protocol: ProtocolKind::Eth,
        signature_domain: "evm:1".to_string(),
        nonce: 0,
        kyc_attestation_provided: false,
        kyc_verified: false,
        wants_cross_chain_atomic: false,
        tx_type4: false,
        session_expires_at: None,
        now: 3,
    })?;
    assert_eq!(ok, RouteDecision::Adapter { chain_id: 1 });

    let replay = router
        .route(RouteRequest {
            uca_id: "uca-a".to_string(),
            persona: persona.clone(),
            role: AccountRole::Owner,
            protocol: ProtocolKind::Eth,
            signature_domain: "evm:1".to_string(),
            nonce: 0,
            kyc_attestation_provided: false,
            kyc_verified: false,
            wants_cross_chain_atomic: false,
            tx_type4: false,
            session_expires_at: None,
            now: 4,
        })
        .expect_err("replay nonce must fail");
    assert!(matches!(
        replay,
        UnifiedAccountError::NonceReplay {
            expected: 1,
            got: 0
        }
    ));

    let bad_domain = router
        .route(RouteRequest {
            uca_id: "uca-a".to_string(),
            persona,
            role: AccountRole::Owner,
            protocol: ProtocolKind::Eth,
            signature_domain: "web30:mainnet".to_string(),
            nonce: 1,
            kyc_attestation_provided: false,
            kyc_verified: false,
            wants_cross_chain_atomic: false,
            tx_type4: false,
            session_expires_at: None,
            now: 5,
        })
        .expect_err("domain mismatch should fail");
    assert!(matches!(
        bad_domain,
        UnifiedAccountError::DomainMismatch { .. }
    ));

    Ok(())
}

#[test]
fn permission_boundary_and_type4_policy_are_enforced() -> Result<()> {
    let mut router = UnifiedAccountRouter::new();
    router.create_uca("uca-a", vec![1; 32], 1)?;
    let persona = evm_persona(1, 0xDD);
    router.add_binding("uca-a", AccountRole::Owner, persona.clone(), 2)?;

    let policy_err = router
        .update_policy(
            "uca-a",
            AccountRole::Delegate,
            AccountPolicy {
                nonce_scope: NonceScope::Global,
                type4_policy_mode: Type4PolicyMode::Rejected,
                allow_type4_with_delegate_or_session: false,
                kyc_policy_mode: KycPolicyMode::Disabled,
            },
            3,
        )
        .expect_err("delegate cannot update policy");
    assert!(matches!(
        policy_err,
        UnifiedAccountError::PermissionDenied {
            role: AccountRole::Delegate,
            action: AccountAction::UpdatePolicy
        }
    ));

    let boundary_err = router
        .route(RouteRequest {
            uca_id: "uca-a".to_string(),
            persona: persona.clone(),
            role: AccountRole::Owner,
            protocol: ProtocolKind::Eth,
            signature_domain: "evm:1".to_string(),
            nonce: 0,
            kyc_attestation_provided: false,
            kyc_verified: false,
            wants_cross_chain_atomic: true,
            tx_type4: false,
            session_expires_at: None,
            now: 4,
        })
        .expect_err("eth cross-chain atomic should be rejected");
    assert!(matches!(
        boundary_err,
        UnifiedAccountError::PersonaBoundaryViolation
    ));

    let type4_err = router
        .route(RouteRequest {
            uca_id: "uca-a".to_string(),
            persona,
            role: AccountRole::Delegate,
            protocol: ProtocolKind::Eth,
            signature_domain: "evm:1".to_string(),
            nonce: 0,
            kyc_attestation_provided: false,
            kyc_verified: false,
            wants_cross_chain_atomic: false,
            tx_type4: true,
            session_expires_at: None,
            now: 5,
        })
        .expect_err("type4 + delegate should be blocked by default");
    assert!(matches!(
        type4_err,
        UnifiedAccountError::Type4PolicyViolation
    ));

    Ok(())
}

#[test]
fn next_nonce_for_persona_tracks_route_progress() -> Result<()> {
    let mut router = UnifiedAccountRouter::new();
    router.create_uca("uca-a", vec![1; 32], 1)?;
    let persona = evm_persona(1, 0xEE);
    router.add_binding("uca-a", AccountRole::Owner, persona.clone(), 2)?;

    let nonce0 = router.next_nonce_for_persona("uca-a", &persona)?;
    assert_eq!(nonce0, 0);

    router.route(RouteRequest {
        uca_id: "uca-a".to_string(),
        persona: persona.clone(),
        role: AccountRole::Owner,
        protocol: ProtocolKind::Eth,
        signature_domain: "evm:1".to_string(),
        nonce: 0,
        kyc_attestation_provided: false,
        kyc_verified: false,
        wants_cross_chain_atomic: false,
        tx_type4: false,
        session_expires_at: None,
        now: 3,
    })?;

    let nonce1 = router.next_nonce_for_persona("uca-a", &persona)?;
    assert_eq!(nonce1, 1);
    Ok(())
}
