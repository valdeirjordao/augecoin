//! License domain: key, model, repository and service.

mod key;
mod model;
mod repo;
mod service;

pub use key::{KeyError, LicenseKey};
pub use model::{License, LicenseStatus, LicenseView, Plan};
pub use repo::{LicenseRepo, NewLicense};
pub use service::{IssuedLicense, LicenseService, Validation};
