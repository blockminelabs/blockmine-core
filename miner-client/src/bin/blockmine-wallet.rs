use std::io::{self, Write};

use anyhow::{Context, Result};
use blockmine_miner::config::CliConfig;
use blockmine_miner::rpc::RpcFacade;
use blockmine_miner::ui::format_bloc;
use blockmine_miner::vast_wallet::{
    acknowledge_worker_wallet_backup, ensure_vast_worker_wallet, load_vast_worker_seed_phrase,
    worker_wallet_backup_acknowledged, worker_wallet_keypair_path,
};
use blockmine_miner::wallet_store::load_managed_keypair;
use blockmine_program::math::rewards::reward_era_for_block;
use clap::{Parser, Subcommand};
use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey};

const DEFAULT_PROGRAM_ID: &str = "FgRe73gAkZPhxpiCFHMYMfLR4dabDaB1FDVFazVTcCtv";
const DEFAULT_RPC_URL: &str = "auto";

#[derive(Debug, Parser)]
#[command(name = "blockmine-wallet", about = "Worker wallet utility for Blockmine container miners")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Ensure,
    Address,
    KeypairPath,
    BackupStatus,
    Reveal {
        #[arg(long, default_value = DEFAULT_RPC_URL)]
        rpc: String,
        #[arg(long, default_value = DEFAULT_PROGRAM_ID)]
        program_id: String,
    },
    FundingHint {
        #[arg(long, default_value = DEFAULT_RPC_URL)]
        rpc: String,
        #[arg(long, default_value = DEFAULT_PROGRAM_ID)]
        program_id: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Ensure => {
            let ensured = ensure_vast_worker_wallet()?;
            if ensured.created {
                if ensured.imported_from_env {
                    println!("Imported worker wallet from environment input.");
                } else {
                    println!("Generated a fresh worker wallet for this instance.");
                }
            } else {
                println!("Worker wallet already present.");
            }
            println!("Address      : {}", ensured.wallet.pubkey);
            println!("Keypair path : {}", ensured.wallet.keypair_path.display());
            if let Some(path) = ensured.wallet.seed_phrase_path {
                println!("Recovery     : {}", path.display());
            } else {
                println!("Recovery     : private-key import only");
            }
            if !worker_wallet_backup_acknowledged()? {
                println!("Backup       : pending");
                println!("Run `blockmine-wallet reveal` before funding the wallet.");
            } else {
                println!("Backup       : confirmed");
            }
        }
        Commands::Address => {
            let wallet = ensure_vast_worker_wallet()?.wallet;
            println!("{}", wallet.pubkey);
        }
        Commands::KeypairPath => {
            println!("{}", worker_wallet_keypair_path()?.display());
        }
        Commands::BackupStatus => {
            if worker_wallet_backup_acknowledged()? {
                println!("acknowledged");
            } else {
                println!("pending");
                std::process::exit(1);
            }
        }
        Commands::Reveal { rpc, program_id } => {
            reveal_wallet_and_confirm(&rpc, &program_id)?;
        }
        Commands::FundingHint { rpc, program_id } => {
            print_funding_hint(&rpc, &program_id)?;
        }
    }

    Ok(())
}

fn reveal_wallet_and_confirm(rpc: &str, program_id: &str) -> Result<()> {
    let wallet = ensure_vast_worker_wallet()?.wallet;
    let keypair = load_managed_keypair(&wallet)?;
    let recovery_phrase = load_vast_worker_seed_phrase()?;

    println!("A Blockmine worker wallet is configured for this instance.");
    println!("Anyone with this recovery material can control the mined funds.");
    println!("Type YES to display the recovery material:");
    if prompt_line()?.trim() != "YES" {
        println!("Aborted.");
        return Ok(());
    }

    println!();
    println!("Public address");
    println!("{}", wallet.pubkey);
    println!();
    if let Some(phrase) = recovery_phrase {
        println!("Recovery phrase");
        println!("{}", phrase);
        println!();
    } else {
        println!("Recovery phrase");
        println!("not available (this wallet was imported from a private key)");
        println!();
    }
    println!("Private key (base58)");
    println!("{}", bs58::encode(keypair.to_bytes()).into_string());
    println!();
    println!("Type Y once you have stored the recovery material safely:");
    if prompt_line()?.trim() == "Y" {
        acknowledge_worker_wallet_backup()?;
        println!("Backup marked as confirmed.");
        println!();
        print_funding_hint(rpc, program_id)?;
    } else {
        println!("Backup marker was not written. Run `blockmine-wallet reveal` again after storing it.");
    }

    Ok(())
}

fn print_funding_hint(rpc: &str, program_id: &str) -> Result<()> {
    let wallet = ensure_vast_worker_wallet()?.wallet;
    let config = cli_config(rpc, program_id)?;
    let rpc = RpcFacade::new(&config);
    let protocol = rpc.fetch_protocol_config()?;
    let current_block = rpc.fetch_current_block()?;
    let current_era = reward_era_for_block(protocol.total_blocks_mined);

    println!("Deposit SOL to:");
    println!("{}", wallet.pubkey);
    println!();
    println!(
        "Accepted block fee: {:.2} SOL",
        protocol.submit_fee_lamports as f64 / 1_000_000_000.0
    );
    println!("Current gross reward: {} BLOC", format_bloc(current_block.block_reward));
    println!("Current era: {}", trim_era_name(current_era.name));
    println!("Current block: #{}", current_block.block_number);

    Ok(())
}

fn cli_config(rpc: &str, program_id: &str) -> Result<CliConfig> {
    Ok(CliConfig {
        rpc_url: rpc.to_string(),
        program_id: program_id
            .parse::<Pubkey>()
            .context("invalid program id")?,
        keypair_path: None,
        commitment: CommitmentConfig::confirmed(),
    })
}

fn trim_era_name(raw: [u8; 16]) -> String {
    let end = raw.iter().position(|byte| *byte == 0).unwrap_or(raw.len());
    String::from_utf8_lossy(&raw[..end]).trim().to_string()
}

fn prompt_line() -> Result<String> {
    let mut input = String::new();
    io::stdout().flush().context("failed to flush stdout")?;
    io::stdin()
        .read_line(&mut input)
        .context("failed to read input")?;
    Ok(input.trim_end().to_string())
}
