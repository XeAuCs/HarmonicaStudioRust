//! Core contracts, grouped by domain under one integration-test target.
#[path = "core_contracts/export.rs"]
mod export;
#[path = "core_contracts/library.rs"]
mod library;
#[path = "core_contracts/melody.rs"]
mod melody;
#[path = "core_contracts/midi.rs"]
mod midi;
#[path = "core_contracts/project.rs"]
mod project;
#[path = "core_contracts/support.rs"]
mod support;
#[path = "core_contracts/transport.rs"]
mod transport;
