use anchor_lang::prelude::*;
use anchor_lang::solana_program::hash::hashv;
use anchor_lang::system_program::{self, Transfer as SystemTransfer};
use anchor_spl::token::{self, Mint, Token, TokenAccount, TransferChecked};

use crate::constants::{
    BLOCK_STATUS_CLOSED, BLOCK_STATUS_OPEN, CONFIG_SEED, CURRENT_BLOCK_SEED, MINER_STATS_SEED,
    VAULT_AUTHORITY_SEED,
};
use crate::errors::ErrorCode;
use crate::events::{BlockOpened, BlockSolved, DifficultyAdjusted};
use crate::math::difficulty::{calculate_next_difficulty, hash_meets_target};
use crate::math::rewards::reward_era_for_open_block;
use crate::state::{CurrentBlock, MinerStats, ProtocolConfig};

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct SubmitSolutionArgs {
    pub nonce: u64,
}

#[derive(Accounts)]
pub struct SubmitSolution<'info> {
    #[account(mut)]
    pub miner: Signer<'info>,
    #[account(mut, seeds = [CONFIG_SEED], bump = config.config_bump)]
    pub config: Box<Account<'info, ProtocolConfig>>,
    #[account(mut, seeds = [CURRENT_BLOCK_SEED], bump = config.current_block_bump)]
    pub current_block: Box<Account<'info, CurrentBlock>>,
    #[account(
        mut,
        seeds = [MINER_STATS_SEED, miner.key().as_ref()],
        bump = miner_stats.bump
    )]
    pub miner_stats: Box<Account<'info, MinerStats>>,
    #[account(address = config.bloc_mint @ ErrorCode::InvalidMint)]
    pub bloc_mint: Box<Account<'info, Mint>>,
    #[account(
        mut,
        address = config.reward_vault @ ErrorCode::InvalidRewardVault,
        token::mint = bloc_mint
    )]
    pub reward_vault: Box<Account<'info, TokenAccount>>,
    /// CHECK: SOL treasury wallet owned by the dev treasury.
    #[account(mut, address = config.treasury_authority @ ErrorCode::Unauthorized)]
    pub treasury_authority: UncheckedAccount<'info>,
    #[account(
        mut,
        address = config.treasury_vault @ ErrorCode::InvalidTreasuryVault,
        token::mint = bloc_mint
    )]
    pub treasury_vault: Box<Account<'info, TokenAccount>>,
    /// CHECK: PDA signer for token transfers.
    #[account(seeds = [VAULT_AUTHORITY_SEED], bump = config.vault_authority_bump)]
    pub vault_authority: UncheckedAccount<'info>,
    #[account(
        mut,
        token::mint = bloc_mint,
        token::authority = miner
    )]
    pub miner_token_account: Box<Account<'info, TokenAccount>>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<SubmitSolution>, args: SubmitSolutionArgs) -> Result<()> {
    process_submission(
        ctx.accounts.miner.key(),
        args.nonce,
        &mut ctx.accounts.config,
        &mut ctx.accounts.current_block,
        &mut ctx.accounts.miner_stats,
        &ctx.accounts.bloc_mint,
        &mut ctx.accounts.reward_vault,
        &ctx.accounts.treasury_authority,
        &mut ctx.accounts.treasury_vault,
        &ctx.accounts.vault_authority,
        &mut ctx.accounts.miner_token_account,
        &ctx.accounts.token_program,
        &ctx.accounts.system_program,
        &ctx.accounts.miner.to_account_info(),
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn process_submission<'info>(
    miner: Pubkey,
    nonce: u64,
    config: &mut Account<'info, ProtocolConfig>,
    current_block: &mut Account<'info, CurrentBlock>,
    miner_stats: &mut Account<'info, MinerStats>,
    bloc_mint: &Account<'info, Mint>,
    reward_vault: &mut Account<'info, TokenAccount>,
    treasury_authority: &UncheckedAccount<'info>,
    treasury_vault: &mut Account<'info, TokenAccount>,
    vault_authority: &UncheckedAccount<'info>,
    miner_token_account: &mut Account<'info, TokenAccount>,
    token_program: &Program<'info, Token>,
    system_program: &Program<'info, System>,
    fee_payer: &AccountInfo<'info>,
) -> Result<()> {
    let clock = Clock::get()?;

    require!(!config.paused, ErrorCode::ProtocolPaused);
    require!(
        current_block.status == BLOCK_STATUS_OPEN,
        ErrorCode::BlockClosed
    );
    if current_block.expires_at > 0 {
        require!(
            clock.unix_timestamp <= current_block.expires_at,
            ErrorCode::BlockExpired
        );
    }

    let block_number = current_block.block_number;
    let current_challenge = current_block.challenge;
    let current_reward = current_block.block_reward;
    let current_target = current_block.difficulty_target;
    let current_bits = current_block.difficulty_bits;
    let current_era = reward_era_for_open_block(config.total_blocks_mined);
    require!(
        current_reward == current_era.reward,
        ErrorCode::InvalidCurrentRewardState
    );

    let hash = hashv(&[&current_challenge, miner.as_ref(), &nonce.to_le_bytes()]).to_bytes();

    require!(
        hash_meets_target(&hash, &current_target),
        ErrorCode::InvalidSolution
    );
    require!(current_reward > 0, ErrorCode::NoRewardsRemaining);
    require!(
        reward_vault.amount >= current_reward,
        ErrorCode::InsufficientVaultBalance
    );

    if config.submit_fee_lamports > 0 {
        system_program::transfer(
            CpiContext::new(
                system_program.to_account_info(),
                SystemTransfer {
                    from: fee_payer.clone(),
                    to: treasury_authority.to_account_info(),
                },
            ),
            config.submit_fee_lamports,
        )?;
    }

    let treasury_fee = ((current_reward as u128)
        .checked_mul(config.treasury_fee_bps as u128)
        .ok_or(ErrorCode::MathOverflow)?
        / 10_000u128) as u64;
    let miner_reward = current_reward
        .checked_sub(treasury_fee)
        .ok_or(ErrorCode::MathOverflow)?;

    let vault_signer: &[&[u8]] = &[VAULT_AUTHORITY_SEED, &[config.vault_authority_bump]];
    let cpi_accounts = TransferChecked {
        from: reward_vault.to_account_info(),
        mint: bloc_mint.to_account_info(),
        to: miner_token_account.to_account_info(),
        authority: vault_authority.to_account_info(),
    };
    token::transfer_checked(
        CpiContext::new_with_signer(
            token_program.to_account_info(),
            cpi_accounts,
            &[vault_signer],
        ),
        miner_reward,
        config.token_decimals,
    )?;

    if treasury_fee > 0 {
        let treasury_transfer = TransferChecked {
            from: reward_vault.to_account_info(),
            mint: bloc_mint.to_account_info(),
            to: treasury_vault.to_account_info(),
            authority: vault_authority.to_account_info(),
        };
        token::transfer_checked(
            CpiContext::new_with_signer(
                token_program.to_account_info(),
                treasury_transfer,
                &[vault_signer],
            ),
            treasury_fee,
            config.token_decimals,
        )?;
    }

    miner_stats.total_submissions = miner_stats
        .total_submissions
        .checked_add(1)
        .ok_or(ErrorCode::MathOverflow)?;
    miner_stats.valid_blocks_found = miner_stats
        .valid_blocks_found
        .checked_add(1)
        .ok_or(ErrorCode::MathOverflow)?;
    miner_stats.total_rewards_earned = miner_stats
        .total_rewards_earned
        .checked_add(miner_reward)
        .ok_or(ErrorCode::MathOverflow)?;
    miner_stats.last_submission_time = clock.unix_timestamp;

    current_block.status = BLOCK_STATUS_CLOSED;
    current_block.winner = miner;
    current_block.winning_nonce = nonce;
    current_block.winning_hash = hash;
    current_block.solved_at = clock.unix_timestamp;

    emit!(BlockSolved {
        block_number,
        winner: miner,
        nonce,
        hash,
        challenge: current_challenge,
        difficulty_bits: current_bits,
        difficulty_target: current_target,
        era_index: current_era.index,
        era_name: current_era.name,
        reward: current_reward,
        miner_reward,
        treasury_fee,
        submit_fee_lamports: config.submit_fee_lamports,
        solved_at: clock.unix_timestamp,
    });

    let solved_blocks_after = config
        .total_blocks_mined
        .checked_add(1)
        .ok_or(ErrorCode::MathOverflow)?;
    config.total_blocks_mined = solved_blocks_after;
    config.total_rewards_distributed = config
        .total_rewards_distributed
        .checked_add(current_reward)
        .ok_or(ErrorCode::MathOverflow)?;
    config.total_treasury_fees_distributed = config
        .total_treasury_fees_distributed
        .checked_add(treasury_fee)
        .ok_or(ErrorCode::MathOverflow)?;

    let next_block_number = block_number.checked_add(1).ok_or(ErrorCode::MathOverflow)?;
    let observed_seconds = (clock.unix_timestamp - current_block.opened_at).max(1) as u64;
    let expected_seconds = config.target_block_time_sec.max(1);
    let previous_bits = config.difficulty_bits;
    let adjustment = calculate_next_difficulty(
        config.difficulty_target,
        observed_seconds,
        expected_seconds,
        config.min_difficulty_bits,
        config.max_difficulty_bits,
    );
    config.difficulty_bits = adjustment.difficulty_bits;
    config.difficulty_target = adjustment.target;
    config.last_adjustment_timestamp = clock.unix_timestamp;
    config.last_adjustment_block = next_block_number;

    if adjustment.changed {
        emit!(DifficultyAdjusted {
            block_number: next_block_number,
            previous_bits,
            next_bits: adjustment.difficulty_bits,
            observed_seconds,
            expected_seconds,
        });
    }

    let next_era = reward_era_for_open_block(solved_blocks_after);
    let next_reward = next_era.reward;
    let next_challenge = hashv(&[
        b"blockmine-next",
        &hash,
        &current_challenge,
        miner.as_ref(),
        &nonce.to_le_bytes(),
        &next_block_number.to_le_bytes(),
        &clock.slot.to_le_bytes(),
        &clock.unix_timestamp.to_le_bytes(),
    ])
    .to_bytes();

    config.current_block_number = next_block_number;
    current_block.block_number = next_block_number;
    current_block.difficulty_bits = config.difficulty_bits;
    current_block.difficulty_target = config.difficulty_target;
    current_block.winner = Pubkey::default();
    current_block.winning_nonce = 0;
    current_block.winning_hash = [0u8; 32];
    current_block.solved_at = 0;

    if next_reward > 0 {
        current_block.challenge = next_challenge;
        current_block.status = BLOCK_STATUS_OPEN;
        current_block.block_reward = next_reward;
        current_block.opened_at = clock.unix_timestamp;
        current_block.expires_at = next_expiry(clock.unix_timestamp, config.block_ttl_sec)?;

        emit!(BlockOpened {
            block_number: current_block.block_number,
            challenge: current_block.challenge,
            difficulty_bits: current_block.difficulty_bits,
            era_index: next_era.index,
            era_name: next_era.name,
            reward: current_block.block_reward,
            opened_at: current_block.opened_at,
        });
    } else {
        current_block.challenge = [0u8; 32];
        current_block.status = BLOCK_STATUS_CLOSED;
        current_block.block_reward = 0;
        current_block.opened_at = clock.unix_timestamp;
        current_block.expires_at = 0;
    }

    Ok(())
}

pub(crate) fn next_expiry(opened_at: i64, block_ttl_sec: i64) -> Result<i64> {
    if block_ttl_sec > 0 {
        opened_at
            .checked_add(block_ttl_sec)
            .ok_or_else(|| error!(ErrorCode::MathOverflow))
    } else {
        Ok(0)
    }
}
