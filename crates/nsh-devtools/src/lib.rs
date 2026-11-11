//! NSH DevTools: the protocol of an app's in-process debugging endpoint.
//!
//! A running app listens on [`PATH`] (a WebSocket) and answers JSON requests
//! named `Domain.method` ([`protocol`]). The methods are the vocabulary
//! `nsh test --target` already drives hosts with (`nsh_dev::protocol::TestStep`),
//! renamed ([`methods`]); the protocol describes itself ([`description`]); the
//! app's log lines are kept for `Console.*` ([`console`]).

#[cfg(feature = "client")]
pub mod client;
pub mod console;
pub mod description;
pub mod methods;
pub mod protocol;
#[cfg(feature = "server")]
pub mod server;
pub mod tree;

/// Version of the DevTools protocol; bumped on incompatible changes.
pub const PROTOCOL_VERSION: u32 = 1;

/// WebSocket endpoint path.
pub const PATH: &str = "/devtools";

/// Environment variable that turns the endpoint on: the address to listen on.
pub const ENV_LISTEN: &str = "NSH_DEVTOOLS";

/// Environment variable holding the token clients must present.
pub const ENV_TOKEN: &str = "NSH_DEVTOOLS_TOKEN";

/// Environment variable naming the file the listening port is written to.
pub const ENV_PORT_FILE: &str = "NSH_DEVTOOLS_PORT_FILE";

/// Environment variable that makes the app wait for a client before it runs.
pub const ENV_WAIT: &str = "NSH_DEVTOOLS_WAIT";
