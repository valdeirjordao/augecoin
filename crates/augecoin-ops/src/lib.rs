//! AUGECOIN operational platform backend.
//!
//! This crate is the bridge between the AUGECOIN blockchain and the validator
//! SaaS layer. It owns licensing, activation, heartbeat/monitoring and the
//! administrative APIs consumed by `operacional.augeco.in`. It holds **no** keys
//! and **no** balances: consensus authority stays on-chain, and the only way a
//! validator enters the set is through an admin-signed `ValidatorAdminOp`
//! submitted through the node RPC (never through this service directly).

pub mod audit;
pub mod config;
pub mod db;
pub mod error;
pub mod http;
pub mod license;
pub mod monitor;
pub mod node;
pub mod release;
pub mod reward;
pub mod serde_util;
pub mod state;
pub mod validator;

pub use error::{AppError, Result};
