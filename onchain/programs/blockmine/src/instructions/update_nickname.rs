use anchor_lang::prelude::*;

use crate::constants::{MAX_NICKNAME_LEN, MINER_STATS_SEED};
use crate::events::NicknameUpdated;
use crate::state::MinerStats;

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, Default)]
pub struct UpdateNicknameArgs {
    pub nickname: [u8; MAX_NICKNAME_LEN],
}

#[derive(Accounts)]
pub struct UpdateNickname<'info> {
    #[account(mut)]
    pub miner: Signer<'info>,
    #[account(
        mut,
        seeds = [MINER_STATS_SEED, miner.key().as_ref()],
        bump = miner_stats.bump
    )]
    pub miner_stats: Account<'info, MinerStats>,
}

pub fn handler(ctx: Context<UpdateNickname>, args: UpdateNicknameArgs) -> Result<()> {
    ctx.accounts.miner_stats.nickname = args.nickname;

    emit!(NicknameUpdated {
        miner: ctx.accounts.miner.key(),
        nickname: args.nickname,
    });

    Ok(())
}
