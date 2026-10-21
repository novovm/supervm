#![forbid(unsafe_code)]

#[cfg(all(test, feature = "iroh-transport"))]
pub(crate) static SERVICE_TEST_SERIAL: tokio::sync::Semaphore =
    tokio::sync::Semaphore::const_new(1);
pub mod adaptive_overlay;
pub mod availability;
#[cfg(any(feature = "iroh-transport", feature = "tls-transport"))]
pub mod bound_session;
pub mod capability;
#[cfg(any(feature = "iroh-transport", feature = "tls-transport"))]
pub mod carrier_stream;
pub mod control_plane;
#[cfg(feature = "iroh-transport")]
pub mod discovery_control;
pub mod duplex;
#[cfg(feature = "iroh-transport")]
pub mod endpoint_delivery;
pub mod eth_chain_config;
pub mod eth_fullnode;
pub mod eth_rlpx;
pub mod eth_runtime_config;
pub mod eth_selection_config;
pub mod gossip;
#[cfg(feature = "iroh-transport")]
pub mod iroh_transport;
pub mod mailbox_receipt;
pub mod mailbox_replicas;
#[cfg(feature = "iroh-transport")]
pub mod mailbox_rpc;
#[cfg(feature = "mailbox-seal")]
pub mod mailbox_seal;
pub mod mailbox_store;
#[cfg(feature = "iroh-transport")]
pub mod mailbox_structured;
pub mod network_time;
pub mod novorudp;
pub mod opaque_rendezvous;
pub mod overlay;
pub mod overlay_runtime;
#[cfg(feature = "iroh-transport")]
pub mod product_bootstrap_refresh;
pub mod product_directory;
pub mod product_identity;
pub mod product_nat;
pub mod product_overlay;
pub mod product_relay;
pub mod product_relay_launch;
#[cfg(feature = "iroh-transport")]
pub mod product_rendezvous_daemon;
#[cfg(feature = "iroh-transport")]
pub mod product_rendezvous_owned;
#[cfg(feature = "iroh-transport")]
pub mod product_service_lease;
pub mod reachability;
pub mod relay;
pub mod route;
pub mod routing;
pub mod runtime_status;
#[cfg(feature = "iroh-transport")]
pub mod service_concurrency;
#[cfg(feature = "tls-transport")]
pub mod tls_transport;
pub mod transport;
#[cfg(any(feature = "iroh-transport", feature = "tls-transport"))]
pub mod transport_binding;

pub use adaptive_overlay::*;
pub use availability::*;
pub use capability::*;
pub use control_plane::*;
pub use eth_chain_config::*;
pub use eth_fullnode::*;
pub use eth_rlpx::*;
pub use eth_runtime_config::*;
pub use eth_selection_config::*;
pub use gossip::*;
pub use novorudp::*;
pub use overlay::*;
pub use overlay_runtime::*;
pub use product_directory::*;
pub use product_identity::*;
pub use product_nat::*;
pub use product_overlay::*;
pub use product_relay::*;
pub use reachability::*;
pub use relay::*;
pub use route::*;
pub use routing::*;
pub use runtime_status::*;
pub use transport::*;
