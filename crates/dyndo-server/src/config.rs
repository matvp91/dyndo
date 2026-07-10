use std::path::PathBuf;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub assets_base_path: PathBuf,
    pub port: u16,
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        use figment::{
            providers::{Env, Format, Yaml},
            Figment,
        };
        Ok(Figment::new()
            .merge(Yaml::file("config.yaml"))
            .merge(Env::prefixed("DYNDO_"))
            .extract()?)
    }
}

#[cfg(test)]
mod tests {
    use figment::{
        providers::{Format, Yaml},
        Figment,
    };

    use super::*;

    #[test]
    fn parses_assets_base_path_and_port() {
        let cfg: Config = Figment::new()
            .merge(Yaml::string("assets_base_path: ./media\nport: 9000\n"))
            .extract()
            .unwrap();
        assert_eq!(cfg.port, 9000);
        assert_eq!(cfg.assets_base_path, PathBuf::from("./media"));
    }
}
