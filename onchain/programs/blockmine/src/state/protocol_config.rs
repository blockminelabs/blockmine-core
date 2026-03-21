use anchor_lang::prelude::*;

#[account]
#[derive(InitSpace)]
pub struct ProtocolConfig {
    pub admin: Pubkey,
    pub bloc_mint: Pubkey,
    pub reward_vault: Pubkey,
    pub treasury_authority: Pubkey,
    pub treasury_vault: Pubkey,
    pub max_supply: u64,
    pub current_block_number: u64,
    pub total_blocks_mined: u64,
    pub total_rewards_distributed: u64,
    pub total_treasury_fees_distributed: u64,
    pub initial_block_reward: u64,
    pub halving_interval: u64,
    pub target_block_time_sec: u64,
    pub adjustment_interval: u64,
    pub submit_fee_lamports: u64,
    pub block_ttl_sec: i64,
    pub last_adjustment_timestamp: i64,
    pub last_adjustment_block: u64,
    pub difficulty_bits: u8,
    pub min_difficulty_bits: u8,
    pub max_difficulty_bits: u8,
    pub token_decimals: u8,
    pub paused: bool,
    pub vault_authority_bump: u8,
    pub config_bump: u8,
    pub current_block_bump: u8,
    pub treasury_fee_bps: u16,
    pub difficulty_target: [u8; 32],
}
