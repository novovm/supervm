use std::{ffi::c_void, ptr};
core::arch::global_asm!(include_str!("probe.S"));
unsafe extern "C" {
    fn abi_probe(function: usize, args: *const usize, result: *mut i32) -> u64;
    fn clobber_control();
}
unsafe extern "C" fn clean_control(_: *mut c_void) -> i32 { 7 }
unsafe fn probe(f: usize, args: [usize; 8]) -> i32 {
    let mut result = -999;
    let mask = abi_probe(f, args.as_ptr(), &mut result);
    assert_eq!(mask, 0, "callee-saved ABI register corruption");
    result
}
fn main() { unsafe {
    let mut result = 0;
    let corrupt = abi_probe(clobber_control as *const () as usize, [0usize;8].as_ptr(), &mut result);
    assert_eq!(corrupt, 1 | (1<<8), "probe must detect intentional RBX/XMM6 clobber");
    assert_eq!(probe(clean_control as *const () as usize, [0;8]), 7);
    let path = std::env::var("C_DIAGNOSTIC_AOEM_DLL").unwrap();
    let dll = libloading::Library::new(path).unwrap();
    let load = |name: &[u8]| -> usize { *dll.get::<unsafe extern "C" fn()>(name).unwrap() as usize };
    let keygen=load(b"aoem_mldsa_keygen_v1\0");
    let sign=load(b"aoem_mldsa_sign_v1\0");
    let verify=load(b"aoem_mldsa_verify\0");
    let free=*dll.get::<unsafe extern "C" fn(*mut u8,usize)>(b"aoem_free\0").unwrap();
    let (mut pk,mut sk)= (ptr::null_mut::<u8>(),ptr::null_mut::<u8>());
    let (mut pl,mut sl)=(0usize,0usize);
    assert_eq!(probe(keygen,[65,(&mut pk as *mut *mut u8) as usize,(&mut pl as *mut usize) as usize,
        (&mut sk as *mut *mut u8) as usize,(&mut sl as *mut usize) as usize,0,0,0]),0);
    assert_eq!((pl,sl),(1952,4032));
    for n in 0..10000u64 {
        let msg=n.to_le_bytes();
        let (mut sig,mut len)=(ptr::null_mut::<u8>(),0usize);
        assert_eq!(probe(sign,[65,sk as usize,sl,msg.as_ptr() as usize,msg.len(),
            (&mut sig as *mut *mut u8) as usize,(&mut len as *mut usize) as usize,0]),0);
        assert_eq!(len,3309);
        let mut valid=0u32;
        assert_eq!(probe(verify,[65,pk as usize,pl,msg.as_ptr() as usize,msg.len(),sig as usize,len,
            (&mut valid as *mut u32) as usize]),0);
        assert_eq!(valid,1);
        let mut bad=msg; bad[0]^=1;
        assert_eq!(probe(verify,[65,pk as usize,pl,bad.as_ptr() as usize,bad.len(),sig as usize,len,
            (&mut valid as *mut u32) as usize]),0);
        assert_eq!(valid,0);
        free(sig,len);
    }
    use zeroize::Zeroize;
    std::slice::from_raw_parts_mut(sk,sl).zeroize();free(sk,sl);free(pk,pl);
    println!("{}",serde_json::json!({"status":"PASS","iterations":10000,"direct_ffi_calls":30001,
        "checked_gpr":8,"checked_xmm":10,"clobber_negative_control":true,"node_or_database_used":false}));
}}
