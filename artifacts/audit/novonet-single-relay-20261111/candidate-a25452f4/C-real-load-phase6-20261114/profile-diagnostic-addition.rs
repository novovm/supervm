
// Phase6 private diagnostic: read ring metadata only, not part of production source.
pub(crate) fn phase6_ring_shape() -> (usize,usize,usize,u64,u64) {
 let b=DELIVERY.lock().expect("test ring lock");let (a,z)=b.records.as_slices();
 (b.records.len(),a.len(),z.len(),b.dropped,b.next_seq)
}
