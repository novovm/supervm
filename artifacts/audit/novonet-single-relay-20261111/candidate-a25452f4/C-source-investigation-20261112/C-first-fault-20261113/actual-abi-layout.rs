mod host {
#[repr(C)]
pub struct AoemCreateOptionsV1 {
    pub abi_version: u32,
    pub struct_size: u32,
    pub ingress_workers: u32,
    pub flags: u32,
}
#[repr(C)]
pub struct AoemOpV2 {
    pub opcode: u8, // 1=read,2=write,3=add_i64,4=inc_i64
    pub flags: u8,
    pub reserved: u16,
    pub key_ptr: *const u8,
    pub key_len: u32,
    pub value_ptr: *const u8,
    pub value_len: u32,
    pub delta: i64,
    pub expect_version: u64, // u64::MAX means None
    pub plan_id: u64,        // 0 => auto
}
#[repr(C)]
pub struct AoemExecV2Result {
    pub processed: u32,
    pub success: u32,
    pub failed_index: u32, // u32::MAX means none
    pub total_writes: u64,
}
#[repr(C)]
pub struct AoemMldsaVerifyItemV1 {
    pub level: u32,
    pub pubkey_ptr: *const u8,
    pub pubkey_len: usize,
    pub message_ptr: *const u8,
    pub message_len: usize,
    pub signature_ptr: *const u8,
    pub signature_len: usize,
}
}
mod runtime {
#[repr(C)]
pub struct AoemCreateOptionsV1 {
    pub abi_version: u32,
    pub struct_size: u32,
    pub ingress_workers: u32,
    pub flags: u32,
}
#[repr(C)]
pub struct AoemOpV2 {
    pub opcode: u8, // 1=read,2=write,3=add_i64,4=inc_i64
    pub flags: u8,  // reserved
    pub reserved: u16,
    pub key_ptr: *const u8,
    pub key_len: u32,
    pub value_ptr: *const u8,
    pub value_len: u32,
    pub delta: i64,
    // u64::MAX means None
    pub expect_version: u64,
    // optional caller-provided plan id; 0 => auto
    pub plan_id: u64,
}
#[repr(C)]
pub struct AoemExecV2Result {
    pub processed: u32,
    pub success: u32,
    pub failed_index: u32, // u32::MAX means none
    pub total_writes: u64,
}
#[repr(C)]
pub struct AoemMldsaVerifyItemV1 {
    // 0 => auto-detect by pubkey length; otherwise 44/65/87 (or legacy 2/3/5).
    pub level: u32,
    pub pubkey_ptr: *const u8,
    pub pubkey_len: usize,
    pub message_ptr: *const u8,
    pub message_len: usize,
    pub signature_ptr: *const u8,
    pub signature_len: usize,
}
}
fn main(){
assert_eq!(std::mem::size_of::<host::AoemCreateOptionsV1>(), std::mem::size_of::<runtime::AoemCreateOptionsV1>());
assert_eq!(std::mem::align_of::<host::AoemCreateOptionsV1>(), std::mem::align_of::<runtime::AoemCreateOptionsV1>());
assert_eq!(std::mem::offset_of!(host::AoemCreateOptionsV1, abi_version), std::mem::offset_of!(runtime::AoemCreateOptionsV1, abi_version));
assert_eq!(std::mem::offset_of!(host::AoemCreateOptionsV1, struct_size), std::mem::offset_of!(runtime::AoemCreateOptionsV1, struct_size));
assert_eq!(std::mem::offset_of!(host::AoemCreateOptionsV1, ingress_workers), std::mem::offset_of!(runtime::AoemCreateOptionsV1, ingress_workers));
assert_eq!(std::mem::offset_of!(host::AoemCreateOptionsV1, flags), std::mem::offset_of!(runtime::AoemCreateOptionsV1, flags));
println!("AoemCreateOptionsV1 size={} align={} fields=4",std::mem::size_of::<host::AoemCreateOptionsV1>(),std::mem::align_of::<host::AoemCreateOptionsV1>());
assert_eq!(std::mem::size_of::<host::AoemOpV2>(), std::mem::size_of::<runtime::AoemOpV2>());
assert_eq!(std::mem::align_of::<host::AoemOpV2>(), std::mem::align_of::<runtime::AoemOpV2>());
assert_eq!(std::mem::offset_of!(host::AoemOpV2, opcode), std::mem::offset_of!(runtime::AoemOpV2, opcode));
assert_eq!(std::mem::offset_of!(host::AoemOpV2, flags), std::mem::offset_of!(runtime::AoemOpV2, flags));
assert_eq!(std::mem::offset_of!(host::AoemOpV2, reserved), std::mem::offset_of!(runtime::AoemOpV2, reserved));
assert_eq!(std::mem::offset_of!(host::AoemOpV2, key_ptr), std::mem::offset_of!(runtime::AoemOpV2, key_ptr));
assert_eq!(std::mem::offset_of!(host::AoemOpV2, key_len), std::mem::offset_of!(runtime::AoemOpV2, key_len));
assert_eq!(std::mem::offset_of!(host::AoemOpV2, value_ptr), std::mem::offset_of!(runtime::AoemOpV2, value_ptr));
assert_eq!(std::mem::offset_of!(host::AoemOpV2, value_len), std::mem::offset_of!(runtime::AoemOpV2, value_len));
assert_eq!(std::mem::offset_of!(host::AoemOpV2, delta), std::mem::offset_of!(runtime::AoemOpV2, delta));
assert_eq!(std::mem::offset_of!(host::AoemOpV2, expect_version), std::mem::offset_of!(runtime::AoemOpV2, expect_version));
assert_eq!(std::mem::offset_of!(host::AoemOpV2, plan_id), std::mem::offset_of!(runtime::AoemOpV2, plan_id));
println!("AoemOpV2 size={} align={} fields=10",std::mem::size_of::<host::AoemOpV2>(),std::mem::align_of::<host::AoemOpV2>());
assert_eq!(std::mem::size_of::<host::AoemExecV2Result>(), std::mem::size_of::<runtime::AoemExecV2Result>());
assert_eq!(std::mem::align_of::<host::AoemExecV2Result>(), std::mem::align_of::<runtime::AoemExecV2Result>());
assert_eq!(std::mem::offset_of!(host::AoemExecV2Result, processed), std::mem::offset_of!(runtime::AoemExecV2Result, processed));
assert_eq!(std::mem::offset_of!(host::AoemExecV2Result, success), std::mem::offset_of!(runtime::AoemExecV2Result, success));
assert_eq!(std::mem::offset_of!(host::AoemExecV2Result, failed_index), std::mem::offset_of!(runtime::AoemExecV2Result, failed_index));
assert_eq!(std::mem::offset_of!(host::AoemExecV2Result, total_writes), std::mem::offset_of!(runtime::AoemExecV2Result, total_writes));
println!("AoemExecV2Result size={} align={} fields=4",std::mem::size_of::<host::AoemExecV2Result>(),std::mem::align_of::<host::AoemExecV2Result>());
assert_eq!(std::mem::size_of::<host::AoemMldsaVerifyItemV1>(), std::mem::size_of::<runtime::AoemMldsaVerifyItemV1>());
assert_eq!(std::mem::align_of::<host::AoemMldsaVerifyItemV1>(), std::mem::align_of::<runtime::AoemMldsaVerifyItemV1>());
assert_eq!(std::mem::offset_of!(host::AoemMldsaVerifyItemV1, level), std::mem::offset_of!(runtime::AoemMldsaVerifyItemV1, level));
assert_eq!(std::mem::offset_of!(host::AoemMldsaVerifyItemV1, pubkey_ptr), std::mem::offset_of!(runtime::AoemMldsaVerifyItemV1, pubkey_ptr));
assert_eq!(std::mem::offset_of!(host::AoemMldsaVerifyItemV1, pubkey_len), std::mem::offset_of!(runtime::AoemMldsaVerifyItemV1, pubkey_len));
assert_eq!(std::mem::offset_of!(host::AoemMldsaVerifyItemV1, message_ptr), std::mem::offset_of!(runtime::AoemMldsaVerifyItemV1, message_ptr));
assert_eq!(std::mem::offset_of!(host::AoemMldsaVerifyItemV1, message_len), std::mem::offset_of!(runtime::AoemMldsaVerifyItemV1, message_len));
assert_eq!(std::mem::offset_of!(host::AoemMldsaVerifyItemV1, signature_ptr), std::mem::offset_of!(runtime::AoemMldsaVerifyItemV1, signature_ptr));
assert_eq!(std::mem::offset_of!(host::AoemMldsaVerifyItemV1, signature_len), std::mem::offset_of!(runtime::AoemMldsaVerifyItemV1, signature_len));
println!("AoemMldsaVerifyItemV1 size={} align={} fields=7",std::mem::size_of::<host::AoemMldsaVerifyItemV1>(),std::mem::align_of::<host::AoemMldsaVerifyItemV1>());
}
