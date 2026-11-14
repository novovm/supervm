// Added only to the copied core::fmt module, never installed toolchain files.
// Each image owns this callback slot; dependency cdylibs leave theirs unset.
static NOVOVM_DIAG_HOOK: crate::sync::atomic::AtomicPtr<()> = crate::sync::atomic::AtomicPtr::new(crate::ptr::null_mut());
#[unstable(feature="novovm_diag", issue="none")]
pub fn __novovm_diag_set_hook(hook:extern "C" fn(u64,usize,usize,usize,usize,usize,usize,usize)) {
 let _=NOVOVM_DIAG_HOOK.compare_exchange(crate::ptr::null_mut(),hook as *mut (),crate::sync::atomic::Ordering::Relaxed,crate::sync::atomic::Ordering::Relaxed).expect("diagnostic callback installed once");
}
#[unstable(feature="novovm_diag", issue="none")]
#[inline]
pub fn __novovm_diag_emit(k:u64,o:usize,b:usize,l:usize,c:usize,t:usize,n:usize,f:usize) {
 let address=NOVOVM_DIAG_HOOK.load(crate::sync::atomic::Ordering::Relaxed);
 if !address.is_null() {
  // SAFETY: the slot is initialized exactly once from a non-null static function
  // of this exact ABI/signature, and is never unloaded or replaced.
  let hook:extern "C" fn(u64,usize,usize,usize,usize,usize,usize,usize)=unsafe {crate::mem::transmute(address)};
  hook(k,o,b,l,c,t,n,f);
 }
}
