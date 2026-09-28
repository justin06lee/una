//! una-core: platform-agnostic core of the una desktop dictation client.
//!
//! Contains the dictation state machine and its controller actor, audio
//! capture/resampling, the server API client, mDNS discovery, configuration,
//! the WAV spool, the unix IPC socket, and the yagami client behind "Fix up". No GUI or Objective-C dependencies.

pub mod audio;
pub mod config;
pub mod correction;
pub mod discovery;
pub mod endpoint;
#[cfg(unix)]
pub mod ipc;
pub mod net;
pub mod spool;
pub mod state;
pub mod yagami;

pub use endpoint::EndpointResolver;
pub use state::{Command, Controller, ControllerHandle, Effect, Event, Machine, Snapshot, State};
