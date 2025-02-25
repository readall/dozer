use dozer_types::ingestion_types::{LocalDetails, LocalStorage, Table};
use std::path::PathBuf;

pub fn get_local_storage_config(typ: &str) -> LocalStorage {
    let p = PathBuf::from("src/connectors/object_store/tests/files".to_string());
    LocalStorage {
        details: Some(LocalDetails {
            path: p.to_str().unwrap().to_string(),
        }),
        tables: vec![Table {
            name: format!("all_types_{typ}"),
            prefix: format!("all_types_{typ}"),
            file_type: typ.to_string(),
            extension: typ.to_string(),
        }],
    }
}
