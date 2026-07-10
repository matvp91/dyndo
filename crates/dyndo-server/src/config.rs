//! Server configuration: layered load (defaults <- config.yaml <- DYNDO_* env)
//! into OpenDAL's own backend config structs, plus construction of the storage
//! Operator selected by `store`. We never mutate process env vars; YAML is
//! deserialized straight into `FsConfig`/`S3Config` and fed to
//! `Operator::from_config`.

use opendal::services::{FsConfig, S3Config};
use serde::{Deserialize, Serialize};

/// Which OpenDAL backend serves assets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum StoreKind {
    Fs,
    S3,
}

/// HTTP listener settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ServerConfig {
    host: String,
    port: u16,
}

/// Fully resolved server configuration. Both `fs` and `s3` are optional; the one
/// named by `store` must supply what OpenDAL needs (fs: `root`, s3: `bucket`) or
/// the operator fails to build.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    store: StoreKind,
    server: ServerConfig,
    // Optional so a missing section deserializes to `None` (serde treats Option
    // fields as optional without `#[serde(default)]`); `skip_serializing_if`
    // keeps the defaults layer from emitting stray `null`s.
    #[serde(skip_serializing_if = "Option::is_none")]
    fs: Option<FsConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    s3: Option<S3Config>,
}

impl AppConfig {
    /// Built-in defaults — the lowest figment layer: `store: fs` and the server
    /// bind address. Neither backend is defaulted; the selected store's config
    /// must be supplied (file/env) or OpenDAL fails when building the operator.
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_fs_and_8080() {
        let c = AppConfig::defaults();
        assert_eq!(c.store, StoreKind::Fs);
        assert!(c.fs.is_none());
        assert!(c.s3.is_none());
        assert_eq!(c.server.host, "0.0.0.0");
        assert_eq!(c.server.port, 8080);
    }
}
