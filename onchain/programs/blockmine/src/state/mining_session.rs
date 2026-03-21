use anchor_lang::prelude::*;

#[account]
#[derive(InitSpace)]
pub struct MiningSession {
    pub miner: Pubkey,
    pub delegate: Pubkey,
    pub expires_at: i64,
    pub max_submissions: u64,
    pub submissions_used: u64,
    pub created_at: i64,
    pub last_used_at: i64,
    pub bump: u8,
    pub active: bool,
    pub _padding0: [u8; 6],
}
