//! Bounded completion accounting, not a delivery cache. Includes every terminal
//! attempt (expiry/cancellation too); pending jobs remain in network totals only.
use novovm_network::novorudp::{
    diagnostics::NativeObjectTraffic, object_link::NativeSendCompletion,
};
use serde::Serialize;
use serde_json::{json, Value};
use std::{cell::RefCell, collections::BTreeMap};

#[derive(Default, Serialize)]
struct Totals {
    attempts: u64,
    object_bytes: u64,
    wire: [u64; 6],
}
impl Totals {
    fn add(&mut self, object_bytes: usize, t: NativeObjectTraffic) {
        self.attempts += 1;
        self.object_bytes += object_bytes as u64;
        for (total, value) in self.wire.iter_mut().zip([
            t.initial.packets,
            t.initial.bytes,
            t.repair.packets,
            t.repair.bytes,
            t.probe.packets,
            t.probe.bytes,
        ]) {
            *total += value;
        }
    }
}
#[derive(Default)]
struct Ledger {
    totals: BTreeMap<String, Totals>,
    bodies: Vec<Value>,
    omitted_bodies: u64,
}
thread_local! { static LEDGER: RefCell<Ledger> = RefCell::new(Ledger::default()); }

pub(crate) fn completed(
    done: &NativeSendCompletion,
    height: u64,
    payload: Option<[u8; 32]>,
    purposes: u8,
) {
    let Some(wire) = done.traffic else {
        return;
    };
    LEDGER.with(|ledger| {
        let mut ledger = ledger.borrow_mut();
        // Three configured validator streams, four purpose combinations and
        // five enum outcomes; inputs cannot create arbitrary keys here.
        let key = format!("{}:{purposes}:{:?}", done.stream, done.state);
        ledger.totals.entry(key).or_default().add(done.body.byte_len(), wire);
        if let Some(payload) = payload {
            if ledger.bodies.len() < 256 {
                ledger.bodies.push(json!({"height":height,"peer":done.peer,
                    "fragment":done.body.id(),"payload":payload,"purposes":purposes,
                    "state":format!("{:?}",done.state),"object_bytes":done.body.byte_len(),"wire":wire}));
            } else { ledger.omitted_bodies += 1; }
        }
    });
}
pub(crate) fn snapshot() -> Value {
    LEDGER.with(|l| {
        let l = l.borrow();
        json!({"scope":"terminal attempts only; pending jobs excluded; writes are not peer delivery; payload equality is not proof a reply was unnecessary",
            "purpose_bits":{"realtime":1,"archive":2},
            "wire_columns":["initial_packets","initial_bytes","repair_packets","repair_bytes","probe_packets","probe_bytes"],
            "totals":l.totals,"bodies":l.bodies,"omitted_bodies":l.omitted_bodies})
    })
}
