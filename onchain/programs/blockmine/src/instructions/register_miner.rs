use anchor_lang::prelude::*;

use crate::constants::{MAX_NICKNAME_LEN, MINER_STATS_SEED};
use crate::events::MinerRegistered;
use crate::state::MinerStats;

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, Default)]
pub struct RegisterMinerArgs {
    pub nickname: [u8; MAX_NICKNAME_LEN],
}

#[derive(Accounts)]
pub struct RegisterMiner<'info> {
    #[account(mut)]
    pub miner: Signer<'info>,
    #[account(
        init_if_needed,
        payer = miner,
        seeds = [MINER_STATS_SEED, miner.key().as_ref()],
        bump,
        space = 8 + MinerStats::INIT_SPACE
    )]
    pub miner_stats: Account<'info, MinerStats>,
    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<RegisterMiner>, args: RegisterMinerArgs) -> Result<()> {
    let miner_stats = &mut ctx.accounts.miner_stats;

    if miner_stats.miner == Pubkey::default() {
        miner_stats.miner = ctx.accounts.miner.key();
        miner_stats.total_submissions = 0;
        miner_stats.valid_blocks_found = 0;
        miner_stats.total_rewards_earned = 0;
        miner_stats.pending_rewards = 0;
        miner_stats.claimed_rewards = 0;
        miner_stats.last_submission_time = 0;
        miner_stats.nickname = args.nickname;
        miner_stats.bump = ctx.bumps.miner_stats;
        miner_stats._padding0 = [0u8; 7];

        emit!(MinerRegistered {
            miner: ctx.accounts.miner.key(),
        });
    } else if args.nickname != [0u8; MAX_NICKNAME_LEN] {
        miner_stats.nickname = args.nickname;
    }

    Ok(())
}
