//! Validator domain: model, repository and service.

mod model;
mod repo;
mod service;

pub use model::{
    is_hex64, validate_machine_id, validate_public_key, Validator, ValidatorStatus, ValidatorView,
};
pub use repo::{HeartbeatUpdate, NewValidator, ValidatorRepo};
pub use service::{
    ActivateInput, ActivationConfig, ActivationResponse, HeartbeatInput, HeartbeatResponse,
    ValidatorService, ValidatorStats, HEARTBEAT_INTERVAL_SECS, ONLINE_WINDOW_SECS,
};
