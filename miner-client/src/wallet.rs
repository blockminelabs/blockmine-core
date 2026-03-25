use std::{
    env,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use solana_sdk::signature::{read_keypair_file, Keypair};

use crate::config::CliConfig;

pub fn load_keypair(config: &CliConfig) -> Result<Keypair> {
    let path = if let Some(path) = &config.keypair_path {
        expand_tilde(path)
    } else if let Ok(wallet_path) = env::var("SOLANA_WALLET") {
        expand_tilde(PathBuf::from(wallet_path).as_path())
    } else {
        default_wallet_path().context("unable to resolve the default Solana wallet path")?
    };

    read_keypair_file(&path)
        .map_err(|error| anyhow::anyhow!("unable to read keypair at {}: {}", path.display(), error))
}

fn default_wallet_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("home directory is not available")?;
    Ok(home.join(".config").join("solana").join("id.json"))
}

fn expand_tilde(path: &Path) -> PathBuf {
    let raw = path.display().to_string();
    if let Some(stripped) = raw.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(stripped);
        }
    }
    path.to_path_buf()
}
