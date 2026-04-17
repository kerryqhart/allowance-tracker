use aws_sdk_dynamodb::Client;
use aws_sdk_dynamodb::types::{
    AttributeDefinition, GlobalSecondaryIndex, KeySchemaElement, Projection,
    ProjectionType, ScalarAttributeType,
};

const CHILDREN_TABLE: &str = "children";
const TRANSACTIONS_TABLE: &str = "transactions";
const GOALS_TABLE: &str = "goals";
const SYNC_EVENTS_TABLE: &str = "sync_events";
const SYNC_METADATA_TABLE: &str = "sync_metadata";

/// Create all necessary DynamoDB tables for the sync service.
///
/// Creates 5 tables:
/// - children: PK: child_id
/// - transactions: PK: child_id, SK: transaction_id
/// - goals: PK: child_id, SK: goal_id
/// - sync_events: PK: child_id, SK: sequence (Number)
/// - sync_metadata: PK: child_id
///
/// All tables use ProvisionedThroughput(5, 5).
pub async fn create_all_tables(client: &Client, prefix: &str) -> anyhow::Result<()> {
    create_table_if_not_exists(
        client,
        &format!("{}{}", prefix, CHILDREN_TABLE),
        vec![
            KeySchemaElement::builder()
                .attribute_name("child_id")
                .key_type(aws_sdk_dynamodb::types::KeyType::Hash)
                .build()
                .map_err(|e| anyhow::anyhow!("Failed to build KeySchemaElement: {}", e))?,
        ],
        vec![
            AttributeDefinition::builder()
                .attribute_name("child_id")
                .attribute_type(ScalarAttributeType::S)
                .build()
                .map_err(|e| anyhow::anyhow!("Failed to build AttributeDefinition: {}", e))?,
        ],
    )
    .await?;

    // Build DateSortIndex GSI
    let date_sort_index = GlobalSecondaryIndex::builder()
        .index_name("DateSortIndex")
        .key_schema(
            KeySchemaElement::builder()
                .attribute_name("child_id")
                .key_type(aws_sdk_dynamodb::types::KeyType::Hash)
                .build()
                .map_err(|e| anyhow::anyhow!("Failed to build KeySchemaElement: {}", e))?,
        )
        .key_schema(
            KeySchemaElement::builder()
                .attribute_name("sort_date")
                .key_type(aws_sdk_dynamodb::types::KeyType::Range)
                .build()
                .map_err(|e| anyhow::anyhow!("Failed to build KeySchemaElement: {}", e))?,
        )
        .projection(
            Projection::builder()
                .projection_type(ProjectionType::All)
                .build(),
        )
        .provisioned_throughput(
            aws_sdk_dynamodb::types::ProvisionedThroughput::builder()
                .read_capacity_units(5)
                .write_capacity_units(5)
                .build()
                .map_err(|e| anyhow::anyhow!("Failed to build ProvisionedThroughput: {}", e))?,
        )
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to build GlobalSecondaryIndex: {}", e))?;

    create_table_with_gsi(
        client,
        &format!("{}{}", prefix, TRANSACTIONS_TABLE),
        vec![
            KeySchemaElement::builder()
                .attribute_name("child_id")
                .key_type(aws_sdk_dynamodb::types::KeyType::Hash)
                .build()
                .map_err(|e| anyhow::anyhow!("Failed to build KeySchemaElement: {}", e))?,
            KeySchemaElement::builder()
                .attribute_name("transaction_id")
                .key_type(aws_sdk_dynamodb::types::KeyType::Range)
                .build()
                .map_err(|e| anyhow::anyhow!("Failed to build KeySchemaElement: {}", e))?,
        ],
        vec![
            AttributeDefinition::builder()
                .attribute_name("child_id")
                .attribute_type(ScalarAttributeType::S)
                .build()
                .map_err(|e| anyhow::anyhow!("Failed to build AttributeDefinition: {}", e))?,
            AttributeDefinition::builder()
                .attribute_name("transaction_id")
                .attribute_type(ScalarAttributeType::S)
                .build()
                .map_err(|e| anyhow::anyhow!("Failed to build AttributeDefinition: {}", e))?,
            AttributeDefinition::builder()
                .attribute_name("sort_date")
                .attribute_type(ScalarAttributeType::S)
                .build()
                .map_err(|e| anyhow::anyhow!("Failed to build AttributeDefinition: {}", e))?,
        ],
        date_sort_index,
    )
    .await?;

    create_table_if_not_exists(
        client,
        &format!("{}{}", prefix, GOALS_TABLE),
        vec![
            KeySchemaElement::builder()
                .attribute_name("child_id")
                .key_type(aws_sdk_dynamodb::types::KeyType::Hash)
                .build()
                .map_err(|e| anyhow::anyhow!("Failed to build KeySchemaElement: {}", e))?,
            KeySchemaElement::builder()
                .attribute_name("goal_id")
                .key_type(aws_sdk_dynamodb::types::KeyType::Range)
                .build()
                .map_err(|e| anyhow::anyhow!("Failed to build KeySchemaElement: {}", e))?,
        ],
        vec![
            AttributeDefinition::builder()
                .attribute_name("child_id")
                .attribute_type(ScalarAttributeType::S)
                .build()
                .map_err(|e| anyhow::anyhow!("Failed to build AttributeDefinition: {}", e))?,
            AttributeDefinition::builder()
                .attribute_name("goal_id")
                .attribute_type(ScalarAttributeType::S)
                .build()
                .map_err(|e| anyhow::anyhow!("Failed to build AttributeDefinition: {}", e))?,
        ],
    )
    .await?;

    create_table_if_not_exists(
        client,
        &format!("{}{}", prefix, SYNC_EVENTS_TABLE),
        vec![
            KeySchemaElement::builder()
                .attribute_name("child_id")
                .key_type(aws_sdk_dynamodb::types::KeyType::Hash)
                .build()
                .map_err(|e| anyhow::anyhow!("Failed to build KeySchemaElement: {}", e))?,
            KeySchemaElement::builder()
                .attribute_name("sequence")
                .key_type(aws_sdk_dynamodb::types::KeyType::Range)
                .build()
                .map_err(|e| anyhow::anyhow!("Failed to build KeySchemaElement: {}", e))?,
        ],
        vec![
            AttributeDefinition::builder()
                .attribute_name("child_id")
                .attribute_type(ScalarAttributeType::S)
                .build()
                .map_err(|e| anyhow::anyhow!("Failed to build AttributeDefinition: {}", e))?,
            AttributeDefinition::builder()
                .attribute_name("sequence")
                .attribute_type(ScalarAttributeType::N)
                .build()
                .map_err(|e| anyhow::anyhow!("Failed to build AttributeDefinition: {}", e))?,
        ],
    )
    .await?;

    create_table_if_not_exists(
        client,
        &format!("{}{}", prefix, SYNC_METADATA_TABLE),
        vec![
            KeySchemaElement::builder()
                .attribute_name("child_id")
                .key_type(aws_sdk_dynamodb::types::KeyType::Hash)
                .build()
                .map_err(|e| anyhow::anyhow!("Failed to build KeySchemaElement: {}", e))?,
        ],
        vec![
            AttributeDefinition::builder()
                .attribute_name("child_id")
                .attribute_type(ScalarAttributeType::S)
                .build()
                .map_err(|e| anyhow::anyhow!("Failed to build AttributeDefinition: {}", e))?,
        ],
    )
    .await?;

    Ok(())
}

/// Create a single DynamoDB table if it doesn't already exist.
///
/// Handles ResourceInUseException gracefully by returning success if the table
/// already exists.
async fn create_table_if_not_exists(
    client: &Client,
    table_name: &str,
    key_schema: Vec<KeySchemaElement>,
    attributes: Vec<AttributeDefinition>,
) -> anyhow::Result<()> {
    let mut create_table_builder = client
        .create_table()
        .table_name(table_name)
        .billing_mode(aws_sdk_dynamodb::types::BillingMode::Provisioned)
        .provisioned_throughput(
            aws_sdk_dynamodb::types::ProvisionedThroughput::builder()
                .read_capacity_units(5)
                .write_capacity_units(5)
                .build()
                .map_err(|e| anyhow::anyhow!("Failed to build ProvisionedThroughput: {}", e))?,
        );

    for ks in key_schema {
        create_table_builder = create_table_builder.key_schema(ks);
    }

    for attr in attributes {
        create_table_builder = create_table_builder.attribute_definitions(attr);
    }

    let result = create_table_builder.send().await;

    match result {
        Ok(_) => Ok(()),
        Err(e) => {
            // Check if the error is ResourceInUseException (table already exists)
            if let Some(service_err) = e.as_service_error() {
                if service_err.is_resource_in_use_exception() {
                    // Table already exists, which is fine
                    return Ok(());
                }
            }
            Err(anyhow::anyhow!("Failed to create table {}: {}", table_name, e))
        }
    }
}

/// Create a single DynamoDB table with a GSI if it doesn't already exist.
///
/// Similar to `create_table_if_not_exists` but also accepts a GlobalSecondaryIndex.
/// Handles ResourceInUseException gracefully by returning success if the table
/// already exists.
async fn create_table_with_gsi(
    client: &Client,
    table_name: &str,
    key_schema: Vec<KeySchemaElement>,
    attributes: Vec<AttributeDefinition>,
    gsi: GlobalSecondaryIndex,
) -> anyhow::Result<()> {
    let mut create_table_builder = client
        .create_table()
        .table_name(table_name)
        .billing_mode(aws_sdk_dynamodb::types::BillingMode::Provisioned)
        .provisioned_throughput(
            aws_sdk_dynamodb::types::ProvisionedThroughput::builder()
                .read_capacity_units(5)
                .write_capacity_units(5)
                .build()
                .map_err(|e| anyhow::anyhow!("Failed to build ProvisionedThroughput: {}", e))?,
        );

    for ks in key_schema {
        create_table_builder = create_table_builder.key_schema(ks);
    }

    for attr in attributes {
        create_table_builder = create_table_builder.attribute_definitions(attr);
    }

    create_table_builder = create_table_builder.global_secondary_indexes(gsi);

    let result = create_table_builder.send().await;

    match result {
        Ok(_) => Ok(()),
        Err(e) => {
            // Check if the error is ResourceInUseException (table already exists)
            if let Some(service_err) = e.as_service_error() {
                if service_err.is_resource_in_use_exception() {
                    // Table already exists, which is fine
                    return Ok(());
                }
            }
            Err(anyhow::anyhow!("Failed to create table {}: {}", table_name, e))
        }
    }
}

/// Delete all tables created by `create_all_tables`.
///
/// Used for test cleanup. Gracefully handles tables that don't exist.
pub async fn delete_all_tables(client: &Client, prefix: &str) -> anyhow::Result<()> {
    let tables = vec![
        CHILDREN_TABLE,
        TRANSACTIONS_TABLE,
        GOALS_TABLE,
        SYNC_EVENTS_TABLE,
        SYNC_METADATA_TABLE,
    ];

    for table in tables {
        let table_name = format!("{}{}", prefix, table);
        let result = client.delete_table().table_name(&table_name).send().await;

        match result {
            Ok(_) => {}
            Err(e) => {
                // Check if the error is ResourceNotFoundException (table doesn't exist)
                if let Some(service_err) = e.as_service_error() {
                    if service_err.is_resource_not_found_exception() {
                        // Table doesn't exist, which is fine
                        continue;
                    }
                }
                // Log the error but don't fail cleanup if a table can't be deleted
                log::warn!("Failed to delete table {}: {}", table_name, e);
            }
        }
    }

    Ok(())
}
