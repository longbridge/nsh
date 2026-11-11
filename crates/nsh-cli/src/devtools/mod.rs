//! `nsh test --target macos` over DevTools: the launcher starts the host
//! with an endpoint of its own and connects to it (`launch.rs`); the host
//! the tests drive is that connection (`host.rs`). `nsh devtools` (`cli.rs`)
//! and its MCP server (`mcp.rs`) keep such a host open for clients.

mod artifacts;
mod cli;
mod host;
mod launch;
mod mcp;
mod timeline;

pub(crate) use artifacts::Artifacts;
pub(crate) use cli::{DevtoolsArgs, run, wait_ready};
pub(crate) use host::DevtoolsHost;
pub(crate) use launch::{LaunchSpec, launch, token};
