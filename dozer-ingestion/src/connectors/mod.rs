pub mod ethereum;
pub mod grpc;
pub mod kafka;
pub mod object_store;
pub mod postgres;

use crate::connectors::postgres::connection::helper::map_connection_config;
use std::collections::HashMap;
use std::fmt::Debug;

use crate::connectors::kafka::connector::KafkaConnector;
use crate::connectors::postgres::connector::{PostgresConfig, PostgresConnector};
use crate::errors::ConnectorError;
use crate::ingestion::Ingestor;
use dozer_types::log::debug;
use dozer_types::models::connection::Connection;
use dozer_types::models::connection::ConnectionConfig;

use crate::connectors::object_store::connector::ObjectStoreConnector;
use dozer_types::prettytable::Table;
use dozer_types::serde;
use dozer_types::serde::{Deserialize, Serialize};
use dozer_types::types::SourceSchema;

pub mod snowflake;

use self::ethereum::{EthLogConnector, EthTraceConnector};
use self::grpc::connector::GrpcConnector;
use crate::connectors::snowflake::connector::SnowflakeConnector;

pub type ValidationResults = HashMap<String, Vec<(Option<String>, Result<(), ConnectorError>)>>;

pub trait Connector: Send + Sync + Debug {
    fn validate(&self, tables: Option<Vec<TableInfo>>) -> Result<(), ConnectorError>;
    fn validate_schemas(&self, tables: &[TableInfo]) -> Result<ValidationResults, ConnectorError>;

    fn get_schemas(
        &self,
        table_names: Option<Vec<TableInfo>>,
    ) -> Result<Vec<SourceSchema>, ConnectorError>;

    fn can_start_from(&self, last_checkpoint: (u64, u64)) -> Result<bool, ConnectorError>;
    fn start(
        &self,
        last_checkpoint: Option<(u64, u64)>,
        ingestor: &Ingestor,
        tables: Vec<TableInfo>,
    ) -> Result<(), ConnectorError>;
    fn get_tables(&self, tables: Option<&[TableInfo]>) -> Result<Vec<TableInfo>, ConnectorError>;

    // This is a default table mapping from schemas. It will result in errors if connector has unsupported data types.
    fn get_tables_default(
        &self,
        tables: Option<&[TableInfo]>,
    ) -> Result<Vec<TableInfo>, ConnectorError> {
        Ok(self
            .get_schemas(tables.map(|t| t.to_vec()))?
            .iter()
            .enumerate()
            .map(|(id, s)| TableInfo {
                name: s.name.to_string(),
                table_name: s.name.to_string(),
                id: id as u32,
                columns: Some(
                    s.schema
                        .fields
                        .iter()
                        .map(|f| ColumnInfo {
                            name: f.name.to_string(),
                            data_type: Some(f.typ.to_string()),
                        })
                        .collect(),
                ),
            })
            .collect::<Vec<TableInfo>>())
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
#[serde(crate = "self::serde")]
pub struct TableInfo {
    pub name: String,
    pub table_name: String,
    pub id: u32,
    pub columns: Option<Vec<ColumnInfo>>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, Hash)]
#[serde(crate = "self::serde")]
pub struct ColumnInfo {
    pub name: String,
    pub data_type: Option<String>,
}

impl ColumnInfo {
    pub fn new(name: String, data_type: Option<String>) -> Self {
        Self { name, data_type }
    }
}

pub fn get_connector(connection: Connection) -> Result<Box<dyn Connector>, ConnectorError> {
    let config = connection
        .config
        .ok_or_else(|| ConnectorError::MissingConfiguration(connection.name.clone()))?;
    match config {
        ConnectionConfig::Postgres(_) => {
            let config = map_connection_config(&config)?;
            let postgres_config = PostgresConfig {
                name: connection.name,
                tables: None,
                config,
            };

            if let Some(dbname) = postgres_config.config.get_dbname() {
                debug!("Connecting to postgres database - {}", dbname.to_string());
            }
            Ok(Box::new(PostgresConnector::new(1, postgres_config)))
        }
        ConnectionConfig::Ethereum(eth_config) => match eth_config.provider.unwrap() {
            dozer_types::ingestion_types::EthProviderConfig::Log(log_config) => Ok(Box::new(
                EthLogConnector::new(2, log_config, connection.name),
            )),
            dozer_types::ingestion_types::EthProviderConfig::Trace(trace_config) => Ok(Box::new(
                EthTraceConnector::new(2, trace_config, connection.name),
            )),
        },
        ConnectionConfig::Grpc(grpc_config) => Ok(Box::new(GrpcConnector::new(
            3,
            connection.name,
            grpc_config,
        ))),
        ConnectionConfig::Snowflake(snowflake) => {
            let snowflake_config = snowflake;

            Ok(Box::new(SnowflakeConnector::new(
                connection.name,
                snowflake_config,
            )))
        }
        ConnectionConfig::Kafka(kafka_config) => Ok(Box::new(KafkaConnector::new(5, kafka_config))),
        ConnectionConfig::S3Storage(object_store_config) => {
            Ok(Box::new(ObjectStoreConnector::new(5, object_store_config)))
        }
        ConnectionConfig::LocalStorage(object_store_config) => {
            Ok(Box::new(ObjectStoreConnector::new(5, object_store_config)))
        }
    }
}

pub fn get_connector_info_table(connection: &Connection) -> Option<Table> {
    match &connection.config {
        Some(ConnectionConfig::Postgres(config)) => Some(config.convert_to_table()),
        Some(ConnectionConfig::Ethereum(config)) => Some(config.convert_to_table()),
        Some(ConnectionConfig::Snowflake(config)) => Some(config.convert_to_table()),
        Some(ConnectionConfig::Kafka(config)) => Some(config.convert_to_table()),
        Some(ConnectionConfig::S3Storage(config)) => Some(config.convert_to_table()),
        Some(ConnectionConfig::LocalStorage(config)) => Some(config.convert_to_table()),
        _ => None,
    }
}
