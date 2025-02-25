use std::fs;

use crate::errors::AdminError;
use dozer_types::constants::DEFAULT_HOME_DIR;
use dozer_types::serde::{Deserialize, Serialize};
use dozer_types::serde_yaml;
pub mod cli_process;
pub mod types;
pub mod utils;
#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
pub struct AdminCliConfig {
    #[serde(default = "default_ui_port")]
    pub ui_port: u32,
    #[serde(default = "default_port")]
    pub port: u32,
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_cors")]
    pub cors: bool,
    #[serde(default = "default_home_dir")]
    pub home_dir: String,
    #[serde(default = "default_ui_path")]
    pub ui_path: String,
}
fn default_ui_path() -> String {
    AdminCliConfig::default().ui_path
}
fn default_ui_port() -> u32 {
    AdminCliConfig::default().ui_port
}
fn default_port() -> u32 {
    AdminCliConfig::default().port
}
fn default_host() -> String {
    AdminCliConfig::default().host
}
fn default_cors() -> bool {
    AdminCliConfig::default().cors
}
fn default_home_dir() -> String {
    DEFAULT_HOME_DIR.to_owned()
}

impl Default for AdminCliConfig {
    fn default() -> Self {
        Self {
            port: 8081,
            ui_port: 3000,
            host: "[::0]".to_owned(),
            cors: true,
            home_dir: default_home_dir(),
            ui_path: "./ui".to_owned(),
        }
    }
}
pub fn load_config(config_path: String) -> Result<AdminCliConfig, AdminError> {
    let contents = fs::read_to_string(config_path).map_err(AdminError::FailedToLoadFile)?;
    let config: AdminCliConfig =
        serde_yaml::from_str(&contents).map_err(|e| AdminError::FailedToParseYaml(Box::new(e)))?;
    Ok(config)
}
