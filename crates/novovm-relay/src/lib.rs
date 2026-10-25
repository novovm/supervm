//! Shared local custody primitives used by the original owners and node tools.
//! Importing this library never constructs a service owner or starts a listener.

#[cfg(target_os = "linux")]
pub mod linux_custody;
#[cfg(target_os = "linux")]
pub mod linux_owner_files;
#[cfg(windows)]
pub mod windows_custody;
#[cfg(windows)]
pub mod windows_owner_files;

pub mod purpose_keys;
