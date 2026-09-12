//! Release (OTA) domain: model, repository and service.

mod model;
mod repo;
mod service;

pub use model::{validate_artifact_hash, validate_signature, Platform, Release, ReleaseView};
pub use repo::{NewRelease, ReleaseRepo};
pub use service::ReleaseService;
