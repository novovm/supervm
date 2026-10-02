use super::*;
use std::cell::RefCell;

#[derive(Default)]
struct Stub {
    status: i32,
    response: Option<Vec<u8>>,
    reported_length: Option<usize>,
    allocation: Option<(*mut u8, usize)>,
    prove_calls: usize,
    verify_calls: usize,
    free_calls: usize,
    freed_length: usize,
    bad_free: bool,
    output_slots_zeroed: bool,
    elf: Vec<u8>,
    input: Vec<u8>,
    image: [u32; 8],
    receipt: Vec<u8>,
    journal: Vec<u8>,
    expected: Option<([u32; 8], Vec<u8>)>,
    abi: u32,
    init_calls: usize,
}

thread_local! {
    static STUB: RefCell<Stub> = RefCell::new(Stub::default());
}

fn reset(stub: Stub) {
    STUB.with(|slot| {
        assert!(
            slot.borrow().allocation.is_none(),
            "previous native allocation leaked"
        );
        *slot.borrow_mut() = stub;
    });
}

unsafe extern "C" fn version() -> u32 {
    STUB.with(|slot| slot.borrow().abi)
}

unsafe extern "C" fn init() -> i32 {
    STUB.with(|slot| {
        let mut state = slot.borrow_mut();
        state.init_calls += 1;
        state.status
    })
}

unsafe extern "C" fn prove(
    elf: *const u8,
    elf_len: usize,
    input: *const u8,
    input_len: usize,
    image: *const u32,
    output: *mut *mut u8,
    output_len: *mut usize,
) -> i32 {
    STUB.with(|slot| {
        let mut state = slot.borrow_mut();
        state.prove_calls += 1;
        state.output_slots_zeroed = unsafe { (*output).is_null() && *output_len == 0 };
        state.elf = unsafe { std::slice::from_raw_parts(elf, elf_len) }.to_vec();
        state.input = unsafe { std::slice::from_raw_parts(input, input_len) }.to_vec();
        state
            .image
            .copy_from_slice(unsafe { std::slice::from_raw_parts(image, 8) });
        let (pointer, actual_len) = if let Some(bytes) = state.response.take() {
            let bytes = bytes.into_boxed_slice();
            let actual_len = bytes.len();
            let pointer = Box::into_raw(bytes).cast::<u8>();
            state.allocation = Some((pointer, actual_len));
            (pointer, actual_len)
        } else {
            (std::ptr::null_mut(), 0)
        };
        unsafe {
            *output = pointer;
            *output_len = state.reported_length.unwrap_or(actual_len);
        }
        state.status
    })
}

unsafe extern "C" fn verify(
    receipt: *const u8,
    receipt_len: usize,
    image: *const u32,
    journal: *const u8,
    journal_len: usize,
) -> i32 {
    STUB.with(|slot| {
        let mut state = slot.borrow_mut();
        state.verify_calls += 1;
        state.receipt = unsafe { std::slice::from_raw_parts(receipt, receipt_len) }.to_vec();
        state
            .image
            .copy_from_slice(unsafe { std::slice::from_raw_parts(image, 8) });
        state.journal = unsafe { std::slice::from_raw_parts(journal, journal_len) }.to_vec();
        if state
            .expected
            .as_ref()
            .is_some_and(|(image, journal)| image != &state.image || journal != &state.journal)
        {
            return -4;
        }
        state.status
    })
}

unsafe extern "C" fn free(pointer: *mut u8, reported_length: usize) {
    STUB.with(|slot| {
        let mut state = slot.borrow_mut();
        state.free_calls += 1;
        state.freed_length = reported_length;
        if let Some((allocated, actual_len)) = state.allocation.take() {
            state.bad_free |= allocated != pointer;
            // The deliberately malformed stub may report an oversized length.
            // Free the actual allocation, while recording the exact ABI args.
            unsafe {
                drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(
                    allocated, actual_len,
                )));
            }
        } else {
            state.bad_free = true;
        }
    });
}

fn api() -> ReceiptApi {
    ReceiptApi {
        prove,
        verify,
        free,
    }
}

#[test]
fn receipt_limits_reject_invalid_policy_and_check_hard_abi_edges_without_allocating() {
    let limits = ReceiptLimits::default();
    assert!(limits.validate_prove(MAX_ELF_BYTES, MAX_BYTES).is_ok());
    assert!(limits.validate_prove(0, 0).is_err());
    assert!(limits.validate_prove(MAX_ELF_BYTES + 1, 0).is_err());
    assert!(limits.validate_prove(1, MAX_BYTES + 1).is_err());
    assert!(limits.validate_verify(MAGIC, MAX_BYTES).is_ok());
    assert!(limits.validate_verify(MAGIC, MAX_BYTES + 1).is_err());
    for invalid in [
        ReceiptLimits {
            elf_bytes: 0,
            ..limits
        },
        ReceiptLimits {
            elf_bytes: MAX_ELF_BYTES + 1,
            ..limits
        },
        ReceiptLimits {
            input_bytes: MAX_BYTES + 1,
            ..limits
        },
        ReceiptLimits {
            receipt_bytes: 7,
            ..limits
        },
        ReceiptLimits {
            receipt_bytes: MAX_BYTES + 1,
            ..limits
        },
        ReceiptLimits {
            journal_bytes: MAX_BYTES + 1,
            ..limits
        },
    ] {
        assert!(invalid.validate().is_err());
        // Invalid limits reject before path resolution or native loading.
        assert!(ReceiptSession::open(Path::new("not-a-receipt-library"), invalid).is_err());
    }
    assert!(ReceiptLimits {
        elf_bytes: 1,
        input_bytes: 0,
        receipt_bytes: 8,
        journal_bytes: 0,
    }
    .validate()
    .is_ok());
}

#[test]
fn receipt_abi_check_precedes_initialization_and_requires_success() {
    reset(Stub {
        abi: 2,
        ..Stub::default()
    });
    assert!(initialize(version, init).is_err());
    STUB.with(|slot| assert_eq!(slot.borrow().init_calls, 0));
    reset(Stub {
        abi: 1,
        status: -4,
        ..Stub::default()
    });
    assert!(initialize(version, init).is_err());
    STUB.with(|slot| assert_eq!(slot.borrow().init_calls, 1));
    reset(Stub {
        abi: 1,
        ..Stub::default()
    });
    initialize(version, init).unwrap();
    STUB.with(|slot| assert_eq!(slot.borrow().init_calls, 1));
}

#[test]
fn receipt_prove_forwards_inputs_copies_and_frees_without_claiming_verification() {
    let response = [MAGIC.as_slice(), b"opaque"].concat();
    reset(Stub {
        response: Some(response.clone()),
        ..Stub::default()
    });
    let image = [0x1234_5678; 8];
    let receipt = api()
        .prove(ReceiptLimits::default(), b"ELF", b"input", &image)
        .unwrap();
    assert_eq!(receipt, response);
    STUB.with(|slot| {
        let state = slot.borrow();
        assert_eq!(state.elf, b"ELF");
        assert_eq!(state.input, b"input");
        assert_eq!(state.image, image);
        assert!(state.output_slots_zeroed);
        assert_eq!(state.prove_calls, 1);
        assert_eq!(state.verify_calls, 0);
        assert_eq!(state.free_calls, 1);
        assert_eq!(state.freed_length, receipt.len());
        assert!(!state.bad_free);
        assert!(state.allocation.is_none());
    });
}

#[test]
fn receipt_producer_errors_and_malformed_buffers_free_exactly_once() {
    for (status, response, reported_length) in [
        (-4, Some(MAGIC.to_vec()), None),
        (-5, Some(MAGIC.to_vec()), None),
        (1, Some(MAGIC.to_vec()), None),
        (0, None, None),
        (0, None, Some(8)),
        (0, Some(MAGIC.to_vec()), Some(0)),
        (0, Some(MAGIC.to_vec()), Some(MAX_BYTES + 1)),
        (0, Some(b"notproof".to_vec()), None),
        (0, Some(b"AORCP00".to_vec()), None),
        (0, Some(b"AORCP001".to_vec()), None),
        (0, Some(b"AORCP003".to_vec()), None),
    ] {
        let allocated = response.is_some();
        let expected_length =
            reported_length.unwrap_or_else(|| response.as_ref().map_or(0, Vec::len));
        reset(Stub {
            status,
            response,
            reported_length,
            ..Stub::default()
        });
        let error = api()
            .prove(ReceiptLimits::default(), b"ELF", &[], &[0; 8])
            .unwrap_err();
        assert_eq!(
            error.downcast_ref::<ReceiptBackendUnavailable>().is_some(),
            status == -5
        );
        STUB.with(|slot| {
            let state = slot.borrow();
            assert_eq!(state.free_calls, usize::from(allocated));
            if allocated {
                assert_eq!(state.freed_length, expected_length);
            }
            assert!(state.allocation.is_none());
            assert!(!state.bad_free);
        });
    }
}

#[test]
fn receipt_policy_rejects_before_any_native_call() {
    reset(Stub::default());
    let limits = ReceiptLimits {
        elf_bytes: 1,
        input_bytes: 1,
        receipt_bytes: 8,
        journal_bytes: 1,
    };
    assert!(api().prove(limits, &[], &[], &[0; 8]).is_err());
    assert!(api().prove(limits, b"xx", &[], &[0; 8]).is_err());
    assert!(api().prove(limits, b"x", b"xx", &[0; 8]).is_err());
    for receipt in [b"".as_slice(), b"trace-digest", b"AORCP002x"] {
        assert!(api().verify(limits, receipt, &[0; 8], &[]).is_err());
    }
    assert!(api().verify(limits, MAGIC, &[0; 8], b"xx").is_err());
    STUB.with(|slot| {
        let state = slot.borrow();
        assert_eq!(state.prove_calls + state.verify_calls + state.free_calls, 0);
    });
}

#[test]
fn receipt_version_is_explicit_and_older_or_future_envelopes_never_reach_backend() {
    assert_eq!(MAGIC, b"AORCP002");
    reset(Stub::default());
    for envelope in [b"AORCP001", b"AORCP003"] {
        assert!(api()
            .verify(ReceiptLimits::default(), envelope, &[0; 8], &[])
            .is_err());
    }
    STUB.with(|slot| assert_eq!(slot.borrow().verify_calls, 0));
    // Structural forwarding only: this stub is not cryptographic proof evidence.
    api()
        .verify(ReceiptLimits::default(), b"AORCP002", &[0; 8], &[])
        .unwrap();
    STUB.with(|slot| assert_eq!(slot.borrow().verify_calls, 1));
}

#[test]
fn receipt_verification_requires_backend_and_exact_independent_pins() {
    let image = [17; 8];
    reset(Stub {
        expected: Some((image, b"independent statement".to_vec())),
        ..Stub::default()
    });
    let limits = ReceiptLimits::default();
    api()
        .verify(limits, MAGIC, &image, b"independent statement")
        .unwrap();
    assert!(api()
        .verify(limits, MAGIC, &[18; 8], b"independent statement")
        .is_err());
    assert!(api()
        .verify(limits, MAGIC, &image, b"independent statementx")
        .is_err());
    assert!(api().verify(limits, MAGIC, &image, &[]).is_err());
    STUB.with(|slot| {
        let state = slot.borrow();
        assert_eq!(state.verify_calls, 4);
        assert_eq!(state.receipt.as_slice(), MAGIC.as_slice());
        assert!(state.journal.is_empty());
        assert_eq!(state.free_calls, 0);
    });
    reset(Stub {
        expected: Some((image, Vec::new())),
        ..Stub::default()
    });
    api().verify(limits, MAGIC, &image, &[]).unwrap();
    STUB.with(|slot| assert_eq!(slot.borrow().verify_calls, 1));
    reset(Stub {
        status: -5,
        ..Stub::default()
    });
    let error = api().verify(limits, MAGIC, &image, &[]).unwrap_err();
    assert_eq!(
        error
            .downcast_ref::<ReceiptBackendUnavailable>()
            .unwrap()
            .operation(),
        "verify"
    );
    reset(Stub {
        status: -4,
        ..Stub::default()
    });
    assert!(api().verify(limits, MAGIC, &image, &[]).is_err());
}

#[test]
fn receipt_allocation_guard_releases_on_unwind_and_session_can_move_to_owner() {
    fn movable<T: Send>() {}
    movable::<ReceiptSession>();
    reset(Stub::default());
    let allocation = MAGIC.to_vec().into_boxed_slice();
    let length = allocation.len();
    let pointer = Box::into_raw(allocation).cast::<u8>();
    STUB.with(|slot| slot.borrow_mut().allocation = Some((pointer, length)));
    let result = std::panic::catch_unwind(|| {
        let _guard = ReceiptBuffer {
            pointer,
            length,
            free,
        };
        panic!("injected owned-copy failure");
    });
    assert!(result.is_err());
    STUB.with(|slot| {
        let state = slot.borrow();
        assert_eq!(state.free_calls, 1);
        assert!(!state.bad_free);
        assert!(state.allocation.is_none());
    });
}
