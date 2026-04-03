use std::fmt::Display;
use std::sync::atomic::{AtomicUsize, Ordering};

use anchor_lang::AccountDeserialize;
use anyhow::{Context, Result};
use blockmine_program::{
    constants::{
        CONFIG_SEED, CURRENT_BLOCK_SEED, MINER_STATS_SEED, MINING_SESSION_SEED,
        VAULT_AUTHORITY_SEED,
    },
    state::{CurrentBlock, MinerStats, MiningSession, ProtocolConfig},
};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    account::Account,
    commitment_config::CommitmentConfig,
    hash::Hash,
    pubkey::Pubkey,
    signature::Signature,
    transaction::Transaction,
};

use crate::config::CliConfig;

pub const OFFICIAL_RPC_URL: &str = "https://api.mainnet-beta.solana.com";
pub const PUBLICNODE_RPC_URL: &str = "https://solana-rpc.publicnode.com";
pub const AUTO_RPC_INPUT: &str = "auto";

pub fn default_rpc_input() -> &'static str {
    AUTO_RPC_INPUT
}

pub fn resolve_rpc_endpoints(raw: &str) -> Vec<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case(AUTO_RPC_INPUT) {
        return vec![OFFICIAL_RPC_URL.to_string(), PUBLICNODE_RPC_URL.to_string()];
    }

    let mut endpoints = Vec::new();
    for token in trimmed
        .split(|ch: char| matches!(ch, ',' | ';' | '\n' | '\r' | '|'))
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if token.eq_ignore_ascii_case(AUTO_RPC_INPUT) {
            endpoints.push(OFFICIAL_RPC_URL.to_string());
            endpoints.push(PUBLICNODE_RPC_URL.to_string());
        } else {
            endpoints.push(token.to_string());
        }
    }

    let mut unique = Vec::new();
    for endpoint in endpoints {
        if !unique.iter().any(|existing| existing == &endpoint) {
            unique.push(endpoint);
        }
    }

    if unique.is_empty() {
        vec![OFFICIAL_RPC_URL.to_string(), PUBLICNODE_RPC_URL.to_string()]
    } else {
        unique
    }
}

pub fn summarize_rpc_pool(raw: &str) -> String {
    let endpoints = resolve_rpc_endpoints(raw);
    match endpoints.as_slice() {
        [] => "No RPC endpoints configured".to_string(),
        [single] => single.clone(),
        [first, second] => format!("{first} -> {second}"),
        [first, second, ..] => format!("{first} -> {second} (+{} more)", endpoints.len() - 2),
    }
}

pub struct RpcFacade {
    clients: Vec<RpcClient>,
    endpoints: Vec<String>,
    active_index: AtomicUsize,
    pub program_id: Pubkey,
}

impl RpcFacade {
    pub fn new(config: &CliConfig) -> Self {
        Self::from_parts(&config.rpc_url, config.program_id, config.commitment)
    }

    pub fn from_parts(rpc_input: &str, program_id: Pubkey, commitment: CommitmentConfig) -> Self {
        let endpoints = resolve_rpc_endpoints(rpc_input);
        let clients = endpoints
            .iter()
            .map(|url| RpcClient::new_with_commitment(url.clone(), commitment))
            .collect();

        Self {
            clients,
            endpoints,
            active_index: AtomicUsize::new(0),
            program_id,
        }
    }

    pub fn client(&self) -> &RpcClient {
        let active = self.active_index.load(Ordering::Relaxed);
        &self.clients[active.min(self.clients.len().saturating_sub(1))]
    }

    pub fn active_url(&self) -> &str {
        let active = self.active_index.load(Ordering::Relaxed);
        &self.endpoints[active.min(self.endpoints.len().saturating_sub(1))]
    }

    pub fn configured_urls(&self) -> &[String] {
        &self.endpoints
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
        let account = self.get_account(pubkey)?;
        let mut data = account.data.as_slice();
        T::try_deserialize(&mut data).context("anchor account deserialization failed")
    }

    pub fn get_account(&self, pubkey: &Pubkey) -> Result<Account> {
        self.run_rpc(&format!("fetching account {pubkey}"), |client| {
            client.get_account(pubkey)
        })
        .map_err(|error| explain_rpc_fetch_error(error, pubkey))
    }

    pub fn get_balance(&self, pubkey: &Pubkey) -> Result<u64> {
        self.run_rpc(&format!("fetching balance for {pubkey}"), |client| {
            client.get_balance(pubkey)
        })
    }

    pub fn get_token_account_balance_raw(&self, pubkey: &Pubkey) -> Result<u64> {
        self.run_rpc(
            &format!("fetching token balance for account {pubkey}"),
            |client| client.get_token_account_balance(pubkey),
        )
        .map(|amount| amount.amount.parse::<u64>().unwrap_or(0))
        .or_else(|error| {
            if is_missing_account_error(&error.to_string()) {
                Ok(0)
            } else {
                Err(error)
            }
        })
    }

    pub fn account_exists(&self, pubkey: &Pubkey) -> Result<bool> {
        match self.get_account(pubkey) {
            Ok(_) => Ok(true),
            Err(error) if is_missing_account_error(&error.to_string()) => Ok(false),
            Err(error) => Err(error),
        }
    }

    pub fn get_latest_blockhash(&self) -> Result<Hash> {
        self.run_rpc("fetching latest blockhash", |client| client.get_latest_blockhash())
    }

    pub fn get_minimum_balance_for_rent_exemption(&self, data_len: usize) -> Result<u64> {
        self.run_rpc(
            &format!("fetching rent exemption minimum for {data_len} bytes"),
            |client| client.get_minimum_balance_for_rent_exemption(data_len),
        )
    }

    pub fn get_fee_for_message(
        &self,
        message: &solana_sdk::message::Message,
    ) -> Result<u64> {
        self.run_rpc("fetching fee for transaction message", |client| {
            client.get_fee_for_message(message)
        })
    }

    pub fn send_and_confirm_transaction(&self, transaction: &Transaction) -> Result<Signature> {
        self.run_rpc("sending and confirming transaction", |client| {
            client.send_and_confirm_transaction(transaction)
        })
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

    fn run_rpc<T, E, F>(&self, label: &str, mut operation: F) -> Result<T>
    where
        F: FnMut(&RpcClient) -> std::result::Result<T, E>,
        E: Display,
    {
        let active = self.active_index.load(Ordering::Relaxed);
        let mut attempts = Vec::with_capacity(self.clients.len());

        for offset in 0..self.clients.len() {
            let index = (active + offset) % self.clients.len();
            let client = &self.clients[index];
            match operation(client) {
                Ok(value) => {
                    self.active_index.store(index, Ordering::Relaxed);
                    return Ok(value);
                }
                Err(error) => {
                    attempts.push(format!(
                        "{} -> {}",
                        self.endpoints[index],
                        humanize_rpc_error(&error.to_string())
                    ));
                }
            }
        }

        anyhow::bail!(
            "all configured RPC endpoints failed while {label}.\n{}",
            attempts.join("\n")
        )
    }
}

fn explain_rpc_fetch_error(error: anyhow::Error, pubkey: &Pubkey) -> anyhow::Error {
    let error_text = error.to_string();
    if is_rpc_rejection_error(&error_text) {
        anyhow::anyhow!(
            "RPC endpoint rejected the request while fetching account {}. The miner is likely being rate-limited or blocked by the current RPC pool. Original error: {}",
            pubkey,
            error_text
        )
    } else if is_missing_account_error(&error_text) {
        anyhow::anyhow!(
            "account {} was not found on the configured RPC pool. Original RPC error: {}",
            pubkey,
            error_text
        )
    } else {
        anyhow::anyhow!("{error_text}")
    }
}

fn humanize_rpc_error(error_text: &str) -> String {
    if is_rpc_rejection_error(error_text) {
        format!("RPC rejected the request ({error_text})")
    } else if is_missing_account_error(error_text) {
        format!("account not found ({error_text})")
    } else {
        error_text.to_string()
    }
}

fn is_rpc_rejection_error(error_text: &str) -> bool {
    let normalized = error_text.to_ascii_lowercase();
    normalized.contains("429")
        || normalized.contains("too many requests")
        || normalized.contains("403")
        || normalized.contains("forbidden")
        || normalized.contains("timeout")
        || normalized.contains("timed out")
        || normalized.contains("connection reset")
        || normalized.contains("unavailable")
        || normalized.contains("gateway")
}

fn is_missing_account_error(error_text: &str) -> bool {
    let normalized = error_text.to_ascii_lowercase();
    !is_rpc_rejection_error(&normalized)
        && (normalized.contains("accountnotfound")
            || normalized.contains("could not find account")
            || normalized.contains("account not found"))
}
