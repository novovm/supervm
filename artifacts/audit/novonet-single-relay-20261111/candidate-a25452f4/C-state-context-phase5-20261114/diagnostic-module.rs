//! Private Phase5 test bridge. Not a production change.
pub fn record_test_delivery(n:u8) {
    crate::native_pipeline::profile::enable_native_delivery_observation();
    crate::native_pipeline::profile::delivery_event("phase5_test_only",0,[n;32],[n.wrapping_add(1);32],1);
}
