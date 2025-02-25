# Dozer Ingestion

This module implements several connectors that can act as a source in either real-time or batch fashion. 
Each of the connectors implements a [set of methods defined](https://github.com/getdozer/dozer/blob/main/dozer-ingestion/src/connectors/mod.rs#L32) to support being a source to the data pipeline.

### Available Connectors
| Connector                          |   Status    | Type           |  Schema Mapping   | History Support | Frequency | Implemented Via |
| :--------------------------------- | :---------: | :------------- | :---------------: | :-------------: | :-------- | :-------------- |
| Postgres                           | Available ✅ | Relational     |      Source       |        ✅        | Real Time | Direct          |
| Ethereum                           | Available ✅ | Blockchain     | Logs/Contract ABI |        ✅        | Real Time | Direct          |
| Snowflake                          | Available ✅ | Data Warehouse |      Source       |        ✅        | Polling   | Direct          |
| Local Files(CSV, Parquet)          |    Beta     | Object Storage |      Source       |        ✅        | Polling   | Data Fusion     |
| AWS S3(CSV, Parquet)               |    Beta     | Object Storage |      Source       |        ✅        | Polling   | Data Fusion     |
| Google Cloud Storage(CSV, Parquet) |    Beta     | Object Storage |      Source       |        ✅        | Polling   | Data Fusion     |
| MySQL                              | In Roadmap  | Relational     |      Source       |        ✅        | Real Time | Debezium        |
| Mongodb                            | In Roadmap  | NoSQL          |      Source       |        ✅        | Real Time | Debezium        |
| Google Sheets                      | In Roadmap  | Applications   |      Source       |        ✅        |           |                 |
| Airtable                           | In Roadmap  | Applications   |      Source       |        ✅        |           |                 |
| Delta Lake                         | In Roadmap  | Data Warehouse |      Source       |        ✅        |           |                 |
| Solana                             | In Roadmap  | Blockchain     | Logs/Contract ABI |        ✅        |           |                 |
| Kafka                              | In Roadmap  | Stream         |  Schema Registry  |        ✅        |           |                 |
| Red Panda                          | In Roadmap  | Stream         |  Schema Registry  |        ✅        |           |                 |
| Pulsar                             | In Roadmap  | Stream         |  Schema Registry  |        ✅        |           |                 |

# New connector implementation

## Trait

Every connector to external database needs to implement connector trait [/dozer-ingestion/src/connectors/mod.rs](https://github.com/getdozer/dozer/blob/main/dozer-ingestion/src/connectors/mod.rs#L35)

* ```fn validate(&self, tables: Option<Vec<TableInfo>>) -> Result<(), ConnectorError>;```<br/>
  This function is supposed to validate connector configuration and connection to database.                                     
  <br/>
* ```fn validate_schemas(&self, tables: &[TableInfo]) -> Result<ValidationResults, ConnectorError>;```<br/>
  This function's purpose is to validate schemas, which are used as sources for data ingestion. It should return error when column type is not supported by existing dozer types.                                                                                                          
  <br/>
* ```fn get_schemas(&self, table_names: Option<Vec<TableInfo>>) -> Result<Vec<SchemaWithChangesType>, ConnectorError>;```<br/>
  This function is used for getting mapped external database schema to dozer schema. Also, as result of schema definition, developer also should provide `ReplicationChangesTrackingType` (described below)                                      
  <br/>
* ```fn get_tables(&self, tables: Option<&[TableInfo]>) -> Result<Vec<TableInfo>, ConnectorError>;```<br/>
  Using this method, developer can fetch all available tables from connector.                                                                          
  <br/>
* ```fn start(&self, from_seq: Option<(u64, u64)>) -> Result<(), ConnectorError>;```<br/>
  This function is responsible for all ingestion processes. It has single parameter, which is used to resume ingestion it was stopped. That tuple contains two values - `(u64, u64)`, first value is lsn of transaction and second value is seq no of last consumer record in transaction. It is used to allow connector to continue from middle of transaction.
  <br/>

## Connector functions usage in dozer commands
Dozer uses connector methods in 3 different commands. During `connector ls` execution, dozer just fetches schemas. To get schemas we use `get_schemas` method.
The other two dozer commands, which use connector are `migrate` and `app run`. During both command execution, first, we validate the connection and schema using `validate` and `validate_schemas` methods. Later, if validation passes, `get_schemas` method is called. After that `app run` command also calls for `start` method in the connector and starts data ingestion.


### Source configuration

Selection of which tables and columns will be used in ingestion is defined in  `sources`  configuration. </br>
The structure of configuration part is this:

```yaml
  name: users
  connection: !Ref pg_data_connection
  table_name: userdata      
  columns:
    - gender        
    - email  
```

From this configuration `table_name` is the table name in the external database and  `name`  is used in dozer transformations. Another property  `connection`  is the reference to connection, which is already defined in  `connections`  configuration. The `columns`  property is used to restrict the list of used columns from the external database. If this value is an empty array, ingestion will fetch all columns of that table.

### Tables and columns selection

Every external schema should be mapped using dozer types. The latest types definitions can be found at [https://getdozer.io/docs/reference/data_types](https://getdozer.io/docs/reference/data_types).
If type is not supported, connector should return error, during schema validation step. During ingestion data should be cast to same type as it was defined in schema.                                                                                                        

### Schemas

During pipeline start,  `start`  function receives tuple  `from_seq: (u64, u64)`. That tuple is used to tell last message lsn and seq no. One lsn is shared for all operations inside single transaction, while second parameter is used for determining how many messages were successfully processed from transaction.

### Unit tests
Unit tests are only possible in places where connection to external database is not required. It is important to have unit tests for schema mapping and data casting to dozer types. More complex tests should be implemented using E2E tests.

### E2E tests
More complex tests require to have connection real database. Such tests cases expect to have several things:
- Database infrastructure, preferably created in docker container(s)
- Connection configuration (with placeholders)
- It should be possible to run test cases without doing any manual modifications in database.

### Replication changes tracking types

| Type        |                                                                                           |
| ----------- | ----------------------------------------------------------------------------------------- |
| FullChanges | Connector gets old record on delete/update operations                                     |
| OnlyPK      | Connector only gets PK of old record on delete/update operations                          |
| Nothing     | Connector cannot get any info about old records. In other words, the table is append-only |
