use schemars::JsonSchema;
use serde::Serialize;
use serde_json::Value;

use crate::apply::{ProgressEvent, Receipt, VerificationReport};
use crate::capability::CapabilityReport;
use crate::error::RecoveryAction;
use crate::model::InspectionReport;
use crate::plan::{Plan, Recipe};

#[derive(Debug, Clone, Copy)]
pub enum Document {
    Recipe,
    Plan,
    Inspection,
    Progress,
    Verification,
    Receipt,
    Error,
    Capability,
}

pub fn document(document: Document) -> Result<Value, serde_json::Error> {
    match document {
        Document::Recipe => serde_json::to_value(schemars::schema_for!(Recipe)),
        Document::Plan => serde_json::to_value(schemars::schema_for!(Plan)),
        Document::Inspection => serde_json::to_value(schemars::schema_for!(InspectionReport)),
        Document::Progress => serde_json::to_value(schemars::schema_for!(ProgressEvent)),
        Document::Verification => serde_json::to_value(schemars::schema_for!(VerificationReport)),
        Document::Receipt => serde_json::to_value(schemars::schema_for!(Receipt)),
        Document::Error => serde_json::to_value(schemars::schema_for!(ErrorSchemaDocument)),
        Document::Capability => serde_json::to_value(schemars::schema_for!(CapabilityReport)),
    }
}

pub fn catalog() -> Value {
    serde_json::json!({
        "schema_version": "avpact.schema-catalog/v0.1",
        "documents": [
            {
                "name": "recipe",
                "schema_version": crate::RECIPE_SCHEMA_VERSION,
                "direction": "input"
            },
            {
                "name": "plan",
                "schema_version": crate::PLAN_SCHEMA_VERSION,
                "direction": "input_output"
            },
            {
                "name": "inspection",
                "schema_version": crate::INSPECTION_SCHEMA_VERSION,
                "direction": "output"
            },
            {
                "name": "progress",
                "schema_version": crate::PROGRESS_SCHEMA_VERSION,
                "direction": "output"
            },
            {
                "name": "verification",
                "schema_version": crate::VERIFICATION_SCHEMA_VERSION,
                "direction": "output"
            },
            {
                "name": "receipt",
                "schema_version": crate::RECEIPT_SCHEMA_VERSION,
                "direction": "output"
            },
            {
                "name": "error",
                "schema_version": crate::ERROR_SCHEMA_VERSION,
                "direction": "output"
            },
            {
                "name": "capability",
                "schema_version": crate::CAPABILITY_SCHEMA_VERSION,
                "direction": "output"
            }
        ]
    })
}

#[derive(JsonSchema, Serialize)]
struct ErrorSchemaDocument {
    schema_version: String,
    error: ErrorSchemaBody,
}

#[derive(JsonSchema, Serialize)]
struct ErrorSchemaBody {
    code: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    diagnostic: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    recovery: Option<ErrorRecoverySchema>,
}

#[derive(JsonSchema, Serialize)]
struct ErrorRecoverySchema {
    action: RecoveryAction,
    output: String,
    output_sha256: String,
    requested_receipt: String,
    recovery_receipt: String,
    recovery_receipt_persisted: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_document_schema_is_serializable() {
        for document_kind in [
            Document::Recipe,
            Document::Plan,
            Document::Inspection,
            Document::Progress,
            Document::Verification,
            Document::Receipt,
            Document::Error,
            Document::Capability,
        ] {
            let schema = document(document_kind).expect("serialize schema");
            assert_eq!(
                schema.get("$schema").and_then(Value::as_str),
                Some("https://json-schema.org/draft/2020-12/schema")
            );
        }
    }

    #[test]
    fn error_schema_declares_the_receipt_recovery_action() {
        let schema = document(Document::Error).expect("serialize error schema");
        let serialized = serde_json::to_string(&schema).expect("render error schema");
        assert!(serialized.contains("do_not_retry_apply"));
        assert!(serialized.contains("recovery_receipt_persisted"));
    }
}
