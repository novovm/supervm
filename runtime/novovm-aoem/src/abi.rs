//! Reviewed subset of aoem/windows/include/aoem.h (SDK source 56e9da15).
//! Layouts match the domain-neutral V2 graph ABI, not a Host business wire.

use std::ffi::{c_char, c_void};

pub(crate) const GRAPH_ABI: u16 = 2;
pub(crate) const OK: i32 = 0;
pub(crate) const INVALID_ARGUMENT: i32 = -1;
pub(crate) const GRAPH_FAULTED: i32 = -9;
pub(crate) const CALLBACK_PANICKED: i32 = -11;

#[repr(C)]
pub(crate) struct CreateOptions {
    pub abi_version: u32,
    pub struct_size: u32,
    pub ingress_workers: u32,
    pub flags: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct TaskDescriptor {
    pub abi_version: u16,
    pub task_kind: u16,
    pub payload_len: u16,
    pub priority: u8,
    pub flags: u8,
    pub graph_id: u64,
    pub task_id: u64,
    pub context_handle: u64,
    pub sequence: u64,
    pub payload: [u8; 88],
}

impl Default for TaskDescriptor {
    fn default() -> Self {
        Self {
            abi_version: GRAPH_ABI,
            task_kind: 0,
            payload_len: 0,
            priority: 0,
            flags: 0,
            graph_id: 0,
            task_id: 0,
            context_handle: 0,
            sequence: 0,
            payload: [0; 88],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct StateEvent {
    pub abi_version: u16,
    pub event_kind: u16,
    pub payload_len: u16,
    pub flags: u16,
    pub graph_id: u64,
    pub task_id: u64,
    pub context_handle: u64,
    pub sequence: u64,
    pub payload: [u8; 216],
}

impl Default for StateEvent {
    fn default() -> Self {
        Self {
            abi_version: GRAPH_ABI,
            event_kind: 0,
            payload_len: 0,
            flags: 0,
            graph_id: 0,
            task_id: 0,
            context_handle: 0,
            sequence: 0,
            payload: [0; 216],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct StepOutput {
    pub flags: u32,
    pub reserved: u32,
    pub continuation: TaskDescriptor,
    pub emitted_task: TaskDescriptor,
    pub event: StateEvent,
}

#[repr(C)]
pub(crate) struct SubmitOptions {
    pub abi_version: u16,
    pub flags: u16,
    pub max_queued_tasks: u32,
    pub event_capacity: u32,
    pub initial_event_sequence: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Completion {
    pub abi_version: u16,
    pub reserved: u16,
    pub status: i32,
    pub graph_id: u64,
    pub processed: u64,
    pub succeeded: u64,
    pub failed: u64,
    pub would_block_retries: u64,
    pub peak_queued_tasks: u64,
}

#[repr(C)]
pub(crate) struct Callbacks {
    pub execute: Option<
        unsafe extern "C-unwind" fn(*const TaskDescriptor, *mut StepOutput, *mut c_void) -> i32,
    >,
    pub retain_context: Option<unsafe extern "C-unwind" fn(u64, *mut c_void) -> i32>,
    pub release_context: Option<unsafe extern "C-unwind" fn(u64, *mut c_void) -> i32>,
    pub state_event: Option<unsafe extern "C-unwind" fn(*const StateEvent, *mut c_void) -> i32>,
    pub completion: Option<unsafe extern "C-unwind" fn(*const Completion, *mut c_void)>,
    pub user_data: *mut c_void,
}

pub(crate) type AbiVersion = unsafe extern "C" fn() -> u32;
pub(crate) type GlobalInit = unsafe extern "C" fn() -> i32;
pub(crate) type Create = unsafe extern "C" fn(*const CreateOptions) -> *mut c_void;
pub(crate) type Destroy = unsafe extern "C" fn(*mut c_void);
pub(crate) type Submit = unsafe extern "C" fn(
    *mut c_void,
    *const TaskDescriptor,
    u32,
    *const SubmitOptions,
    *const Callbacks,
) -> i32;
pub(crate) type Cancel = unsafe extern "C" fn(*mut c_void, u64) -> i32;
pub(crate) type ActiveCount = unsafe extern "C" fn(*mut c_void) -> u64;
pub(crate) type LastError = unsafe extern "C" fn(*mut c_void) -> *const c_char;

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{offset_of, size_of};

    #[test]
    fn layouts_match_the_packaged_c_header() {
        assert_eq!(size_of::<CreateOptions>(), 16);
        assert_eq!(size_of::<TaskDescriptor>(), 128);
        assert_eq!(offset_of!(TaskDescriptor, graph_id), 8);
        assert_eq!(offset_of!(TaskDescriptor, payload), 40);
        assert_eq!(size_of::<StateEvent>(), 256);
        assert_eq!(offset_of!(StateEvent, payload), 40);
        assert_eq!(size_of::<StepOutput>(), 520);
        assert_eq!(offset_of!(StepOutput, event), 264);
        assert_eq!(size_of::<Completion>(), 56);
        assert_eq!(size_of::<Callbacks>(), 6 * size_of::<*const c_void>());
        #[cfg(target_pointer_width = "64")]
        {
            assert_eq!(size_of::<SubmitOptions>(), 24);
            assert_eq!(offset_of!(SubmitOptions, initial_event_sequence), 16);
        }
    }
}
