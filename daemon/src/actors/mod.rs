//! The actor split from docs/architecture.md, as real types instead of
//! prose: a Control actor that ingests artifacts into Scene Memory, and a
//! Presence actor that owns live presences and the (unbuilt) text-to-motion
//! mapping. `registry.rs` is the thin thing that wires these to the socket
//! protocol; the actual behavior lives here.

pub mod control;
pub mod presence;
pub mod scene_memory;

pub use control::ControlActor;
pub use presence::PresenceActor;
pub use scene_memory::SceneMemory;
