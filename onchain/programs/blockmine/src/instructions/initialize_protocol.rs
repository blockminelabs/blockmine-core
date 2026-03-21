use anchor_lang::prelude::*;
use anchor_lang::solana_program::hash::hashv;
use anchor_spl::token::{Mint, TokenAccount};

use crate::constants::{
    BLOCK_STATUS_OPEN, CONFIG_SEED, CURRENT_BLOCK_SEED, FIXED_SUBMIT_FEE_LAMPORTS,
    VAULT_AUTHORITY_SEED,
};
use crate::errors::ErrorCode;
use crate::events::{BlockOpened, ProtocolInitialized};
use crate::math::difficulty::target_from_difficulty_bits;
use crate::math::rewards::{reward_era_for_block, TOTAL_PROTOCOL_EMISSIONS};
use crate::state::{CurrentBlock, ProtocolConfig};

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct InitializeProtocolArgs {
    pub max_supply: u64,
    pub initial_block_reward: u64,
    pub treasury_fee_bps: u16,
    pub halving_interval: u64,
    pub target_block_time_sec: u64,
    pub adjustment_interval: u64,
    pub initial_difficulty_bits: u8,
    pub min_difficulty_bits: u8,
    pub max_difficulty_bits: u8,
    pub submit_fee_lamports: u64,
    pub block_ttl_sec: i64,
    pub token_decimals: u8,
}

#[derive(Accounts)]
pub struct InitializeProtocol<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    pub bloc_mint: Box<Account<'info, Mint>>,
    #[account(
        init,
        payer = admin,
        seeds = [CONFIG_SEED],
        bump,
        space = 8 + ProtocolConfig::INIT_SPACE
    )]
    pub config: Box<Account<'info, ProtocolConfig>>,
    #[account(
        init,
        payer = admin,
        seeds = [CURRENT_BLOCK_SEED],
        bump,
        space = 8 + CurrentBlock::INIT_SPACE
    )]
    pub current_block: Box<Account<'info, CurrentBlock>>,
    /// CHECK: PDA authority for the reward vault ATA.
    #[account(seeds = [VAULT_AUTHORITY_SEED], bump)]
    pub vault_authority: UncheckedAccount<'info>,
    #[account(
        constraint = reward_vault.mint == bloc_mint.key() @ ErrorCode::InvalidRewardVault,
        constraint = reward_vault.owner == vault_authority.key() @ ErrorCode::InvalidRewardVault
    )]
    pub reward_vault: Box<Account<'info, TokenAccount>>,
    /// CHECK: Treasury authority can be any wallet chosen at init time.
    pub treasury_authority: UncheckedAccount<'info>,
    #[account(
        constraint = treasury_vault.mint == bloc_mint.key() @ ErrorCode::InvalidTreasuryVault,
        constraint = treasury_vault.owner == treasury_authority.key() @ ErrorCode::InvalidTreasuryVault
    )]
    pub treasury_vault: Box<Account<'info, TokenAccount>>,
    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<InitializeProtocol>, args: InitializeProtocolArgs) -> Result<()> {
    require!(args.halving_interval > 0, ErrorCode::InvalidHalvingInterval);
    require!(args.adjustment_interval > 0, ErrorCode::InvalidAdjustmentInterval);
    require!(args.treasury_fee_bps == 100, ErrorCode::InvalidTreasuryFee);
    require!(
        args.submit_fee_lamports == FIXED_SUBMIT_FEE_LAMPORTS,
        ErrorCode::InvalidSubmitFee
    );
    require!(
        args.min_difficulty_bits <= args.initial_difficulty_bits
            && args.initial_difficulty_bits <= args.max_difficulty_bits,
        ErrorCode::InvalidDifficulty
    );

    let clock = Clock::get()?;
    let target = target_from_difficulty_bits(args.initial_difficulty_bits);
    let initial_era = reward_era_for_block(0);
    let challenge = hashv(&[
        b"blockmine-genesis",
        ctx.accounts.admin.key().as_ref(),
        ctx.accounts.bloc_mint.key().as_ref(),
        &clock.slot.to_le_bytes(),
        &clock.unix_timestamp.to_le_bytes(),
    ])
    .to_bytes();

    let config = &mut ctx.accounts.config;
    config.admin = ctx.accounts.admin.key();
    config.bloc_mint = ctx.accounts.bloc_mint.key();
    config.reward_vault = ctx.accounts.reward_vault.key();
    config.treasury_authority = ctx.accounts.treasury_authority.key();
    config.treasury_vault = ctx.accounts.treasury_vault.key();
    config.max_supply = TOTAL_PROTOCOL_EMISSIONS;
    config.current_block_number = 0;
    config.total_blocks_mined = 0;
    config.total_rewards_distributed = 0;
    config.total_treasury_fees_distributed = 0;
    config.initial_block_reward = initial_era.reward;
    config.treasury_fee_bps = args.treasury_fee_bps;
    config.halving_interval = args.halving_interval;
    config.target_block_time_sec = args.target_block_time_sec;
    config.adjustment_interval = args.adjustment_interval;
    config.submit_fee_lamports = args.submit_fee_lamports;
    config.block_ttl_sec = args.block_ttl_sec;
    config.last_adjustment_timestamp = clock.unix_timestamp;
    config.last_adjustment_block = 0;
    config.difficulty_bits = args.initial_difficulty_bits;
    config.min_difficulty_bits = args.min_difficulty_bits;
    config.max_difficulty_bits = args.max_difficulty_bits;
    config.token_decimals = args.token_decimals;
    config.paused = false;
    config.vault_authority_bump = ctx.bumps.vault_authority;
    config.config_bump = ctx.bumps.config;
    config.current_block_bump = ctx.bumps.current_block;
    config.difficulty_target = target;

    let current_block = &mut ctx.accounts.current_block;
    current_block.block_number = 0;
    current_block.challenge = challenge;
    current_block.difficulty_bits = args.initial_difficulty_bits;
    current_block.status = BLOCK_STATUS_OPEN;
    current_block.bump = ctx.bumps.current_block;
    current_block._padding0 = [0u8; 5];
    current_block.difficulty_target = target;
    current_block.block_reward = initial_era.reward;
    current_block.opened_at = clock.unix_timestamp;
    current_block.expires_at = if args.block_ttl_sec > 0 {
        clock
            .unix_timestamp
            .checked_add(args.block_ttl_sec)
            .ok_or(ErrorCode::MathOverflow)?
    } else {
        0
    };
    current_block.winner = Pubkey::default();
    current_block.winning_nonce = 0;
    current_block.winning_hash = [0u8; 32];
    current_block.solved_at = 0;

    emit!(ProtocolInitialized {
        admin: config.admin,
        bloc_mint: config.bloc_mint,
        reward_vault: config.reward_vault,
        treasury_authority: config.treasury_authority,
        treasury_vault: config.treasury_vault,
        initial_reward: config.initial_block_reward,
        initial_era_index: initial_era.index,
        initial_era_name: initial_era.name,
        treasury_fee_bps: config.treasury_fee_bps,
        initial_difficulty_bits: config.difficulty_bits,
    });

    emit!(BlockOpened {
        block_number: current_block.block_number,
        challenge: current_block.challenge,
        difficulty_bits: current_block.difficulty_bits,
        era_index: initial_era.index,
        era_name: initial_era.name,
        reward: current_block.block_reward,
        opened_at: current_block.opened_at,
    });

    Ok(())
}
