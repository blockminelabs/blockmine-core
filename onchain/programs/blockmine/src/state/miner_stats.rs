use anchor_lang::prelude::*;

use crate::constants::MAX_NICKNAME_LEN;

#[account]
#[derive(InitSpace)]
pub struct MinerStats {
    pub miner: Pubkey,
    pub total_submissions: u64,
    pub valid_blocks_found: u64,
    pub total_rewards_earned: u64,
    pub pending_rewards: u64,
    pub claimed_rewards: u64,
    pub last_submission_time: i64,
    pub nickname: [u8; MAX_NICKNAME_LEN],
    pub bump: u8,
    pub _padding0: [u8; 7],
}

