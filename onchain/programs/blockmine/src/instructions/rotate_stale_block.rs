use anchor_lang::prelude::*;
use anchor_lang::solana_program::hash::hashv;

use crate::constants::{BLOCK_STATUS_OPEN, CONFIG_SEED, CURRENT_BLOCK_SEED};
use crate::errors::ErrorCode;
use crate::events::{BlockOpened, BlockStaleRotated, DifficultyAdjusted};
use crate::math::difficulty::target_from_difficulty_bits;
use crate::math::rewards::reward_era_for_open_block;
use crate::state::{CurrentBlock, ProtocolConfig};

use super::submit_solution::next_expiry;

#[derive(Accounts)]
pub struct RotateStaleBlock<'info> {
    #[account(mut)]
    pub caller: Signer<'info>,
    #[account(mut, seeds = [CONFIG_SEED], bump = config.config_bump)]
    pub config: Box<Account<'info, ProtocolConfig>>,
    #[account(mut, seeds = [CURRENT_BLOCK_SEED], bump = config.current_block_bump)]
    pub current_block: Box<Account<'info, CurrentBlock>>,
}

pub fn handler(ctx: Context<RotateStaleBlock>) -> Result<()> {
    let clock = Clock::get()?;
    let config = &mut ctx.accounts.config;
    let current_block = &mut ctx.accounts.current_block;

    require!(!config.paused, ErrorCode::ProtocolPaused);
    require!(
        current_block.status == BLOCK_STATUS_OPEN,
        ErrorCode::BlockClosed
    );
    require!(
        config.block_ttl_sec > 0,
        ErrorCode::StaleBlockRecoveryDisabled
    );
    require!(
        current_block.expires_at > 0,
        ErrorCode::StaleBlockRecoveryDisabled
    );
    require!(
        clock.unix_timestamp > current_block.expires_at,
        ErrorCode::BlockNotStale
    );
    let expected_open_reward = reward_era_for_open_block(config.total_blocks_mined);
    require!(
        current_block.block_reward == expected_open_reward.reward,
        ErrorCode::InvalidCurrentRewardState
    );

    let stale_block_number = current_block.block_number;
    let next_block_number = stale_block_number
        .checked_add(1)
        .ok_or(ErrorCode::MathOverflow)?;
    let previous_bits = config.difficulty_bits;
    let next_bits = config.min_difficulty_bits;
    let next_target = target_from_difficulty_bits(next_bits);
    let next_era = reward_era_for_open_block(config.total_blocks_mined);
    require!(next_era.reward > 0, ErrorCode::NoRewardsRemaining);
    let stale_for_seconds = (clock.unix_timestamp - current_block.opened_at).max(0) as u64;
    let next_challenge = hashv(&[
        b"blockmine-stale-rotate",
        &current_block.challenge,
        ctx.accounts.caller.key().as_ref(),
        &stale_block_number.to_le_bytes(),
        &next_block_number.to_le_bytes(),
        &clock.slot.to_le_bytes(),
        &clock.unix_timestamp.to_le_bytes(),
    ])
    .to_bytes();

    config.current_block_number = next_block_number;
    config.difficulty_bits = next_bits;
    config.difficulty_target = next_target;
    config.last_adjustment_timestamp = clock.unix_timestamp;
    config.last_adjustment_block = next_block_number;

    current_block.block_number = next_block_number;
    current_block.challenge = next_challenge;
    current_block.difficulty_bits = next_bits;
    current_block.status = BLOCK_STATUS_OPEN;
    current_block.difficulty_target = next_target;
    current_block.block_reward = next_era.reward;
    current_block.opened_at = clock.unix_timestamp;
    current_block.expires_at = next_expiry(clock.unix_timestamp, config.block_ttl_sec)?;
    current_block.winner = Pubkey::default();
    current_block.winning_nonce = 0;
    current_block.winning_hash = [0u8; 32];
    current_block.solved_at = 0;

    emit!(DifficultyAdjusted {
        block_number: next_block_number,
        previous_bits,
        next_bits,
        observed_seconds: stale_for_seconds,
        expected_seconds: config.target_block_time_sec.max(1),
    });

    emit!(BlockStaleRotated {
        stale_block_number,
        next_block_number,
        caller: ctx.accounts.caller.key(),
        previous_bits,
        next_bits,
        next_era_index: next_era.index,
        next_era_name: next_era.name,
        next_reward: next_era.reward,
        stale_for_seconds,
        rotated_at: clock.unix_timestamp,
    });

    emit!(BlockOpened {
        block_number: current_block.block_number,
        challenge: current_block.challenge,
        difficulty_bits: current_block.difficulty_bits,
        era_index: next_era.index,
        era_name: next_era.name,
        reward: current_block.block_reward,
        opened_at: current_block.opened_at,
    });

    Ok(())
}
