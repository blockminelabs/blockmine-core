use anchor_lang::prelude::*;

#[account]
#[derive(InitSpace)]
pub struct BlockHistory {
    pub block_number: u64,
    pub winner: Pubkey,
    pub reward: u64,
    pub hash: [u8; 32],
    pub nonce: u64,
    pub timestamp: i64,
    pub difficulty_bits: u8,
    pub bump: u8,
    pub _padding0: [u8; 6],
    pub difficulty_target: [u8; 32],
    pub challenge: [u8; 32],
}
