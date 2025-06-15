use std::collections::HashMap;

use config::ServerConfig;
use const_format::concatcp;
use database::DbConnection;
use sdk::TokenVerificationModule;
use std::{collections::HashMap, fs, path::Path};
use yixuan_service::{
    ServiceContext, ServiceError,
    config::{ServiceType, load_environment_config},
    network::{NetworkEntityManager, NetworkServer},
};

mod config;
mod database;
mod handlers;
mod sdk;

const SERVICE_TYPE: ServiceType = ServiceType::Dbgate;
const CONFIG_DIR: &str = "config/30-dbgate-server/";

#[derive(thiserror::Error, Debug)]
pub enum StartupError {
    #[error("failed to connect to database {0}")]
    DbConnect(#[from] sqlx::Error),
    #[error("{0}")]
    Service(#[from] ServiceError),
}

#[tokio::main]
async fn main() -> Result<(), StartupError> {
    common::logging::init_tracing(tracing::Level::DEBUG);
    let env = load_environment_config();

    let config = common::config_util::load_or_create::<ServerConfig>(
        concatcp!(CONFIG_DIR, "config.toml"),
        include_str!("../config.default.toml"),
    );

    // Create parent directory for SQLite DB file if needed
    if let crate::config::DbType::Sqlite = config.database.db_type {
        let db_path = format!("{}.db", config.database.database);
        if let Some(parent_dir) = Path::new(&db_path).parent() {
            if parent_dir.components().next().is_some() {
                if !parent_dir.exists() {
                    fs::create_dir_all(parent_dir)
                        .map_err(|e| StartupError::Service(ServiceError::ModuleStartup(Box::new(e))))?;
                    tracing::info!("Created database directory: {:?}", parent_dir);
                }
            }
        }
    }

    let db_connection = DbConnection::connect(&config.database).await?;

    let (service_tx, listener) = handlers::start_handler_task();

    let service = ServiceContext::new()
        .insert_module(db_connection)
        .insert_module(NetworkEntityManager::new(listener, HashMap::new()))
        .configure_module::<TokenVerificationModule>(config.auth)
        .configure_module::<NetworkServer>(vec![env.services.get(&SERVICE_TYPE).unwrap().addr])
        .start()?;

    let _ = service_tx.send(service);

    tokio::signal::ctrl_c().await.unwrap();
    Ok(())
}
