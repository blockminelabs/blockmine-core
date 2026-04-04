use anchor_lang::prelude::*;

pub mod constants;
pub mod errors;
pub mod events;
pub mod instructions;
pub mod math;
pub mod state;

use instructions::*;

declare_id!("FgRe73gAkZPhxpiCFHMYMfLR4dabDaB1FDVFazVTcCtv");

#[program]
pub mod blockmine_program {
    use super::*;

    pub fn initialize_protocol(
        ctx: Context<InitializeProtocol>,
        args: InitializeProtocolArgs,
    ) -> Result<()> {
        initialize_protocol::handler(ctx, args)
    }

    pub fn register_miner(ctx: Context<RegisterMiner>, args: RegisterMinerArgs) -> Result<()> {
        register_miner::handler(ctx, args)
    }

    pub fn update_nickname(ctx: Context<UpdateNickname>, args: UpdateNicknameArgs) -> Result<()> {
        update_nickname::handler(ctx, args)
    }

    pub fn submit_solution(ctx: Context<SubmitSolution>, args: SubmitSolutionArgs) -> Result<()> {
        submit_solution::handler(ctx, args)
    }

    pub fn authorize_mining_session(
        ctx: Context<AuthorizeMiningSession>,
        args: AuthorizeMiningSessionArgs,
    ) -> Result<()> {
        session_mining::authorize_handler(ctx, args)
    }

    pub fn submit_solution_with_session(
        ctx: Context<SubmitSolutionWithSession>,
        args: SubmitSolutionArgs,
    ) -> Result<()> {
        session_mining::submit_with_session_handler(ctx, args)
    }

    pub fn rotate_stale_block(ctx: Context<RotateStaleBlock>) -> Result<()> {
        rotate_stale_block::handler(ctx)
    }

}
