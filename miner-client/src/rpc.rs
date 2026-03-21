use anyhow::{Context, Result};
use anchor_lang::AccountDeserialize;
use blockmine_program::{
    constants::{CONFIG_SEED, CURRENT_BLOCK_SEED, MINER_STATS_SEED, MINING_SESSION_SEED, VAULT_AUTHORITY_SEED},
    state::{CurrentBlock, MinerStats, MiningSession, ProtocolConfig},
};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    pubkey::Pubkey,
};

use crate::config::CliConfig;

pub struct RpcFacade {
    client: RpcClient,
    pub program_id: Pubkey,
}

impl RpcFacade {
    pub fn new(config: &CliConfig) -> Self {
        Self {
            client: RpcClient::new_with_commitment(config.rpc_url.clone(), config.commitment),
            program_id: config.program_id,
        }
    }

    pub fn client(&self) -> &RpcClient {
        &self.client
    }

    pub fn fetch_protocol_config(&self) -> Result<ProtocolConfig> {
        let (config_pda, _) = self.config_pda();
        self.fetch_anchor_account::<ProtocolConfig>(&config_pda)
            .context("failed to fetch protocol config")
    }

    pub fn fetch_current_block(&self) -> Result<CurrentBlock> {
        let (block_pda, _) = self.current_block_pda();
        self.fetch_anchor_account::<CurrentBlock>(&block_pda)
            .context("failed to fetch current block")
    }

    pub fn fetch_miner_stats(&self, miner: &Pubkey) -> Result<MinerStats> {
        let (miner_stats_pda, _) = self.miner_stats_pda(miner);
        self.fetch_anchor_account::<MinerStats>(&miner_stats_pda)
            .context("failed to fetch miner stats")
    }

    pub fn fetch_mining_session(&self, miner: &Pubkey) -> Result<MiningSession> {
        let (session_pda, _) = self.mining_session_pda(miner);
        self.fetch_anchor_account::<MiningSession>(&session_pda)
            .context("failed to fetch mining session")
    }

    pub fn fetch_anchor_account<T: AccountDeserialize>(&self, pubkey: &Pubkey) -> Result<T> {
        let account = self
            .client
            .get_account(pubkey)
            .with_context(|| format!("account {} not found", pubkey))?;
        let mut data = account.data.as_slice();
        T::try_deserialize(&mut data).context("anchor account deserialization failed")
    }

    pub fn config_pda(&self) -> (Pubkey, u8) {
        Pubkey::find_program_address(&[CONFIG_SEED], &self.program_id)
    }

    pub fn current_block_pda(&self) -> (Pubkey, u8) {
        Pubkey::find_program_address(&[CURRENT_BLOCK_SEED], &self.program_id)
    }

    pub fn miner_stats_pda(&self, miner: &Pubkey) -> (Pubkey, u8) {
        Pubkey::find_program_address(&[MINER_STATS_SEED, miner.as_ref()], &self.program_id)
    }

    pub fn vault_authority_pda(&self) -> (Pubkey, u8) {
        Pubkey::find_program_address(&[VAULT_AUTHORITY_SEED], &self.program_id)
    }

    pub fn mining_session_pda(&self, miner: &Pubkey) -> (Pubkey, u8) {
        Pubkey::find_program_address(&[MINING_SESSION_SEED, miner.as_ref()], &self.program_id)
    }
}
