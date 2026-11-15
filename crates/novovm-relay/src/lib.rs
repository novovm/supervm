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

#[cfg(feature = "iroh-transport")]
pub mod asas_route;
#[cfg(feature = "iroh-transport")]
pub mod asas_client;
pub mod purpose_keys;
pub mod transport_identity;
