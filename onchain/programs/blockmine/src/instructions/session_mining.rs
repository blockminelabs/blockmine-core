use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};

use crate::constants::{
    BLOCK_HISTORY_SEED, CONFIG_SEED, CURRENT_BLOCK_SEED, MINER_STATS_SEED, MINING_SESSION_SEED,
    VAULT_AUTHORITY_SEED,
};
use crate::errors::ErrorCode;
use crate::events::MiningSessionAuthorized;
use crate::state::{BlockHistory, CurrentBlock, MinerStats, MiningSession, ProtocolConfig};

use super::submit_solution::{process_submission, SubmitSolutionArgs};

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct AuthorizeMiningSessionArgs {
    pub delegate: Pubkey,
    pub expires_at: i64,
    pub max_submissions: u64,
}

#[derive(Accounts)]
pub struct AuthorizeMiningSession<'info> {
    #[account(mut)]
    pub miner: Signer<'info>,
    #[account(
        init_if_needed,
        payer = miner,
        seeds = [MINING_SESSION_SEED, miner.key().as_ref()],
        bump,
        space = 8 + MiningSession::INIT_SPACE
    )]
    pub mining_session: Account<'info, MiningSession>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct SubmitSolutionWithSession<'info> {
    #[account(mut)]
    pub delegate: Signer<'info>,
    /// CHECK: beneficiary wallet for rewards, stats, and proof binding.
    pub miner: UncheckedAccount<'info>,
    #[account(
        mut,
        seeds = [MINING_SESSION_SEED, miner.key().as_ref()],
        bump = mining_session.bump,
        constraint = mining_session.miner == miner.key() @ ErrorCode::InvalidSessionMiner,
        constraint = mining_session.delegate == delegate.key() @ ErrorCode::InvalidSessionDelegate,
        constraint = mining_session.active @ ErrorCode::SessionInactive
    )]
    pub mining_session: Box<Account<'info, MiningSession>>,
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
    #[account(
        init,
        payer = delegate,
        seeds = [BLOCK_HISTORY_SEED, &current_block.block_number.to_le_bytes()],
        bump,
        space = 8 + BlockHistory::INIT_SPACE
    )]
    pub block_history: Box<Account<'info, BlockHistory>>,
    #[account(address = config.bloc_mint @ ErrorCode::InvalidMint)]
    pub bloc_mint: Box<Account<'info, Mint>>,
    #[account(
        mut,
        address = config.reward_vault @ ErrorCode::InvalidRewardVault,
        token::mint = bloc_mint
    )]
    pub reward_vault: Box<Account<'info, TokenAccount>>,
    #[account(
        mut,
        address = config.treasury_vault @ ErrorCode::InvalidTreasuryVault,
        token::mint = bloc_mint
    )]
    pub treasury_vault: Box<Account<'info, TokenAccount>>,
    /// CHECK: SOL treasury wallet owned by the dev treasury.
    #[account(mut, address = config.treasury_authority @ ErrorCode::Unauthorized)]
    pub treasury_authority: UncheckedAccount<'info>,
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

pub fn authorize_handler(
    ctx: Context<AuthorizeMiningSession>,
    args: AuthorizeMiningSessionArgs,
) -> Result<()> {
    let clock = Clock::get()?;
    require!(
        args.delegate != Pubkey::default(),
        ErrorCode::InvalidSessionDelegate
    );
    require!(
        args.expires_at == 0 || args.expires_at > clock.unix_timestamp,
        ErrorCode::InvalidSessionExpiry
    );

    let mining_session = &mut ctx.accounts.mining_session;
    mining_session.miner = ctx.accounts.miner.key();
    mining_session.delegate = args.delegate;
    mining_session.expires_at = args.expires_at;
    mining_session.max_submissions = args.max_submissions;
    mining_session.submissions_used = 0;
    mining_session.created_at = clock.unix_timestamp;
    mining_session.last_used_at = 0;
    mining_session.bump = ctx.bumps.mining_session;
    mining_session.active = true;
    mining_session._padding0 = [0u8; 6];

    emit!(MiningSessionAuthorized {
        miner: mining_session.miner,
        delegate: mining_session.delegate,
        expires_at: mining_session.expires_at,
        max_submissions: mining_session.max_submissions,
    });

    Ok(())
}

pub fn submit_with_session_handler(
    ctx: Context<SubmitSolutionWithSession>,
    args: SubmitSolutionArgs,
) -> Result<()> {
    let clock = Clock::get()?;
    let session = &mut ctx.accounts.mining_session;

    require!(session.active, ErrorCode::SessionInactive);
    if session.expires_at > 0 {
        require!(
            clock.unix_timestamp <= session.expires_at,
            ErrorCode::SessionExpired
        );
    }
    if session.max_submissions > 0 {
        require!(
            session.submissions_used < session.max_submissions,
            ErrorCode::SessionInactive
        );
    }

    let block_history_bump = ctx.bumps.block_history;
    process_submission(
        ctx.accounts.miner.key(),
        args.nonce,
        block_history_bump,
        &mut ctx.accounts.config,
        &mut ctx.accounts.current_block,
        &mut ctx.accounts.miner_stats,
        &mut ctx.accounts.block_history,
        &ctx.accounts.bloc_mint,
        &mut ctx.accounts.reward_vault,
        &ctx.accounts.treasury_authority,
        &mut ctx.accounts.treasury_vault,
        &ctx.accounts.vault_authority,
        &mut ctx.accounts.miner_token_account,
        &ctx.accounts.token_program,
        &ctx.accounts.system_program,
        &ctx.accounts.delegate.to_account_info(),
    )?;

    session.submissions_used = session
        .submissions_used
        .checked_add(1)
        .ok_or(ErrorCode::MathOverflow)?;
    session.last_used_at = clock.unix_timestamp;
    if session.max_submissions > 0 && session.submissions_used >= session.max_submissions {
        session.active = false;
    }

    Ok(())
}
