use std::io::{self, Write};

use anyhow::{Context, Result};
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Signer;

use crate::commands::mine::{self, MineOptions};
use crate::config::CliConfig;
use crate::engine::BackendMode;
use crate::wallet::load_keypair;

pub fn run(
    config: &CliConfig,
    backend: BackendMode,
    batch_size: u64,
    gpu_batch_size: Option<u64>,
    cpu_threads: usize,
    gpu_platform: usize,
    gpu_device: usize,
    gpu_local_work_size: Option<usize>,
) -> Result<()> {
    let signer = load_keypair(config)?;
    let signer_pubkey = signer.pubkey();

    println!("BlockMine Desktop Miner");
    println!("Incolla l'address pubblico del wallet da usare per il mining.");
    println!("Il programma verifichera che corrisponda al keypair locale configurato.");
    println!();
    print!("Wallet address [{}]: ", signer_pubkey);
    io::stdout().flush().context("failed to flush stdout")?;

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .context("failed to read wallet address")?;
    let trimmed = input.trim();
    let wallet_address = if trimmed.is_empty() {
        signer_pubkey
    } else {
        trimmed
            .parse::<Pubkey>()
            .context("wallet address non valido")?
    };

    if wallet_address != signer_pubkey {
        anyhow::bail!(
            "l'address {} non corrisponde al keypair configurato {}. Per minare servono address pubblico e keypair corrispondente.",
            wallet_address,
            signer_pubkey
        );
    }

    println!("Wallet verificato: {}", signer_pubkey);
    println!("Avvio mining...");

    mine::run_with_signer(
        config,
        signer,
        MineOptions {
            backend,
            batch_size,
            gpu_batch_size,
            cpu_threads,
            gpu_platform,
            gpu_device,
            gpu_local_work_size,
            start_nonce: None,
        },
    )
}
