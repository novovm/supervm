#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UcaStatus {
    Active,
    Suspended,
    Recovering,
    Revoked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BindingState {
    Bound,
    Revoking,
    Revoked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NonceScope {
    Persona,
    Chain,
    Global,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccountRole {
    Owner,
    Delegate,
    SessionKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccountAction {
    BindPersona,
    RevokeBinding,
    UpdatePolicy,
    SubmitTransaction,
    RecoverAccount,
    RotateKey,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PersonaType {
    Web30,
    Evm,
    Bitcoin,
    Solana,
    Other(String),
}

impl PersonaType {
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            PersonaType::Web30 => "web30",
            PersonaType::Evm => "evm",
            PersonaType::Bitcoin => "bitcoin",
            PersonaType::Solana => "solana",
            PersonaType::Other(other) => other.as_str(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PersonaAddress {
    pub persona_type: PersonaType,
    pub chain_id: u64,
    pub external_address: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UcaAccount {
    pub uca_id: String,
    pub primary_key_ref: Vec<u8>,
    pub status: UcaStatus,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonaBinding {
    pub uca_id: String,
    pub persona_type: PersonaType,
    pub chain_id: u64,
    pub external_address: Vec<u8>,
    pub binding_state: BindingState,
    pub bound_at: u64,
    pub revoked_at: Option<u64>,
    pub cooldown_until: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountPolicy {
    pub nonce_scope: NonceScope,
    pub allow_type4_with_delegate_or_session: bool,
}

impl Default for AccountPolicy {
    fn default() -> Self {
        Self {
            nonce_scope: NonceScope::Persona,
            allow_type4_with_delegate_or_session: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProtocolKind {
    Eth,
    Web30,
    Other(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteRequest {
    pub uca_id: String,
    pub persona: PersonaAddress,
    pub role: AccountRole,
    pub protocol: ProtocolKind,
    pub signature_domain: String,
    pub nonce: u64,
    pub wants_cross_chain_atomic: bool,
    pub tx_type4: bool,
    pub session_expires_at: Option<u64>,
    pub now: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RouteDecision {
    FastPath,
    Adapter { chain_id: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccountAuditEvent {
    UcaCreated {
        uca_id: String,
        at: u64,
    },
    BindingAdded {
        uca_id: String,
        persona: PersonaAddress,
        at: u64,
    },
    BindingConflictRejected {
        request_uca_id: String,
        existing_uca_id: String,
        persona: PersonaAddress,
        at: u64,
    },
    BindingRevoked {
        uca_id: String,
        persona: PersonaAddress,
        at: u64,
        cooldown_until: Option<u64>,
    },
    NonceReplayRejected {
        uca_id: String,
        scope: NonceScope,
        expected: u64,
        got: u64,
        at: u64,
    },
    DomainMismatchRejected {
        uca_id: String,
        expected: String,
        got: String,
        at: u64,
    },
    PermissionDenied {
        uca_id: String,
        role: AccountRole,
        action: AccountAction,
        at: u64,
    },
    KeyRotated {
        uca_id: String,
        at: u64,
    },
    SessionKeyExpired {
        uca_id: String,
        expires_at: u64,
        now: u64,
        at: u64,
    },
    Type4PolicyRejected {
        uca_id: String,
        role: AccountRole,
        at: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnifiedAccountError {
    UcaAlreadyExists {
        uca_id: String,
    },
    UcaNotFound {
        uca_id: String,
    },
    UcaNotActive {
        uca_id: String,
        status: UcaStatus,
    },
    BindingConflict {
        existing_uca_id: String,
    },
    BindingAlreadyExists,
    BindingNotFound,
    BindingNotOwnedByUca,
    CooldownActive {
        cooldown_until: u64,
    },
    PermissionDenied {
        role: AccountRole,
        action: AccountAction,
    },
    DomainMismatch {
        expected: String,
        got: String,
    },
    NonceReplay {
        expected: u64,
        got: u64,
    },
    PersonaBoundaryViolation,
    Type4PolicyViolation,
    SessionKeyExpired {
        expires_at: u64,
        now: u64,
    },
}

impl fmt::Display for UnifiedAccountError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UnifiedAccountError::UcaAlreadyExists { uca_id } => {
                write!(f, "UCA already exists: {uca_id}")
            }
            UnifiedAccountError::UcaNotFound { uca_id } => write!(f, "UCA not found: {uca_id}"),
            UnifiedAccountError::UcaNotActive { uca_id, status } => {
                write!(f, "UCA is not active: {uca_id:?} ({status:?})")
            }
            UnifiedAccountError::BindingConflict { existing_uca_id } => {
                write!(f, "binding conflict, existing owner: {existing_uca_id}")
            }
            UnifiedAccountError::BindingAlreadyExists => write!(f, "binding already exists"),
            UnifiedAccountError::BindingNotFound => write!(f, "binding not found"),
            UnifiedAccountError::BindingNotOwnedByUca => {
                write!(f, "binding does not belong to requested UCA")
            }
            UnifiedAccountError::CooldownActive { cooldown_until } => {
                write!(f, "binding cooldown active until {cooldown_until}")
            }
            UnifiedAccountError::PermissionDenied { role, action } => {
                write!(f, "permission denied for role {role:?} action {action:?}")
            }
            UnifiedAccountError::DomainMismatch { expected, got } => {
                write!(f, "domain mismatch: expected {expected}, got {got}")
            }
            UnifiedAccountError::NonceReplay { expected, got } => {
                write!(f, "nonce rejected: expected {expected}, got {got}")
            }
            UnifiedAccountError::PersonaBoundaryViolation => {
                write!(f, "eth persona cannot request cross-chain atomic operation")
            }
            UnifiedAccountError::Type4PolicyViolation => {
                write!(
                    f,
                    "type4 transaction cannot be used with delegate/session role by policy"
                )
            }
            UnifiedAccountError::SessionKeyExpired { expires_at, now } => {
                write!(f, "session key expired at {expires_at}, now {now}")
            }
        }
    }
}

impl std::error::Error for UnifiedAccountError {}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
struct PersonaKey {
    persona_type: PersonaType,
    chain_id: u64,
    external_address: Vec<u8>,
}

impl From<&PersonaAddress> for PersonaKey {
    fn from(value: &PersonaAddress) -> Self {
        Self {
            persona_type: value.persona_type.clone(),
            chain_id: value.chain_id,
            external_address: value.external_address.clone(),
        }
    }
}

impl PersonaKey {
    fn as_address(&self) -> PersonaAddress {
        PersonaAddress {
            persona_type: self.persona_type.clone(),
            chain_id: self.chain_id,
            external_address: self.external_address.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
enum NonceKey {
    Persona { uca_id: String, persona: PersonaKey },
    Chain { uca_id: String, chain_id: u64 },
    Global { uca_id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UcaRecord {
    account: UcaAccount,
    policy: AccountPolicy,
    bindings: HashMap<PersonaKey, PersonaBinding>,
}

/// Unified account core + router logic.
///
/// This router is intentionally in-memory and deterministic so upper layers can
/// integrate first, then swap in a persistent backend.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct UnifiedAccountRouter {
    ucas: HashMap<String, UcaRecord>,
    binding_owner: HashMap<PersonaKey, String>,
    cooldowns: HashMap<PersonaKey, u64>,
    nonces: HashMap<NonceKey, u64>,
    events: Vec<AccountAuditEvent>,
}

impl UnifiedAccountRouter {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create_uca(
        &mut self,
        uca_id: impl Into<String>,
        primary_key_ref: Vec<u8>,
        now: u64,
    ) -> Result<(), UnifiedAccountError> {
        let uca_id = uca_id.into();
        if self.ucas.contains_key(&uca_id) {
            return Err(UnifiedAccountError::UcaAlreadyExists { uca_id });
        }

        let account = UcaAccount {
            uca_id: uca_id.clone(),
            primary_key_ref,
            status: UcaStatus::Active,
            created_at: now,
            updated_at: now,
        };
        self.ucas.insert(
            uca_id.clone(),
            UcaRecord {
                account,
                policy: AccountPolicy::default(),
                bindings: HashMap::new(),
            },
        );
        self.events
            .push(AccountAuditEvent::UcaCreated { uca_id, at: now });
        Ok(())
    }

    pub fn update_policy(
        &mut self,
        uca_id: &str,
        role: AccountRole,
        policy: AccountPolicy,
        now: u64,
    ) -> Result<(), UnifiedAccountError> {
        self.ensure_permission(uca_id, role, AccountAction::UpdatePolicy, now)?;
        let record = self
            .ucas
            .get_mut(uca_id)
            .ok_or_else(|| UnifiedAccountError::UcaNotFound {
                uca_id: uca_id.to_string(),
            })?;
        record.policy = policy;
        record.account.updated_at = now;
        Ok(())
    }

    pub fn add_binding(
        &mut self,
        uca_id: &str,
        role: AccountRole,
        persona: PersonaAddress,
        now: u64,
    ) -> Result<(), UnifiedAccountError> {
        self.ensure_permission(uca_id, role, AccountAction::BindPersona, now)?;
        self.ensure_uca_active(uca_id)?;

        let key = PersonaKey::from(&persona);
        if let Some(cooldown_until) = self.cooldowns.get(&key) {
            if now < *cooldown_until {
                return Err(UnifiedAccountError::CooldownActive {
                    cooldown_until: *cooldown_until,
                });
            }
            self.cooldowns.remove(&key);
        }

        if let Some(existing_uca_id) = self.binding_owner.get(&key) {
            if existing_uca_id != uca_id {
                self.events
                    .push(AccountAuditEvent::BindingConflictRejected {
                        request_uca_id: uca_id.to_string(),
                        existing_uca_id: existing_uca_id.clone(),
                        persona,
                        at: now,
                    });
                return Err(UnifiedAccountError::BindingConflict {
                    existing_uca_id: existing_uca_id.clone(),
                });
            }
            return Err(UnifiedAccountError::BindingAlreadyExists);
        }

        let record = self
            .ucas
            .get_mut(uca_id)
            .ok_or_else(|| UnifiedAccountError::UcaNotFound {
                uca_id: uca_id.to_string(),
            })?;

        let binding = PersonaBinding {
            uca_id: uca_id.to_string(),
            persona_type: key.persona_type.clone(),
            chain_id: key.chain_id,
            external_address: key.external_address.clone(),
            binding_state: BindingState::Bound,
            bound_at: now,
            revoked_at: None,
            cooldown_until: None,
        };

        record.bindings.insert(key.clone(), binding);
        record.account.updated_at = now;
        self.binding_owner.insert(key, uca_id.to_string());
        self.events.push(AccountAuditEvent::BindingAdded {
            uca_id: uca_id.to_string(),
            persona,
            at: now,
        });
        Ok(())
    }

    pub fn revoke_binding(
        &mut self,
        uca_id: &str,
        role: AccountRole,
        persona: PersonaAddress,
        cooldown_seconds: u64,
        now: u64,
    ) -> Result<(), UnifiedAccountError> {
        self.ensure_permission(uca_id, role, AccountAction::RevokeBinding, now)?;
        let key = PersonaKey::from(&persona);
        let record = self
            .ucas
            .get_mut(uca_id)
            .ok_or_else(|| UnifiedAccountError::UcaNotFound {
                uca_id: uca_id.to_string(),
            })?;

        let binding = record
            .bindings
            .get_mut(&key)
            .ok_or(UnifiedAccountError::BindingNotFound)?;
        if binding.uca_id != uca_id {
            return Err(UnifiedAccountError::BindingNotOwnedByUca);
        }
        binding.binding_state = BindingState::Revoked;
        binding.revoked_at = Some(now);
        let cooldown_until = now.saturating_add(cooldown_seconds);
        binding.cooldown_until = Some(cooldown_until);

        self.binding_owner.remove(&key);
        self.cooldowns.insert(key.clone(), cooldown_until);
        record.account.updated_at = now;
        self.events.push(AccountAuditEvent::BindingRevoked {
            uca_id: uca_id.to_string(),
            persona: key.as_address(),
            at: now,
            cooldown_until: Some(cooldown_until),
        });
        Ok(())
    }

    pub fn route(&mut self, request: RouteRequest) -> Result<RouteDecision, UnifiedAccountError> {
        self.ensure_permission(
            &request.uca_id,
            request.role,
            AccountAction::SubmitTransaction,
            request.now,
        )?;
        self.ensure_uca_active(&request.uca_id)?;
        if request.role == AccountRole::SessionKey {
            if let Some(expires_at) = request.session_expires_at {
                if request.now > expires_at {
                    self.events.push(AccountAuditEvent::SessionKeyExpired {
                        uca_id: request.uca_id.clone(),
                        expires_at,
                        now: request.now,
                        at: request.now,
                    });
                    return Err(UnifiedAccountError::SessionKeyExpired {
                        expires_at,
                        now: request.now,
                    });
                }
            }
        }

        let key = PersonaKey::from(&request.persona);
        let owner = self
            .binding_owner
            .get(&key)
            .ok_or_else(|| UnifiedAccountError::BindingNotFound)?;
        if owner != &request.uca_id {
            return Err(UnifiedAccountError::BindingNotOwnedByUca);
        }

        let (expected_domain_hint, domain_ok) =
            self.verify_domain(&request.persona, &request.signature_domain);
        if !domain_ok {
            self.events.push(AccountAuditEvent::DomainMismatchRejected {
                uca_id: request.uca_id.clone(),
                expected: expected_domain_hint.clone(),
                got: request.signature_domain.clone(),
                at: request.now,
            });
            return Err(UnifiedAccountError::DomainMismatch {
                expected: expected_domain_hint,
                got: request.signature_domain,
            });
        }

        let policy = self
            .ucas
            .get(&request.uca_id)
            .ok_or_else(|| UnifiedAccountError::UcaNotFound {
                uca_id: request.uca_id.clone(),
            })?
            .policy
            .clone();

        if request.tx_type4
            && request.role != AccountRole::Owner
            && !policy.allow_type4_with_delegate_or_session
        {
            self.events.push(AccountAuditEvent::Type4PolicyRejected {
                uca_id: request.uca_id.clone(),
                role: request.role,
                at: request.now,
            });
            return Err(UnifiedAccountError::Type4PolicyViolation);
        }

        if request.protocol == ProtocolKind::Eth && request.wants_cross_chain_atomic {
            return Err(UnifiedAccountError::PersonaBoundaryViolation);
        }

        self.use_nonce(&request, policy.nonce_scope)?;

        if request.protocol == ProtocolKind::Web30 {
            return Ok(RouteDecision::FastPath);
        }

        Ok(RouteDecision::Adapter {
            chain_id: request.persona.chain_id,
        })
    }

    pub fn rotate_primary_key(
        &mut self,
        uca_id: &str,
        role: AccountRole,
        next_primary_key_ref: Vec<u8>,
        now: u64,
    ) -> Result<(), UnifiedAccountError> {
        self.ensure_permission(uca_id, role, AccountAction::RotateKey, now)?;
        self.ensure_uca_active(uca_id)?;
        let record = self
            .ucas
            .get_mut(uca_id)
            .ok_or_else(|| UnifiedAccountError::UcaNotFound {
                uca_id: uca_id.to_string(),
            })?;
        record.account.primary_key_ref = next_primary_key_ref;
        record.account.updated_at = now;
        self.events.push(AccountAuditEvent::KeyRotated {
            uca_id: uca_id.to_string(),
            at: now,
        });
        Ok(())
    }

    pub fn resolve_binding_owner(&self, persona: &PersonaAddress) -> Option<&str> {
        let key = PersonaKey::from(persona);
        self.binding_owner.get(&key).map(String::as_str)
    }

    pub fn next_nonce_for_persona(
        &self,
        uca_id: &str,
        persona: &PersonaAddress,
    ) -> Result<u64, UnifiedAccountError> {
        self.ensure_uca_active(uca_id)?;
        let persona_key = PersonaKey::from(persona);
        let owner = self
            .binding_owner
            .get(&persona_key)
            .ok_or(UnifiedAccountError::BindingNotFound)?;
        if owner != uca_id {
            return Err(UnifiedAccountError::BindingNotOwnedByUca);
        }
        let record = self
            .ucas
            .get(uca_id)
            .ok_or_else(|| UnifiedAccountError::UcaNotFound {
                uca_id: uca_id.to_string(),
            })?;
        let nonce_key = match record.policy.nonce_scope {
            NonceScope::Persona => NonceKey::Persona {
                uca_id: uca_id.to_string(),
                persona: persona_key,
            },
            NonceScope::Chain => NonceKey::Chain {
                uca_id: uca_id.to_string(),
                chain_id: persona.chain_id,
            },
            NonceScope::Global => NonceKey::Global {
                uca_id: uca_id.to_string(),
            },
        };
        Ok(self.nonces.get(&nonce_key).copied().unwrap_or(0))
    }

    #[must_use]
    pub fn events(&self) -> &[AccountAuditEvent] {
        &self.events
    }

    pub fn take_events(&mut self) -> Vec<AccountAuditEvent> {
        std::mem::take(&mut self.events)
    }

    fn use_nonce(
        &mut self,
        request: &RouteRequest,
        scope: NonceScope,
    ) -> Result<(), UnifiedAccountError> {
        let nonce_key = match scope {
            NonceScope::Persona => NonceKey::Persona {
                uca_id: request.uca_id.clone(),
                persona: PersonaKey::from(&request.persona),
            },
            NonceScope::Chain => NonceKey::Chain {
                uca_id: request.uca_id.clone(),
                chain_id: request.persona.chain_id,
            },
            NonceScope::Global => NonceKey::Global {
                uca_id: request.uca_id.clone(),
            },
        };

        let expected = self.nonces.get(&nonce_key).copied().unwrap_or(0);
        if request.nonce != expected {
            self.events.push(AccountAuditEvent::NonceReplayRejected {
                uca_id: request.uca_id.clone(),
                scope,
                expected,
                got: request.nonce,
                at: request.now,
            });
            return Err(UnifiedAccountError::NonceReplay {
                expected,
                got: request.nonce,
            });
        }
        self.nonces.insert(nonce_key, expected.saturating_add(1));
        Ok(())
    }

    fn verify_domain(&self, persona: &PersonaAddress, domain: &str) -> (String, bool) {
        match persona.persona_type {
            PersonaType::Evm => {
                let eth = format!("evm:{}", persona.chain_id);
                let personal = format!("evm-personal:{}", persona.chain_id);
                let eip712 = format!("eip712:{}:", persona.chain_id);
                let ok = domain == eth || domain == personal || domain.starts_with(&eip712);
                (format!("{eth} | {personal} | {eip712}<app>"), ok)
            }
            PersonaType::Web30 => ("web30:<network>".to_string(), domain.starts_with("web30:")),
            _ => {
                let expected = format!("{}:{}", persona.persona_type.as_str(), persona.chain_id);
                let ok = domain.starts_with(&expected);
                (format!("{expected}[:suffix]"), ok)
            }
        }
    }

    fn ensure_uca_active(&self, uca_id: &str) -> Result<(), UnifiedAccountError> {
        let record = self
            .ucas
            .get(uca_id)
            .ok_or_else(|| UnifiedAccountError::UcaNotFound {
                uca_id: uca_id.to_string(),
            })?;
        if record.account.status != UcaStatus::Active {
            return Err(UnifiedAccountError::UcaNotActive {
                uca_id: uca_id.to_string(),
                status: record.account.status,
            });
        }
        Ok(())
    }

    fn ensure_permission(
        &mut self,
        uca_id: &str,
        role: AccountRole,
        action: AccountAction,
        now: u64,
    ) -> Result<(), UnifiedAccountError> {
        let allowed = match role {
            AccountRole::Owner => true,
            AccountRole::Delegate => matches!(action, AccountAction::SubmitTransaction),
            AccountRole::SessionKey => matches!(action, AccountAction::SubmitTransaction),
        };
        if !allowed {
            self.events.push(AccountAuditEvent::PermissionDenied {
                uca_id: uca_id.to_string(),
                role,
                action,
                at: now,
            });
            return Err(UnifiedAccountError::PermissionDenied { role, action });
        }
        Ok(())
    }
}
