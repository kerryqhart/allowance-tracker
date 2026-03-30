//! Schema drift validation test.
//!
//! Compares the DynamoDB table schemas defined in the SAM template
//! (infrastructure/template.yaml) against what create_all_tables produces
//! on DynamoDB Local. If they disagree, this test fails.

mod common;

use common::{DYNAMO_LOCAL_PORT, is_dynamo_local_available};
use sync_service::storage::table_definitions;
use std::collections::HashMap;

/// A simplified representation of a DynamoDB table schema for comparison.
#[derive(Debug, PartialEq)]
struct TableSchema {
    key_schema: Vec<(String, String)>,       // (attribute_name, key_type) e.g. ("child_id", "HASH")
    attribute_defs: Vec<(String, String)>,    // (attribute_name, attribute_type) e.g. ("child_id", "S")
}

/// Parse all DynamoDB table schemas from the SAM template YAML.
/// Returns a map of normalized_table_name -> TableSchema.
fn parse_sam_template() -> HashMap<String, TableSchema> {
    let template_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("infrastructure")
        .join("template.yaml");

    let content = std::fs::read_to_string(&template_path)
        .unwrap_or_else(|e| panic!("Failed to read SAM template at {:?}: {}", template_path, e));

    let doc: serde_yaml::Value = serde_yaml::from_str(&content)
        .unwrap_or_else(|e| panic!("Failed to parse SAM template YAML: {}", e));

    let resources = doc.get("Resources")
        .expect("SAM template missing Resources section");

    let mut schemas = HashMap::new();

    if let serde_yaml::Value::Mapping(resources_map) = resources {
        for (_resource_name, resource) in resources_map {
            let type_val = resource.get("Type")
                .and_then(|v| v.as_str());

            if type_val != Some("AWS::DynamoDB::Table") {
                continue;
            }

            let props = resource.get("Properties").expect("DynamoDB table missing Properties");

            // Get table name and normalize: "allowance-tracker-sync-events" -> "sync_events"
            let table_name = props.get("TableName")
                .and_then(|v| v.as_str())
                .expect("DynamoDB table missing TableName");
            let normalized = table_name
                .strip_prefix("allowance-tracker-")
                .unwrap_or(table_name)
                .replace('-', "_");

            // Parse KeySchema
            let key_schema_val = props.get("KeySchema")
                .expect("DynamoDB table missing KeySchema");
            let mut key_schema = Vec::new();
            if let serde_yaml::Value::Sequence(keys) = key_schema_val {
                for key in keys {
                    let attr_name = key.get("AttributeName")
                        .and_then(|v| v.as_str())
                        .expect("KeySchema missing AttributeName")
                        .to_string();
                    let key_type = key.get("KeyType")
                        .and_then(|v| v.as_str())
                        .expect("KeySchema missing KeyType")
                        .to_string();
                    key_schema.push((attr_name, key_type));
                }
            }

            // Parse AttributeDefinitions
            let attr_defs_val = props.get("AttributeDefinitions")
                .expect("DynamoDB table missing AttributeDefinitions");
            let mut attribute_defs = Vec::new();
            if let serde_yaml::Value::Sequence(attrs) = attr_defs_val {
                for attr in attrs {
                    let attr_name = attr.get("AttributeName")
                        .and_then(|v| v.as_str())
                        .expect("AttributeDefinition missing AttributeName")
                        .to_string();
                    let attr_type = attr.get("AttributeType")
                        .and_then(|v| v.as_str())
                        .expect("AttributeDefinition missing AttributeType")
                        .to_string();
                    attribute_defs.push((attr_name, attr_type));
                }
            }

            // Sort for stable comparison
            key_schema.sort();
            attribute_defs.sort();

            schemas.insert(normalized, TableSchema { key_schema, attribute_defs });
        }
    }

    schemas
}

/// Query DynamoDB Local for the actual table schemas created by create_all_tables.
/// Returns a map of table_base_name -> TableSchema.
async fn get_dynamo_local_schemas(client: &aws_sdk_dynamodb::Client, prefix: &str) -> HashMap<String, TableSchema> {
    let table_bases = ["children", "transactions", "goals", "sync_events", "sync_metadata"];
    let mut schemas = HashMap::new();

    for base in &table_bases {
        let table_name = format!("{}{}", prefix, base);
        let describe = client
            .describe_table()
            .table_name(&table_name)
            .send()
            .await
            .unwrap_or_else(|e| panic!("Failed to describe table {}: {}", table_name, e));

        let table_desc = describe.table().expect("No table description returned");

        let mut key_schema: Vec<(String, String)> = table_desc
            .key_schema()
            .iter()
            .map(|ks| {
                (
                    ks.attribute_name().to_string(),
                    format!("{:?}", ks.key_type()),  // "Hash" or "Range"
                )
            })
            .collect();

        let mut attribute_defs: Vec<(String, String)> = table_desc
            .attribute_definitions()
            .iter()
            .map(|ad| {
                (
                    ad.attribute_name().to_string(),
                    format!("{:?}", ad.attribute_type()),  // "S" or "N"
                )
            })
            .collect();

        // Normalize key_type format: SDK returns "Hash"/"Range", SAM uses "HASH"/"RANGE"
        for (_, kt) in &mut key_schema {
            *kt = kt.to_uppercase();
        }

        // Normalize attribute_type format: SDK returns "S"/"N" already, just uppercase for safety
        for (_, at) in &mut attribute_defs {
            *at = at.to_uppercase();
        }

        key_schema.sort();
        attribute_defs.sort();

        schemas.insert(base.to_string(), TableSchema { key_schema, attribute_defs });
    }

    schemas
}

#[tokio::test]
async fn test_sam_template_matches_create_all_tables() {
    if !is_dynamo_local_available(DYNAMO_LOCAL_PORT).await {
        eprintln!("SKIPPING: DynamoDB Local not available on port {}", DYNAMO_LOCAL_PORT);
        return;
    }

    // Source 1: Parse SAM template
    let sam_schemas = parse_sam_template();
    assert_eq!(sam_schemas.len(), 5, "SAM template should define exactly 5 DynamoDB tables, found {}", sam_schemas.len());

    // Source 2: Create tables on DynamoDB Local and describe them
    let client = sync_service::create_local_dynamo_client(DYNAMO_LOCAL_PORT).await.unwrap();
    let prefix = format!("drift_test_{}_", uuid::Uuid::new_v4().to_string().replace('-', "")[..8].to_string());

    table_definitions::create_all_tables(&client, &prefix).await.unwrap();
    let dynamo_schemas = get_dynamo_local_schemas(&client, &prefix).await;

    // Cleanup
    table_definitions::delete_all_tables(&client, &prefix).await.unwrap();

    // Compare
    assert_eq!(sam_schemas.len(), dynamo_schemas.len(),
        "SAM defines {} tables but create_all_tables creates {}",
        sam_schemas.len(), dynamo_schemas.len());

    for (table_name, sam_schema) in &sam_schemas {
        let dynamo_schema = dynamo_schemas.get(table_name)
            .unwrap_or_else(|| panic!(
                "SAM template defines table '{}' but create_all_tables does not create it. \
                 SAM tables: {:?}, code tables: {:?}",
                table_name,
                sam_schemas.keys().collect::<Vec<_>>(),
                dynamo_schemas.keys().collect::<Vec<_>>()
            ));

        assert_eq!(
            sam_schema.key_schema, dynamo_schema.key_schema,
            "Key schema mismatch for table '{}':\n  SAM:  {:?}\n  Code: {:?}",
            table_name, sam_schema.key_schema, dynamo_schema.key_schema
        );

        assert_eq!(
            sam_schema.attribute_defs, dynamo_schema.attribute_defs,
            "Attribute definitions mismatch for table '{}':\n  SAM:  {:?}\n  Code: {:?}",
            table_name, sam_schema.attribute_defs, dynamo_schema.attribute_defs
        );
    }
}
