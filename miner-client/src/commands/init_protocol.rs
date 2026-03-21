use anyhow::{Context, Result};
use blockmine_program::instructions::InitializeProtocolArgs;
use solana_sdk::{pubkey::Pubkey, signature::Signer};

use crate::config::CliConfig;
use crate::rpc::RpcFacade;
use crate::submitter;
use crate::wallet::load_keypair;

#[derive(Debug, Clone)]
pub struct InitProtocolCommand {
    pub mint: String,
    pub treasury_authority: Option<String>,
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

pub fn run(config: &CliConfig, command: InitProtocolCommand) -> Result<()> {
    let signer = load_keypair(config)?;
    let rpc = RpcFacade::new(config);
    let mint = command
        .mint
        .parse::<Pubkey>()
        .context("invalid mint pubkey")?;
    let treasury_authority = match &command.treasury_authority {
        Some(value) => value
            .parse::<Pubkey>()
            .context("invalid treasury authority pubkey")?,
        None => signer.pubkey(),
    };

    let signature = submitter::initialize_protocol(
        &rpc,
        &signer,
        mint,
        treasury_authority,
        InitializeProtocolArgs {
            max_supply: command.max_supply,
            initial_block_reward: command.initial_block_reward,
            treasury_fee_bps: command.treasury_fee_bps,
            halving_interval: command.halving_interval,
            target_block_time_sec: command.target_block_time_sec,
            adjustment_interval: command.adjustment_interval,
            initial_difficulty_bits: command.initial_difficulty_bits,
            min_difficulty_bits: command.min_difficulty_bits,
            max_difficulty_bits: command.max_difficulty_bits,
            submit_fee_lamports: command.submit_fee_lamports,
            block_ttl_sec: command.block_ttl_sec,
            token_decimals: command.token_decimals,
        },
    )?;

    let reward_vault = spl_associated_token_account::get_associated_token_address(
        &rpc.vault_authority_pda().0,
        &mint,
    );
    let treasury_vault =
        spl_associated_token_account::get_associated_token_address(&treasury_authority, &mint);

    println!("signature={signature}");
    println!("reward_vault={reward_vault}");
    println!("treasury_authority={treasury_authority}");
    println!("treasury_vault={treasury_vault}");
    println!("treasury_fee_bps={}", command.treasury_fee_bps);
    println!("config_pda={}", rpc.config_pda().0);
    println!("current_block_pda={}", rpc.current_block_pda().0);
    Ok(())
}
