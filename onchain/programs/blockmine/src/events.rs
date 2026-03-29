use anchor_lang::prelude::*;

#[event]
pub struct ProtocolInitialized {
    pub admin: Pubkey,
    pub bloc_mint: Pubkey,
    pub reward_vault: Pubkey,
    pub treasury_authority: Pubkey,
    pub treasury_vault: Pubkey,
    pub initial_reward: u64,
    pub initial_era_index: u8,
    pub initial_era_name: [u8; 16],
    pub treasury_fee_bps: u16,
    pub initial_difficulty_bits: u8,
}

#[event]
pub struct BlockOpened {
    pub block_number: u64,
    pub challenge: [u8; 32],
    pub difficulty_bits: u8,
    pub era_index: u8,
    pub era_name: [u8; 16],
    pub reward: u64,
    pub opened_at: i64,
}

#[event]
pub struct BlockSolved {
    pub block_number: u64,
    pub winner: Pubkey,
    pub nonce: u64,
    pub hash: [u8; 32],
    pub challenge: [u8; 32],
    pub difficulty_bits: u8,
    pub difficulty_target: [u8; 32],
    pub era_index: u8,
    pub era_name: [u8; 16],
    pub reward: u64,
    pub miner_reward: u64,
    pub treasury_fee: u64,
    pub submit_fee_lamports: u64,
    pub solved_at: i64,
}

#[event]
pub struct DifficultyAdjusted {
    pub block_number: u64,
    pub previous_bits: u8,
    pub next_bits: u8,
    pub observed_seconds: u64,
    pub expected_seconds: u64,
}

#[event]
pub struct BlockStaleRotated {
    pub stale_block_number: u64,
    pub next_block_number: u64,
    pub caller: Pubkey,
    pub previous_bits: u8,
    pub next_bits: u8,
    pub next_era_index: u8,
    pub next_era_name: [u8; 16],
    pub next_reward: u64,
    pub stale_for_seconds: u64,
    pub rotated_at: i64,
}

#[event]
pub struct MinerRegistered {
    pub miner: Pubkey,
}

#[event]
pub struct NicknameUpdated {
    pub miner: Pubkey,
    pub nickname: [u8; 32],
}

#[event]
pub struct PauseStateChanged {
    pub paused: bool,
}

#[event]
pub struct DifficultyConfigUpdated {
    pub target_block_time_sec: u64,
    pub adjustment_interval: u64,
    pub difficulty_bits: u8,
    pub min_difficulty_bits: u8,
    pub max_difficulty_bits: u8,
}

#[event]
pub struct RuntimeConfigUpdated {
    pub submit_fee_lamports: u64,
    pub block_ttl_sec: i64,
}

#[event]
pub struct TreasuryAccountsUpdated {
    pub treasury_authority: Pubkey,
    pub treasury_vault: Pubkey,
}

#[event]
pub struct ProtocolReset {
    pub block_number: u64,
    pub challenge: [u8; 32],
    pub difficulty_bits: u8,
    pub era_index: u8,
    pub era_name: [u8; 16],
    pub reward: u64,
    pub reset_at: i64,
}

#[event]
pub struct MiningSessionAuthorized {
    pub miner: Pubkey,
    pub delegate: Pubkey,
    pub expires_at: i64,
    pub max_submissions: u64,
}
