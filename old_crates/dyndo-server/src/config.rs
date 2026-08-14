use std::path::Path;

use figment::Figment;
use figment::providers::{Env, Format, Serialized, Yaml};
use opendal::Operator;
use opendal::services::{FsConfig, S3Config};
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("DYNDO_CONFIG points to a missing file: {0}")]
    MissingConfigFile(String),
    // Boxed to keep `Result<_, ConfigError>` small.
    #[error("failed to load configuration: {0}")]
    Load(Box<figment::Error>),
    #[error("failed to build storage operator: {0}")]
    Operator(#[from] opendal::Error),
}

impl From<figment::Error> for ConfigError {
    fn from(e: figment::Error) -> Self {
        ConfigError::Load(Box::new(e))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum StoreKind {
    Fs,
    S3,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ServerConfig {
    host: String,
    port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    store: StoreKind,
    server: ServerConfig,
    #[serde(skip_serializing_if = "Option::is_none")]
    fs: Option<FsConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    s3: Option<S3Config>,
}

impl AppConfig {
    fn defaults() -> Self {
        AppConfig {
            store: StoreKind::Fs,
            server: ServerConfig {
                host: "0.0.0.0".to_string(),
                port: 8080,
            },
            fs: None,
            s3: None,
        }
    }

    pub fn bind(&self) -> (&str, u16) {
        (&self.server.host, self.server.port)
    }

    pub fn build_operator(&self) -> Result<Operator, ConfigError> {
        let op = match self.store {
            StoreKind::Fs => Operator::from_config(self.fs.clone().unwrap_or_default())?,
            StoreKind::S3 => Operator::from_config(self.s3.clone().unwrap_or_default())?,
        };
        Ok(op)
    }
}

pub fn load() -> Result<AppConfig, ConfigError> {
    let cfg = build_figment()?.extract()?;
    Ok(cfg)
}

fn build_figment() -> Result<Figment, ConfigError> {
    let path = match std::env::var("DYNDO_CONFIG") {
        Ok(p) => {
            if !Path::new(&p).exists() {
                return Err(ConfigError::MissingConfigFile(p));
            }
            p
        }
        Err(_) => "config.yaml".to_string(),
    };
    // `Yaml::file` silently yields no data if the path is absent, which is what
    // we want for the default `config.yaml`. `split("__")` nests on a *double*
    // underscore so single underscores inside field names survive:
    // `DYNDO_SERVER__PORT` -> `server.port`, `DYNDO_S3__ACCESS_KEY_ID` ->
    // `s3.access_key_id`. The stray `DYNDO_CONFIG` -> `config` key has no
    // matching field and is ignored on extract.
    Ok(Figment::new()
        .merge(Serialized::defaults(AppConfig::defaults()))
        .merge(Yaml::file(path))
        .merge(Env::prefixed("DYNDO_").split("__")))
}
