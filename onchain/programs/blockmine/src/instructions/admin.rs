use anchor_lang::prelude::*;
use anchor_lang::solana_program::hash::hashv;

use crate::constants::{CONFIG_SEED, CURRENT_BLOCK_SEED, FIXED_SUBMIT_FEE_LAMPORTS};
use crate::errors::ErrorCode;
use crate::events::{
    DifficultyConfigUpdated, PauseStateChanged, ProtocolReset, RuntimeConfigUpdated,
    TreasuryAccountsUpdated,
};
use crate::math::difficulty::target_from_difficulty_bits;
use crate::math::rewards::{reward_era_for_block, TOTAL_PROTOCOL_EMISSIONS};
use crate::state::{CurrentBlock, ProtocolConfig};

#[derive(Accounts)]
pub struct SetPaused<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(mut, seeds = [CONFIG_SEED], bump = config.config_bump)]
    pub config: Account<'info, ProtocolConfig>,
}

#[derive(Accounts)]
pub struct RotateAdmin<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(mut, seeds = [CONFIG_SEED], bump = config.config_bump)]
    pub config: Account<'info, ProtocolConfig>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct UpdateDifficultyParamsArgs {
    pub target_block_time_sec: u64,
    pub adjustment_interval: u64,
    pub difficulty_bits: u8,
    pub min_difficulty_bits: u8,
    pub max_difficulty_bits: u8,
}

#[derive(Accounts)]
pub struct UpdateDifficultyParams<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(mut, seeds = [CONFIG_SEED], bump = config.config_bump)]
    pub config: Account<'info, ProtocolConfig>,
    #[account(mut, seeds = [CURRENT_BLOCK_SEED], bump = config.current_block_bump)]
    pub current_block: Account<'info, CurrentBlock>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct UpdateRuntimeParamsArgs {
    pub block_ttl_sec: i64,
}

#[derive(Accounts)]
pub struct UpdateRuntimeParams<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(mut, seeds = [CONFIG_SEED], bump = config.config_bump)]
    pub config: Account<'info, ProtocolConfig>,
    #[account(mut, seeds = [CURRENT_BLOCK_SEED], bump = config.current_block_bump)]
    pub current_block: Account<'info, CurrentBlock>,
}

#[derive(Accounts)]
pub struct UpdateTreasuryAccounts<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(mut, seeds = [CONFIG_SEED], bump = config.config_bump)]
    pub config: Account<'info, ProtocolConfig>,
    /// CHECK: SOL treasury wallet chosen by the admin.
    pub treasury_authority: UncheckedAccount<'info>,
    #[account(
        constraint = treasury_vault.mint == config.bloc_mint @ ErrorCode::InvalidTreasuryVault,
        constraint = treasury_vault.owner == treasury_authority.key() @ ErrorCode::InvalidTreasuryVault
    )]
    pub treasury_vault: Account<'info, anchor_spl::token::TokenAccount>,
}

#[derive(Accounts)]
pub struct ResetProtocol<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(mut, seeds = [CONFIG_SEED], bump = config.config_bump)]
    pub config: Account<'info, ProtocolConfig>,
    #[account(mut, seeds = [CURRENT_BLOCK_SEED], bump = config.current_block_bump)]
    pub current_block: Account<'info, CurrentBlock>,
}

pub fn set_paused_handler(ctx: Context<SetPaused>, paused: bool) -> Result<()> {
    require_keys_eq!(
        ctx.accounts.admin.key(),
        ctx.accounts.config.admin,
        ErrorCode::Unauthorized
    );

    ctx.accounts.config.paused = paused;
    emit!(PauseStateChanged { paused });
    Ok(())
}

pub fn rotate_admin_handler(ctx: Context<RotateAdmin>, new_admin: Pubkey) -> Result<()> {
    require_keys_eq!(
        ctx.accounts.admin.key(),
        ctx.accounts.config.admin,
        ErrorCode::Unauthorized
    );

    ctx.accounts.config.admin = new_admin;
    Ok(())
}

pub fn update_difficulty_params_handler(
    ctx: Context<UpdateDifficultyParams>,
    args: UpdateDifficultyParamsArgs,
) -> Result<()> {
    require_keys_eq!(
        ctx.accounts.admin.key(),
        ctx.accounts.config.admin,
        ErrorCode::Unauthorized
    );
    require!(args.target_block_time_sec > 0, ErrorCode::InvalidAdjustmentInterval);
    require!(args.adjustment_interval > 0, ErrorCode::InvalidAdjustmentInterval);
    require!(
        args.min_difficulty_bits <= args.difficulty_bits
            && args.difficulty_bits <= args.max_difficulty_bits,
        ErrorCode::InvalidDifficulty
    );

    let clock = Clock::get()?;
    let next_target = target_from_difficulty_bits(args.difficulty_bits);
    let config = &mut ctx.accounts.config;
    config.target_block_time_sec = args.target_block_time_sec;
    config.adjustment_interval = args.adjustment_interval;
    config.difficulty_bits = args.difficulty_bits;
    config.min_difficulty_bits = args.min_difficulty_bits;
    config.max_difficulty_bits = args.max_difficulty_bits;
    config.difficulty_target = next_target;
    config.last_adjustment_timestamp = clock.unix_timestamp;
    config.last_adjustment_block = ctx.accounts.current_block.block_number;

    let current_block = &mut ctx.accounts.current_block;
    current_block.difficulty_bits = args.difficulty_bits;
    current_block.difficulty_target = next_target;

    emit!(DifficultyConfigUpdated {
        target_block_time_sec: args.target_block_time_sec,
        adjustment_interval: args.adjustment_interval,
        difficulty_bits: args.difficulty_bits,
        min_difficulty_bits: args.min_difficulty_bits,
        max_difficulty_bits: args.max_difficulty_bits,
    });
    Ok(())
}

pub fn update_runtime_params_handler(
    ctx: Context<UpdateRuntimeParams>,
    args: UpdateRuntimeParamsArgs,
) -> Result<()> {
    require_keys_eq!(
        ctx.accounts.admin.key(),
        ctx.accounts.config.admin,
        ErrorCode::Unauthorized
    );

    let config = &mut ctx.accounts.config;
    config.block_ttl_sec = args.block_ttl_sec;
    config.submit_fee_lamports = FIXED_SUBMIT_FEE_LAMPORTS;

    let current_block = &mut ctx.accounts.current_block;
    current_block.expires_at = if args.block_ttl_sec > 0 {
        current_block
            .opened_at
            .checked_add(args.block_ttl_sec)
            .ok_or(ErrorCode::MathOverflow)?
    } else {
        0
    };

    emit!(RuntimeConfigUpdated {
        submit_fee_lamports: FIXED_SUBMIT_FEE_LAMPORTS,
        block_ttl_sec: args.block_ttl_sec,
    });
    Ok(())
}

pub fn update_treasury_accounts_handler(ctx: Context<UpdateTreasuryAccounts>) -> Result<()> {
    require_keys_eq!(
        ctx.accounts.admin.key(),
        ctx.accounts.config.admin,
        ErrorCode::Unauthorized
    );

    let config = &mut ctx.accounts.config;
    config.treasury_authority = ctx.accounts.treasury_authority.key();
    config.treasury_vault = ctx.accounts.treasury_vault.key();

    emit!(TreasuryAccountsUpdated {
        treasury_authority: config.treasury_authority,
        treasury_vault: config.treasury_vault,
    });
    Ok(())
}

pub fn reset_protocol_handler(ctx: Context<ResetProtocol>) -> Result<()> {
    require_keys_eq!(
        ctx.accounts.admin.key(),
        ctx.accounts.config.admin,
        ErrorCode::Unauthorized
    );

    let clock = Clock::get()?;
    let config = &mut ctx.accounts.config;
    let current_block = &mut ctx.accounts.current_block;
    let genesis_era = reward_era_for_block(0);
    let genesis_challenge = hashv(&[
        b"blockmine-reset",
        ctx.accounts.admin.key().as_ref(),
        config.bloc_mint.as_ref(),
        &clock.slot.to_le_bytes(),
        &clock.unix_timestamp.to_le_bytes(),
    ])
    .to_bytes();

    config.current_block_number = 0;
    config.max_supply = TOTAL_PROTOCOL_EMISSIONS;
    config.total_blocks_mined = 0;
    config.total_rewards_distributed = 0;
    config.total_treasury_fees_distributed = 0;
    config.initial_block_reward = genesis_era.reward;
    config.last_adjustment_timestamp = clock.unix_timestamp;
    config.last_adjustment_block = 0;
    config.paused = false;

    current_block.block_number = 0;
    current_block.challenge = genesis_challenge;
    current_block.difficulty_bits = config.difficulty_bits;
    current_block.status = crate::constants::BLOCK_STATUS_OPEN;
    current_block.bump = config.current_block_bump;
    current_block._padding0 = [0u8; 5];
    current_block.difficulty_target = config.difficulty_target;
    current_block.block_reward = genesis_era.reward;
    current_block.opened_at = clock.unix_timestamp;
    current_block.expires_at = if config.block_ttl_sec > 0 {
        clock
            .unix_timestamp
            .checked_add(config.block_ttl_sec)
            .ok_or(ErrorCode::MathOverflow)?
    } else {
        0
    };
    current_block.winner = Pubkey::default();
    current_block.winning_nonce = 0;
    current_block.winning_hash = [0u8; 32];
    current_block.solved_at = 0;

    emit!(ProtocolReset {
        block_number: current_block.block_number,
        challenge: current_block.challenge,
        difficulty_bits: current_block.difficulty_bits,
        era_index: genesis_era.index,
        era_name: genesis_era.name,
        reward: current_block.block_reward,
        reset_at: clock.unix_timestamp,
    });

    Ok(())
}
