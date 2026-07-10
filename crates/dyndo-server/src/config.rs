//! Server configuration: layered load (defaults <- config.yaml <- DYNDO_* env)
//! into OpenDAL's own backend config structs, plus construction of the storage
//! Operator selected by `store`. We never mutate process env vars; YAML is
//! deserialized straight into `FsConfig`/`S3Config` and fed to
//! `Operator::from_config`.

use opendal::services::{FsConfig, S3Config};
use serde::{Deserialize, Serialize};

use std::error::Error;
use std::path::Path;

use figment::providers::{Env, Format, Serialized, Yaml};
use figment::Figment;

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

/// Load configuration by layering: defaults <- YAML file <- `DYNDO_*` env.
///
/// The file is `./config.yaml`, or the path in `DYNDO_CONFIG` when set.
pub fn load() -> Result<AppConfig, Box<dyn Error>> {
    let cfg = build_figment()?.extract()?;
    Ok(cfg)
}

/// Assemble the figment providers. A missing default `config.yaml` is ignored
/// (figment skips it); a missing *explicit* `DYNDO_CONFIG` path is fatal.
fn build_figment() -> Result<Figment, Box<dyn Error>> {
    let path = match std::env::var("DYNDO_CONFIG") {
        Ok(p) => {
            if !Path::new(&p).exists() {
                return Err(format!("DYNDO_CONFIG points to a missing file: {p}").into());
            }
            p
        }
        Err(_) => "config.yaml".to_string(),
    };
    // `Yaml::file` silently yields no data if the path is absent, which is what
    // we want for the default `config.yaml`. `Env::split("_")` maps
    // `DYNDO_SERVER_PORT` -> `server.port`; the stray `DYNDO_CONFIG` -> `config`
    // key has no matching field and is ignored on extract.
    Ok(Figment::new()
        .merge(Serialized::defaults(AppConfig::defaults()))
        .merge(Yaml::file(path))
        .merge(Env::prefixed("DYNDO_").split("_")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use figment::Jail;

    #[test]
    fn defaults_are_fs_and_8080() {
        let c = AppConfig::defaults();
        assert_eq!(c.store, StoreKind::Fs);
        assert!(c.fs.is_none());
        assert!(c.s3.is_none());
        assert_eq!(c.server.host, "0.0.0.0");
        assert_eq!(c.server.port, 8080);
    }

    #[test]
    fn load_with_no_file_uses_defaults() {
        Jail::expect_with(|_jail| {
            let c = load().unwrap();
            assert_eq!(c.store, StoreKind::Fs);
            assert!(c.fs.is_none());
            assert_eq!(c.server.port, 8080);
            Ok(())
        });
    }

    #[test]
    fn load_reads_yaml_and_selects_s3() {
        Jail::expect_with(|jail| {
            jail.create_file(
                "config.yaml",
                "store: s3\nserver:\n  port: 9000\ns3:\n  bucket: my-assets\n  region: eu-west-1\n",
            )?;
            let c = load().unwrap();
            assert_eq!(c.store, StoreKind::S3);
            assert_eq!(c.server.port, 9000);
            // host absent from YAML -> supplied by the defaults layer (deep merge).
            assert_eq!(c.server.host, "0.0.0.0");
            let s3 = c.s3.as_ref().unwrap();
            assert_eq!(s3.bucket, "my-assets");
            assert_eq!(s3.region.as_deref(), Some("eu-west-1"));
            Ok(())
        });
    }

    #[test]
    fn env_overrides_yaml() {
        Jail::expect_with(|jail| {
            jail.create_file("config.yaml", "server:\n  port: 9000\n")?;
            jail.set_env("DYNDO_SERVER_PORT", "1234");
            let c = load().unwrap();
            assert_eq!(c.server.port, 1234);
            Ok(())
        });
    }

    #[test]
    fn missing_dyndo_config_path_errors() {
        Jail::expect_with(|jail| {
            jail.set_env("DYNDO_CONFIG", "does-not-exist.yaml");
            assert!(load().is_err());
            Ok(())
        });
    }

    #[test]
    fn explicit_dyndo_config_path_loads() {
        Jail::expect_with(|jail| {
            jail.create_file(
                "custom.yaml",
                "store: s3\ns3:\n  bucket: b\n  region: us-east-1\n",
            )?;
            jail.set_env("DYNDO_CONFIG", "custom.yaml");
            let c = load().unwrap();
            assert_eq!(c.store, StoreKind::S3);
            assert_eq!(c.s3.as_ref().unwrap().bucket, "b");
            Ok(())
        });
    }
}
