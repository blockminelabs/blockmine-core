use anyhow::{Context, Result};
use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey};

use crate::cli::Cli;

#[derive(Debug, Clone)]
pub struct CliConfig {
    pub rpc_url: String,
    pub program_id: Pubkey,
    pub keypair_path: Option<std::path::PathBuf>,
    pub commitment: CommitmentConfig,
}

impl CliConfig {
    pub fn from_cli(cli: &Cli) -> Result<Self> {
        let program_id = cli
            .program_id
            .parse::<Pubkey>()
            .context("invalid program id")?;

        Ok(Self {
            rpc_url: cli.rpc.clone(),
            program_id,
            keypair_path: cli.keypair.clone(),
            commitment: CommitmentConfig::confirmed(),
        })
    }
}
