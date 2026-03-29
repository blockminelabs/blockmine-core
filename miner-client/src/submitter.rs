use anchor_lang::{InstructionData, ToAccountMetas};
use anyhow::{Context, Result};
use blockmine_program::instruction::{
    InitializeProtocol, RegisterMiner, RotateStaleBlock, SubmitSolution, SubmitSolutionWithSession,
};
use blockmine_program::instructions::{
    InitializeProtocolArgs, RegisterMinerArgs, SubmitSolutionArgs,
};
use solana_client::rpc_config::RpcSimulateTransactionConfig;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    instruction::Instruction,
    signature::{Keypair, Signature, Signer},
    transaction::Transaction,
};
use spl_associated_token_account::{
    get_associated_token_address, instruction::create_associated_token_account_idempotent,
};

use crate::rpc::RpcFacade;

pub fn initialize_protocol(
    rpc: &RpcFacade,
    signer: &Keypair,
    mint: Pubkey,
    treasury_authority: Pubkey,
    args: InitializeProtocolArgs,
) -> Result<Signature> {
    let config_pda = rpc.config_pda().0;
    let current_block_pda = rpc.current_block_pda().0;
    let vault_authority_pda = rpc.vault_authority_pda().0;
    let reward_vault = ensure_associated_token_account(rpc, signer, vault_authority_pda, mint)
        .context("failed to create or load reward vault ATA")?;
    let treasury_vault = ensure_associated_token_account(rpc, signer, treasury_authority, mint)
        .context("failed to create or load treasury vault ATA")?;

    let accounts = blockmine_program::accounts::InitializeProtocol {
        admin: signer.pubkey(),
        bloc_mint: mint,
        config: config_pda,
        current_block: current_block_pda,
        vault_authority: vault_authority_pda,
        reward_vault,
        treasury_authority,
        treasury_vault,
        system_program: solana_sdk::system_program::ID,
    };
    let instruction = Instruction {
        program_id: rpc.program_id,
        accounts: accounts.to_account_metas(None),
        data: InitializeProtocol { args }.data(),
    };

    send_instruction(rpc, signer, instruction).context("initialize_protocol transaction failed")
}

pub fn submit_solution(rpc: &RpcFacade, signer: &Keypair, nonce: u64) -> Result<Signature> {
    let config_pda = rpc.config_pda().0;
    let current_block_pda = rpc.current_block_pda().0;
    let miner_stats_pda = rpc.miner_stats_pda(&signer.pubkey()).0;
    let vault_authority_pda = rpc.vault_authority_pda().0;
    let protocol_config = rpc.fetch_protocol_config()?;
    let miner_token_account =
        ensure_associated_token_account(rpc, signer, signer.pubkey(), protocol_config.bloc_mint)
            .context("failed to create or load miner ATA")?;

    let accounts = blockmine_program::accounts::SubmitSolution {
        miner: signer.pubkey(),
        config: config_pda,
        current_block: current_block_pda,
        miner_stats: miner_stats_pda,
        bloc_mint: protocol_config.bloc_mint,
        reward_vault: protocol_config.reward_vault,
        treasury_authority: protocol_config.treasury_authority,
        treasury_vault: protocol_config.treasury_vault,
        vault_authority: vault_authority_pda,
        miner_token_account,
        token_program: spl_token::ID,
        system_program: solana_sdk::system_program::ID,
    };
    let instruction = Instruction {
        program_id: rpc.program_id,
        accounts: accounts.to_account_metas(None),
        data: SubmitSolution {
            args: SubmitSolutionArgs { nonce },
        }
        .data(),
    };

    send_instruction(rpc, signer, instruction).context("submit_solution transaction failed")
}

pub fn submit_solution_with_session(
    rpc: &RpcFacade,
    delegate_signer: &Keypair,
    miner: Pubkey,
    nonce: u64,
) -> Result<Signature> {
    let config_pda = rpc.config_pda().0;
    let current_block_pda = rpc.current_block_pda().0;
    let mining_session_pda = rpc.mining_session_pda(&miner).0;
    let miner_stats_pda = rpc.miner_stats_pda(&miner).0;
    let vault_authority_pda = rpc.vault_authority_pda().0;
    let protocol_config = rpc.fetch_protocol_config()?;
    let miner_token_account =
        ensure_associated_token_account(rpc, delegate_signer, miner, protocol_config.bloc_mint)
            .context("failed to create or load miner ATA for session submit")?;

    let accounts = blockmine_program::accounts::SubmitSolutionWithSession {
        delegate: delegate_signer.pubkey(),
        miner,
        mining_session: mining_session_pda,
        config: config_pda,
        current_block: current_block_pda,
        miner_stats: miner_stats_pda,
        bloc_mint: protocol_config.bloc_mint,
        reward_vault: protocol_config.reward_vault,
        treasury_authority: protocol_config.treasury_authority,
        treasury_vault: protocol_config.treasury_vault,
        vault_authority: vault_authority_pda,
        miner_token_account,
        token_program: spl_token::ID,
        system_program: solana_sdk::system_program::ID,
    };
    let instruction = Instruction {
        program_id: rpc.program_id,
        accounts: accounts.to_account_metas(None),
        data: SubmitSolutionWithSession {
            args: SubmitSolutionArgs { nonce },
        }
        .data(),
    };

    send_instruction(rpc, delegate_signer, instruction)
        .context("submit_solution_with_session transaction failed")
}

pub fn register_miner(rpc: &RpcFacade, signer: &Keypair, nickname: [u8; 32]) -> Result<Signature> {
    let miner_stats_pda = rpc.miner_stats_pda(&signer.pubkey()).0;
    let accounts = blockmine_program::accounts::RegisterMiner {
        miner: signer.pubkey(),
        miner_stats: miner_stats_pda,
        system_program: solana_sdk::system_program::ID,
    };
    let instruction = Instruction {
        program_id: rpc.program_id,
        accounts: accounts.to_account_metas(None),
        data: RegisterMiner {
            args: RegisterMinerArgs { nickname },
        }
        .data(),
    };

    send_instruction(rpc, signer, instruction).context("register_miner transaction failed")
}

pub fn rotate_stale_block(rpc: &RpcFacade, signer: &Keypair) -> Result<Signature> {
    let config_pda = rpc.config_pda().0;
    let current_block_pda = rpc.current_block_pda().0;
    let accounts = blockmine_program::accounts::RotateStaleBlock {
        caller: signer.pubkey(),
        config: config_pda,
        current_block: current_block_pda,
    };
    let instruction = Instruction {
        program_id: rpc.program_id,
        accounts: accounts.to_account_metas(None),
        data: RotateStaleBlock {}.data(),
    };

    send_instruction(rpc, signer, instruction).context("rotate_stale_block transaction failed")
}

fn send_instruction(
    rpc: &RpcFacade,
    signer: &Keypair,
    instruction: Instruction,
) -> Result<Signature> {
    send_instructions(rpc, signer, vec![instruction])
}

fn send_instructions(
    rpc: &RpcFacade,
    signer: &Keypair,
    instructions: Vec<Instruction>,
) -> Result<Signature> {
    let recent_blockhash = rpc
        .client()
        .get_latest_blockhash()
        .context("failed to fetch latest blockhash")?;
    let tx = Transaction::new_signed_with_payer(
        &instructions,
        Some(&signer.pubkey()),
        &[signer],
        recent_blockhash,
    );

    rpc.client()
        .send_and_confirm_transaction(&tx)
        .or_else(|send_error| {
            let simulation = rpc.client().simulate_transaction_with_config(
                &tx,
                RpcSimulateTransactionConfig {
                    sig_verify: false,
                    replace_recent_blockhash: true,
                    commitment: Some(CommitmentConfig::confirmed()),
                    ..RpcSimulateTransactionConfig::default()
                },
            );

            match simulation {
                Ok(response) => {
                    let logs = response
                        .value
                        .logs
                        .unwrap_or_default()
                        .join("\n");
                    Err(anyhow::anyhow!(
                        "failed to send and confirm transaction: {send_error}\nSimulation logs:\n{logs}"
                    ))
                }
                Err(sim_error) => Err(anyhow::anyhow!(
                    "failed to send and confirm transaction: {send_error}\nAdditionally failed to simulate transaction: {sim_error}"
                )),
            }
        })
        .context("transaction execution failed")
}

fn ensure_associated_token_account(
    rpc: &RpcFacade,
    signer: &Keypair,
    owner: Pubkey,
    mint: Pubkey,
) -> Result<Pubkey> {
    let ata = get_associated_token_address(&owner, &mint);
    if rpc.client().get_account(&ata).is_ok() {
        return Ok(ata);
    }

    let instruction =
        create_associated_token_account_idempotent(&signer.pubkey(), &owner, &mint, &spl_token::ID);
    send_instructions(rpc, signer, vec![instruction])
        .with_context(|| format!("failed to create ATA {} for owner {}", ata, owner))?;

    Ok(ata)
}
