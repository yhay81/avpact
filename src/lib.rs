pub mod apply;
pub mod capability;
pub mod error;
mod hex;
pub mod inspect;
pub mod model;
pub mod plan;
pub mod schema;
mod strict_json;

pub const ERROR_SCHEMA_VERSION: &str = "avpact.error/v0.1";
pub const CAPABILITY_SCHEMA_VERSION: &str = "avpact.capability/v0.1";
pub const INSPECTION_SCHEMA_VERSION: &str = "avpact.inspection/v0.1";
pub const PLAN_SCHEMA_VERSION: &str = "avpact.plan/v0.1";
pub const PROGRESS_SCHEMA_VERSION: &str = "avpact.progress/v0.1";
pub const RECIPE_SCHEMA_VERSION: &str = "avpact.recipe/v0.1";
pub const LEGACY_RECEIPT_SCHEMA_VERSION: &str = "avpact.receipt/v0.1";
pub const RECEIPT_SCHEMA_VERSION: &str = "avpact.receipt/v0.2";
pub const VERIFICATION_SCHEMA_VERSION: &str = "avpact.verification/v0.1";
