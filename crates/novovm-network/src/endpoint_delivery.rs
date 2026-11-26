//! Optional live-device delivery on the existing authenticated native session.
//! No custody database or storage receipt: the application owns E2EE and ACKs.
use anyhow::{ensure, Result};
use futures_util::future::BoxFuture;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, Weak,
    },
};
use subtle::ConstantTimeEq;
use zeroize::{Zeroize, Zeroizing};

pub const MAX_ENDPOINT_DELIVERY_BYTES: usize = 65_536;
const MAX_ROUTES: usize = 256;

pub struct EndpointDeliveryRequestV1 {
    pub mailbox: [u8; 32],
    pub object: [u8; 32],
    pub ciphertext: Vec<u8>,
    pub expires: i64,
}

#[cfg(test)]
#[path = "endpoint_delivery_tests.rs"]
mod tests;
impl Drop for EndpointDeliveryRequestV1 {
    fn drop(&mut self) {
        self.ciphertext.zeroize();
    }
}
pub enum EndpointDeliveryResultV1 {
    Delivered {
        receipt_object: [u8; 32],
        receipt_expires: i64,
        receipt: Vec<u8>,
    },
    Unavailable,
    Rejected,
}
pub type EndpointDeliveryHandlerV1 = Arc<
    dyn Fn(EndpointDeliveryRequestV1) -> BoxFuture<'static, Result<EndpointDeliveryResultV1>>
        + Send
        + Sync,
>;

struct Route {
    generation: u64,
    write: Zeroizing<[u8; 32]>,
    handler: EndpointDeliveryHandlerV1,
    active: Arc<AtomicBool>,
}
// Exclusive mailbox registrations rotate their single writer. A device
// destination can instead hold independently revocable write grants.
type RouteKey = ([u8; 32], Option<[u8; 32]>);

#[derive(Default)]
struct State {
    generation: u64,
    routes: BTreeMap<RouteKey, Route>,
}
#[derive(Default)]
pub(crate) struct EndpointDeliveryRegistryV1(Arc<Mutex<State>>);

/// Dropping the owner registration withdraws only its own generation.
pub struct EndpointDeliveryRegistrationV1 {
    state: Weak<Mutex<State>>,
    key: RouteKey,
    generation: u64,
    active: Arc<AtomicBool>,
}
impl Drop for EndpointDeliveryRegistrationV1 {
    fn drop(&mut self) {
        self.active.store(false, Ordering::Release);
        if let Some(state) = self.state.upgrade() {
            if let Ok(mut state) = state.lock() {
                if state
                    .routes
                    .get(&self.key)
                    .is_some_and(|r| r.generation == self.generation)
                {
                    state.routes.remove(&self.key);
                }
            }
        }
    }
}
impl EndpointDeliveryRegistryV1 {
    pub(crate) fn register(
        &self,
        mailbox: [u8; 32],
        write: [u8; 32],
        handler: EndpointDeliveryHandlerV1,
    ) -> Result<EndpointDeliveryRegistrationV1> {
        self.register_route(mailbox, write, handler, None)
    }

    pub(crate) fn register_grant(
        &self,
        destination: [u8; 32],
        write: [u8; 32],
        handler: EndpointDeliveryHandlerV1,
    ) -> Result<EndpointDeliveryRegistrationV1> {
        let tag = Sha256::digest(write).into();
        self.register_route(destination, write, handler, Some(tag))
    }

    fn register_route(
        &self,
        mailbox: [u8; 32],
        write: [u8; 32],
        handler: EndpointDeliveryHandlerV1,
        tag: Option<[u8; 32]>,
    ) -> Result<EndpointDeliveryRegistrationV1> {
        let write = Zeroizing::new(write);
        ensure!(
            mailbox != [0; 32] && *write != [0; 32],
            "endpoint delivery route/capability"
        );
        let mut state = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("endpoint delivery registry unavailable"))?;
        let key = (mailbox, tag);
        if let Some(((address, existing), _)) = state.routes.range((mailbox, None)..).next() {
            ensure!(
                *address != mailbox || existing.is_some() == tag.is_some(),
                "endpoint delivery address registration mode"
            );
        }
        ensure!(
            state.routes.contains_key(&key) || state.routes.len() < MAX_ROUTES,
            "endpoint delivery route capacity"
        );
        let generation = state
            .generation
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("endpoint delivery generation exhausted"))?;
        state.generation = generation;
        let active = Arc::new(AtomicBool::new(true));
        if let Some(old) = state.routes.insert(
            key,
            Route {
                generation,
                write,
                handler,
                active: active.clone(),
            },
        ) {
            old.active.store(false, Ordering::Release);
        }
        Ok(EndpointDeliveryRegistrationV1 {
            state: Arc::downgrade(&self.0),
            key,
            generation,
            active,
        })
    }
    pub(crate) async fn deliver(
        &self,
        write: &[u8; 32],
        request: EndpointDeliveryRequestV1,
    ) -> Result<EndpointDeliveryResultV1> {
        ensure!(
            !request.ciphertext.is_empty()
                && request.ciphertext.len() <= MAX_ENDPOINT_DELIVERY_BYTES,
            "endpoint delivery ciphertext bound"
        );
        ensure!(request.expires > 0, "endpoint delivery expiry");
        let (handler, active) = {
            let state = self
                .0
                .lock()
                .map_err(|_| anyhow::anyhow!("endpoint delivery registry unavailable"))?;
            let mailbox = request.mailbox;
            let Some(((address, mode), first)) = state.routes.range((mailbox, None)..).next()
            else {
                return Ok(EndpointDeliveryResultV1::Unavailable);
            };
            if *address != mailbox {
                return Ok(EndpointDeliveryResultV1::Unavailable);
            };
            let route = if mode.is_none() {
                first
            } else {
                let tag = Sha256::digest(write).into();
                let Some(route) = state.routes.get(&(mailbox, Some(tag))) else {
                    return Ok(EndpointDeliveryResultV1::Rejected);
                };
                route
            };
            if !bool::from(route.write.as_ref().ct_eq(write)) {
                return Ok(EndpointDeliveryResultV1::Rejected);
            }
            (route.handler.clone(), route.active.clone())
        };
        if !active.load(Ordering::Acquire) {
            return Ok(EndpointDeliveryResultV1::Unavailable);
        }
        let result = handler(request).await?;
        // A concurrent withdrawal cannot undo an application commit. Suppress
        // success and let the original message reconcile with its durable ACK.
        if !active.load(Ordering::Acquire) {
            return Ok(EndpointDeliveryResultV1::Unavailable);
        }
        if let EndpointDeliveryResultV1::Delivered {
            receipt_object,
            receipt_expires,
            receipt,
        } = &result
        {
            ensure!(
                *receipt_object != [0; 32]
                    && *receipt_expires > 0
                    && !receipt.is_empty()
                    && receipt.len() <= MAX_ENDPOINT_DELIVERY_BYTES,
                "endpoint device receipt bounds"
            );
        }
        Ok(result)
    }
}
