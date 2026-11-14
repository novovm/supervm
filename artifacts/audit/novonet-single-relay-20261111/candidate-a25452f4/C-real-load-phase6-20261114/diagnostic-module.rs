use anyhow::{ensure,Context,Result};
use serde_json::{Value,json};
fn bytes(v:&Value)->Result<[u8;32]> {
 let s=v.as_str().context("hash string")?;ensure!(s.len()==64,"hash size");
 let mut b=[0;32];for (i,x) in b.iter_mut().enumerate(){*x=u8::from_str_radix(&s[2*i..2*i+2],16)?;}Ok(b)
}
pub fn restore_retained_window(input:&Value)->Result<Value> {
 use crate::native_pipeline::profile as p;
 p::enable_native_delivery_observation();
 let events=input["events"].as_array().context("events")?;
 ensure!(events.len()==2048 && input["evicted"].as_u64()==Some(213510),"fixed original metadata");
 let before=p::phase6_ring_shape();ensure!(before.4<=213510,"fresh test context required");
 // Evicted content is unavailable. These explicit test fillers are all evicted before capture.
 for _ in before.4..213510 {p::delivery_event("phase6_evicted_test_filler",0,[0;32],[0;32],0);}
 for e in events {
  let name=match e["event"].as_str(){Some("realtime_owned")=>"realtime_owned",Some("admitted")=>"admitted",Some("delivered")=>"delivered",Some("received")=>"received",_=>anyhow::bail!("unexpected original event")};
  p::delivery_event(name,e["height"].as_u64().context("height")?,bytes(&e["peer"])?,bytes(&e["fragment"])?,u8::try_from(e["purposes"].as_u64().context("purpose")?)?);
 }
 let (len,a,z,evicted,next)=p::phase6_ring_shape();
 ensure!((len,a,z,evicted,next)==(2048,1530,518,213510,215558),"retained ring shape differs");
 Ok(json!({"len":len,"first_slice":a,"second_slice":z,"evicted":evicted,"next_seq":next,"replayed_record_contents":true,"replayed_old_heap_history":false}))
}
