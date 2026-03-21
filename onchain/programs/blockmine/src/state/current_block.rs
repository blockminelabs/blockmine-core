use anchor_lang::prelude::*;

#[account]
#[derive(InitSpace)]
pub struct CurrentBlock {
    pub block_number: u64,
    pub challenge: [u8; 32],
    pub difficulty_bits: u8,
    pub status: u8,
    pub bump: u8,
    pub _padding0: [u8; 5],
    pub difficulty_target: [u8; 32],
    pub block_reward: u64,
    pub opened_at: i64,
    pub expires_at: i64,
    pub winner: Pubkey,
    pub winning_nonce: u64,
    pub winning_hash: [u8; 32],
    pub solved_at: i64,
}
