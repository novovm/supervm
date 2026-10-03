//! Alias the original product ABI layouts; do not define a second graph ABI.
pub(super) use crate::{
    AoemAbiVersion as AbiVersion, AoemCancelSemanticGraphV2 as Cancel,
    AoemCreateOptionsV1 as CreateOptions, AoemCreateWithOptions as Create, AoemDestroy as Destroy,
    AoemGlobalInit as GlobalInit, AoemGraphCallbacksV2 as Callbacks,
    AoemGraphCompletionV2 as Completion, AoemGraphSubmitOptionsV2 as SubmitOptions,
    AoemLastError as LastError, AoemSemanticGraphV2ActiveCount as ActiveCount,
    AoemStateEventV2 as StateEvent, AoemSubmitSemanticGraphV2 as Submit,
    AoemTaskDescriptorV2 as TaskDescriptor, AoemTaskStepOutputV2 as StepOutput,
    AOEM_ERROR_CALLBACK_PANICKED as CALLBACK_PANICKED, AOEM_ERROR_GRAPH_FAULTED as GRAPH_FAULTED,
    AOEM_ERROR_INVALID_ARGUMENT as INVALID_ARGUMENT, AOEM_SEMANTIC_GRAPH_ABI_V2 as GRAPH_ABI,
    AOEM_STATUS_OK as OK,
};

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::c_void;
    use std::mem::{offset_of, size_of};

    #[test]
    fn resident_layouts_reuse_the_packaged_product_abi() {
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
