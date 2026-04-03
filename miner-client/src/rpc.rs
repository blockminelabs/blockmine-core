use anchor_lang::AccountDeserialize;
use anyhow::{Context, Result};
use blockmine_program::{
    constants::{
        CONFIG_SEED, CURRENT_BLOCK_SEED, MINER_STATS_SEED, MINING_SESSION_SEED,
        VAULT_AUTHORITY_SEED,
    },
    state::{CurrentBlock, MinerStats, MiningSession, ProtocolConfig},
};
use reqwest::blocking::Client;
use serde::Deserialize;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    account::Account,
    commitment_config::CommitmentConfig,
    hash::Hash,
    message::Message,
    pubkey::Pubkey,
    signature::Signature,
    transaction::Transaction,
};

use crate::config::CliConfig;

pub const PUBLICNODE_RPC_URL: &str = "https://solana-rpc.publicnode.com";
pub const MINER_STATE_RELAY_URL: &str = "https://blockmine.dev/api/miner/state";

pub struct RpcFacade {
    client: RpcClient,
    relay_client: Client,
    pub program_id: Pubkey,
}

#[derive(Debug, Deserialize)]
struct MinerStateRelayResponse {
    ok: bool,
    config: RelayProtocolConfig,
    #[serde(rename = "currentBlock")]
    current_block: RelayCurrentBlock,
}

#[derive(Debug, Deserialize)]
struct RelayProtocolConfig {
    admin: String,
    #[serde(rename = "blocMint")]
    bloc_mint: String,
    #[serde(rename = "rewardVault")]
    reward_vault: String,
    #[serde(rename = "treasuryAuthority")]
    treasury_authority: String,
    #[serde(rename = "treasuryVault")]
    treasury_vault: String,
    #[serde(rename = "maxSupply")]
    max_supply: String,
    #[serde(rename = "currentBlockNumber")]
    current_block_number: String,
    #[serde(rename = "totalBlocksMined")]
    total_blocks_mined: String,
    #[serde(rename = "totalRewardsDistributed")]
    total_rewards_distributed: String,
    #[serde(rename = "totalTreasuryFeesDistributed")]
    total_treasury_fees_distributed: String,
    #[serde(rename = "initialBlockReward")]
    initial_block_reward: String,
    #[serde(rename = "treasuryFeeBps")]
    treasury_fee_bps: u16,
    #[serde(rename = "halvingInterval")]
    halving_interval: String,
    #[serde(rename = "targetBlockTimeSec")]
    target_block_time_sec: String,
    #[serde(rename = "adjustmentInterval")]
    adjustment_interval: String,
    #[serde(rename = "submitFeeLamports")]
    submit_fee_lamports: String,
    #[serde(rename = "blockTtlSec")]
    block_ttl_sec: String,
    #[serde(rename = "lastAdjustmentTimestamp")]
    last_adjustment_timestamp: String,
    #[serde(rename = "lastAdjustmentBlock")]
    last_adjustment_block: String,
    #[serde(rename = "difficultyBits")]
    difficulty_bits: u8,
    #[serde(rename = "minDifficultyBits")]
    min_difficulty_bits: u8,
    #[serde(rename = "maxDifficultyBits")]
    max_difficulty_bits: u8,
    #[serde(rename = "tokenDecimals")]
    token_decimals: u8,
    paused: bool,
    #[serde(rename = "vaultAuthorityBump")]
    vault_authority_bump: u8,
    #[serde(rename = "configBump")]
    config_bump: u8,
    #[serde(rename = "currentBlockBump")]
    current_block_bump: u8,
    #[serde(rename = "difficultyTargetHex")]
    difficulty_target_hex: String,
}

#[derive(Debug, Deserialize)]
struct RelayCurrentBlock {
    #[serde(rename = "blockNumber")]
    block_number: String,
    #[serde(rename = "challengeHex")]
    challenge_hex: String,
    #[serde(rename = "difficultyBits")]
    difficulty_bits: u8,
    status: u8,
    #[serde(rename = "difficultyTargetHex")]
    difficulty_target_hex: String,
    #[serde(rename = "blockReward")]
    block_reward: String,
    #[serde(rename = "openedAt")]
    opened_at: String,
    #[serde(rename = "expiresAt")]
    expires_at: String,
    winner: String,
    #[serde(rename = "winningNonce")]
    winning_nonce: String,
    #[serde(rename = "winningHashHex")]
    winning_hash_hex: String,
    #[serde(rename = "solvedAt")]
    solved_at: String,
}

impl RpcFacade {
    pub fn new(config: &CliConfig) -> Self {
        Self::from_parts(&config.rpc_url, config.program_id, config.commitment)
    }

    pub fn from_parts(rpc_url: &str, program_id: Pubkey, commitment: CommitmentConfig) -> Self {
        Self {
            client: RpcClient::new_with_commitment(rpc_url.to_string(), commitment),
            relay_client: Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .expect("failed to build relay HTTP client"),
            program_id,
        }
    }

    pub fn client(&self) -> &RpcClient {
        &self.client
    }

    pub fn fetch_protocol_config(&self) -> Result<ProtocolConfig> {
        Ok(self.fetch_relay_state()?.config.try_into()?)
    }

    pub fn fetch_current_block(&self) -> Result<CurrentBlock> {
        Ok(self.fetch_relay_state()?.current_block.try_into()?)
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
            .map_err(|error| explain_rpc_fetch_error(pubkey, &error.to_string()))?;
        let mut data = account.data.as_slice();
        T::try_deserialize(&mut data).context("anchor account deserialization failed")
    }

    pub fn get_account(&self, pubkey: &Pubkey) -> Result<Account> {
        self.client
            .get_account(pubkey)
            .map_err(|error| explain_rpc_fetch_error(pubkey, &error.to_string()))
    }

    pub fn account_exists(&self, pubkey: &Pubkey) -> Result<bool> {
        match self.get_account(pubkey) {
            Ok(_) => Ok(true),
            Err(error) if is_missing_account_error(&error.to_string()) => Ok(false),
            Err(error) => Err(error),
        }
    }

    pub fn get_balance(&self, pubkey: &Pubkey) -> Result<u64> {
        self.client
            .get_balance(pubkey)
            .with_context(|| format!("failed to fetch balance for {pubkey}"))
    }

    pub fn get_token_account_balance_raw(&self, pubkey: &Pubkey) -> Result<u64> {
        match self.client.get_token_account_balance(pubkey) {
            Ok(amount) => Ok(amount.amount.parse::<u64>().unwrap_or(0)),
            Err(error) if is_missing_account_error(&error.to_string()) => Ok(0),
            Err(error) => Err(anyhow::Error::new(error))
                .with_context(|| format!("failed to fetch token account balance for {pubkey}")),
        }
    }

    pub fn get_latest_blockhash(&self) -> Result<Hash> {
        self.client
            .get_latest_blockhash()
            .context("failed to fetch latest blockhash")
    }

    pub fn get_minimum_balance_for_rent_exemption(&self, data_len: usize) -> Result<u64> {
        self.client
            .get_minimum_balance_for_rent_exemption(data_len)
            .with_context(|| format!("failed to fetch rent exemption minimum for {data_len} bytes"))
    }

    pub fn get_fee_for_message(&self, message: &Message) -> Result<u64> {
        self.client
            .get_fee_for_message(message)
            .context("failed to fetch transaction fee preview")
    }

    pub fn send_and_confirm_transaction(&self, transaction: &Transaction) -> Result<Signature> {
        self.client
            .send_and_confirm_transaction(transaction)
            .context("failed to send and confirm transaction")
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

    fn fetch_relay_state(&self) -> Result<MinerStateRelayResponse> {
        let response = self
            .relay_client
            .get(MINER_STATE_RELAY_URL)
            .send()
            .and_then(|response| response.error_for_status())
            .context("failed to fetch the Blockmine relay state")?;
        let payload: MinerStateRelayResponse = response
            .json()
            .context("failed to decode the Blockmine relay response")?;

        if !payload.ok {
            anyhow::bail!("the Blockmine relay returned an invalid state response");
        }

        Ok(payload)
    }
}

impl TryFrom<RelayProtocolConfig> for ProtocolConfig {
    type Error = anyhow::Error;

    fn try_from(value: RelayProtocolConfig) -> Result<Self> {
        Ok(Self {
            admin: parse_pubkey(&value.admin, "config.admin")?,
            bloc_mint: parse_pubkey(&value.bloc_mint, "config.blocMint")?,
            reward_vault: parse_pubkey(&value.reward_vault, "config.rewardVault")?,
            treasury_authority: parse_pubkey(
                &value.treasury_authority,
                "config.treasuryAuthority",
            )?,
            treasury_vault: parse_pubkey(&value.treasury_vault, "config.treasuryVault")?,
            max_supply: parse_u64(&value.max_supply, "config.maxSupply")?,
            current_block_number: parse_u64(
                &value.current_block_number,
                "config.currentBlockNumber",
            )?,
            total_blocks_mined: parse_u64(&value.total_blocks_mined, "config.totalBlocksMined")?,
            total_rewards_distributed: parse_u64(
                &value.total_rewards_distributed,
                "config.totalRewardsDistributed",
            )?,
            total_treasury_fees_distributed: parse_u64(
                &value.total_treasury_fees_distributed,
                "config.totalTreasuryFeesDistributed",
            )?,
            initial_block_reward: parse_u64(
                &value.initial_block_reward,
                "config.initialBlockReward",
            )?,
            halving_interval: parse_u64(&value.halving_interval, "config.halvingInterval")?,
            target_block_time_sec: parse_u64(
                &value.target_block_time_sec,
                "config.targetBlockTimeSec",
            )?,
            adjustment_interval: parse_u64(
                &value.adjustment_interval,
                "config.adjustmentInterval",
            )?,
            submit_fee_lamports: parse_u64(
                &value.submit_fee_lamports,
                "config.submitFeeLamports",
            )?,
            block_ttl_sec: parse_i64(&value.block_ttl_sec, "config.blockTtlSec")?,
            last_adjustment_timestamp: parse_i64(
                &value.last_adjustment_timestamp,
                "config.lastAdjustmentTimestamp",
            )?,
            last_adjustment_block: parse_u64(
                &value.last_adjustment_block,
                "config.lastAdjustmentBlock",
            )?,
            difficulty_bits: value.difficulty_bits,
            min_difficulty_bits: value.min_difficulty_bits,
            max_difficulty_bits: value.max_difficulty_bits,
            token_decimals: value.token_decimals,
            paused: value.paused,
            vault_authority_bump: value.vault_authority_bump,
            config_bump: value.config_bump,
            current_block_bump: value.current_block_bump,
            treasury_fee_bps: value.treasury_fee_bps,
            difficulty_target: parse_fixed_hex::<32>(
                &value.difficulty_target_hex,
                "config.difficultyTargetHex",
            )?,
        })
    }
}

impl TryFrom<RelayCurrentBlock> for CurrentBlock {
    type Error = anyhow::Error;

    fn try_from(value: RelayCurrentBlock) -> Result<Self> {
        Ok(Self {
            block_number: parse_u64(&value.block_number, "currentBlock.blockNumber")?,
            challenge: parse_fixed_hex::<32>(&value.challenge_hex, "currentBlock.challengeHex")?,
            difficulty_bits: value.difficulty_bits,
            status: value.status,
            bump: 0,
            _padding0: [0u8; 5],
            difficulty_target: parse_fixed_hex::<32>(
                &value.difficulty_target_hex,
                "currentBlock.difficultyTargetHex",
            )?,
            block_reward: parse_u64(&value.block_reward, "currentBlock.blockReward")?,
            opened_at: parse_i64(&value.opened_at, "currentBlock.openedAt")?,
            expires_at: parse_i64(&value.expires_at, "currentBlock.expiresAt")?,
            winner: parse_pubkey(&value.winner, "currentBlock.winner")?,
            winning_nonce: parse_u64(&value.winning_nonce, "currentBlock.winningNonce")?,
            winning_hash: parse_fixed_hex::<32>(
                &value.winning_hash_hex,
                "currentBlock.winningHashHex",
            )?,
            solved_at: parse_i64(&value.solved_at, "currentBlock.solvedAt")?,
        })
    }
}

fn parse_pubkey(value: &str, label: &str) -> Result<Pubkey> {
    value
        .parse::<Pubkey>()
        .with_context(|| format!("invalid relay pubkey in {label}: {value}"))
}

fn parse_u64(value: &str, label: &str) -> Result<u64> {
    value
        .parse::<u64>()
        .with_context(|| format!("invalid relay u64 in {label}: {value}"))
}

fn parse_i64(value: &str, label: &str) -> Result<i64> {
    value
        .parse::<i64>()
        .with_context(|| format!("invalid relay i64 in {label}: {value}"))
}

fn parse_fixed_hex<const N: usize>(value: &str, label: &str) -> Result<[u8; N]> {
    let decoded = hex::decode(value).with_context(|| format!("invalid relay hex in {label}"))?;
    if decoded.len() != N {
        anyhow::bail!("invalid relay hex length in {label}: expected {N}, got {}", decoded.len());
    }

    let mut bytes = [0u8; N];
    bytes.copy_from_slice(&decoded);
    Ok(bytes)
}

fn explain_rpc_fetch_error(pubkey: &Pubkey, error_text: &str) -> anyhow::Error {
    if is_rpc_rejection_error(error_text) {
        anyhow::anyhow!(
            "RPC endpoint rejected the request while fetching account {}. The current transaction RPC is likely rate-limited or blocked. Original error: {}",
            pubkey,
            error_text
        )
    } else {
        anyhow::anyhow!("account {} not found. Original RPC error: {}", pubkey, error_text)
    }
}

fn is_rpc_rejection_error(error_text: &str) -> bool {
    let normalized = error_text.to_ascii_lowercase();
    normalized.contains("429")
        || normalized.contains("too many requests")
        || normalized.contains("403")
        || normalized.contains("forbidden")
}

fn is_missing_account_error(error_text: &str) -> bool {
    let normalized = error_text.to_ascii_lowercase();
    normalized.contains("accountnotfound")
        || normalized.contains("could not find account")
        || normalized.contains("account not found")
}
