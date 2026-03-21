use std::{
    fs,
    path::{Path, PathBuf},
    thread::sleep,
    time::Duration,
};

use anchor_lang::{AccountDeserialize, AnchorSerialize};
use anyhow::{anyhow, Context, Result};
use blockmine_program::{
    constants::{CONFIG_SEED, CURRENT_BLOCK_SEED},
    instructions::UpdateDifficultyParamsArgs,
    math::rewards::{reward_era_for_block, ERA_NAME_LEN},
    state::{CurrentBlock, ProtocolConfig},
};
use clap::{Parser, Subcommand};
use sha2::{Digest, Sha256};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    bpf_loader_upgradeable::{self, UpgradeableLoaderState},
    commitment_config::CommitmentConfig,
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::{read_keypair_file, Keypair, Signature, Signer},
    transaction::Transaction,
};

#[derive(Parser, Debug)]
#[command(name = "devnet-admin")]
#[command(about = "BlockMine Devnet admin utility")]
struct Cli {
    #[arg(long, default_value = "https://api.devnet.solana.com")]
    rpc: String,
    #[arg(long, default_value = "~/.config/solana/id.json")]
    keypair: String,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    UpgradeProgram {
        #[arg(long)]
        program_id: Pubkey,
        #[arg(long)]
        binary: PathBuf,
        #[arg(long, default_value_t = 900)]
        chunk_size: usize,
    },
    UpgradeFromBuffer {
        #[arg(long)]
        program_id: Pubkey,
        #[arg(long)]
        buffer: Pubkey,
    },
    UploadToBuffer {
        #[arg(long)]
        buffer: Pubkey,
        #[arg(long)]
        binary: PathBuf,
        #[arg(long, default_value_t = 900)]
        chunk_size: usize,
    },
    ExtendProgramForBinary {
        #[arg(long)]
        program_id: Pubkey,
        #[arg(long)]
        binary: PathBuf,
    },
    SetDifficulty {
        #[arg(long)]
        program_id: Pubkey,
        #[arg(long)]
        target_block_time: u64,
        #[arg(long)]
        adjustment_interval: u64,
        #[arg(long)]
        difficulty_bits: u8,
        #[arg(long)]
        min_difficulty_bits: u8,
        #[arg(long)]
        max_difficulty_bits: u8,
    },
    ResetProtocol {
        #[arg(long)]
        program_id: Pubkey,
    },
    ShowConfig {
        #[arg(long)]
        program_id: Pubkey,
    },
    ShowWallet,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let client = RpcClient::new_with_commitment(cli.rpc, CommitmentConfig::confirmed());
    let payer = load_keypair(&cli.keypair)?;

    match cli.command {
        Command::UpgradeProgram {
            program_id,
            binary,
            chunk_size,
        } => upgrade_program(&client, &payer, program_id, &binary, chunk_size),
        Command::UpgradeFromBuffer { program_id, buffer } => {
            upgrade_from_existing_buffer(&client, &payer, program_id, buffer)
        }
        Command::UploadToBuffer {
            buffer,
            binary,
            chunk_size,
        } => upload_to_existing_buffer(&client, &payer, buffer, &binary, chunk_size),
        Command::ExtendProgramForBinary { program_id, binary } => {
            extend_program_for_binary(&client, &payer, program_id, &binary)
        }
        Command::SetDifficulty {
            program_id,
            target_block_time,
            adjustment_interval,
            difficulty_bits,
            min_difficulty_bits,
            max_difficulty_bits,
        } => set_difficulty(
            &client,
            &payer,
            program_id,
            UpdateDifficultyParamsArgs {
                target_block_time_sec: target_block_time,
                adjustment_interval,
                difficulty_bits,
                min_difficulty_bits,
                max_difficulty_bits,
            },
        ),
        Command::ResetProtocol { program_id } => reset_protocol(&client, &payer, program_id),
        Command::ShowConfig { program_id } => show_config(&client, program_id),
        Command::ShowWallet => show_wallet(&client, &payer),
    }
}

fn load_keypair(path: &str) -> Result<Keypair> {
    let resolved = expand_tilde(path);
    read_keypair_file(&resolved)
        .map_err(|error| anyhow!(error.to_string()))
        .with_context(|| format!("failed to read keypair from {}", resolved.display()))
}

fn expand_tilde(path: &str) -> PathBuf {
    if path == "~" {
        return home::home_dir().unwrap_or_else(|| PathBuf::from(path));
    }

    if let Some(stripped) = path.strip_prefix("~/") {
        if let Some(home) = home::home_dir() {
            return home.join(stripped);
        }
    }

    PathBuf::from(path)
}

fn send_transaction(
    client: &RpcClient,
    payer: &Keypair,
    instructions: Vec<Instruction>,
    extra_signers: &[&Keypair],
) -> Result<Signature> {
    let recent_blockhash = client.get_latest_blockhash()?;
    let mut signers: Vec<&dyn Signer> = vec![payer];
    signers.extend(extra_signers.iter().map(|signer| *signer as &dyn Signer));

    let tx = Transaction::new_signed_with_payer(
        &instructions,
        Some(&payer.pubkey()),
        &signers,
        recent_blockhash,
    );

    match client.send_and_confirm_transaction(&tx) {
        Ok(signature) => Ok(signature),
        Err(error) => {
            eprintln!("transaction failed, attempting simulation for logs...");
            match client.simulate_transaction(&tx) {
                Ok(simulation) => {
                    if let Some(logs) = simulation.value.logs {
                        for log in logs {
                            eprintln!("sim_log: {log}");
                        }
                    }
                    if let Some(units) = simulation.value.units_consumed {
                        eprintln!("sim_units_consumed: {units}");
                    }
                    if let Some(err) = simulation.value.err {
                        eprintln!("sim_err: {err:?}");
                    }
                }
                Err(sim_error) => {
                    eprintln!("simulation also failed: {sim_error}");
                }
            }

            Err(error).context("transaction failed")
        }
    }
}

fn upgrade_program(
    client: &RpcClient,
    payer: &Keypair,
    program_id: Pubkey,
    binary_path: &Path,
    chunk_size: usize,
) -> Result<()> {
    let program_bytes = fs::read(binary_path)
        .with_context(|| format!("failed to read program binary {}", binary_path.display()))?;
    let buffer = Keypair::new();
    let buffer_len = UpgradeableLoaderState::size_of_buffer(program_bytes.len());
    let buffer_lamports = client
        .get_minimum_balance_for_rent_exemption(buffer_len)
        .context("failed to fetch rent exemption for upgrade buffer")?;
    let payer_balance = client
        .get_balance(&payer.pubkey())
        .context("failed to fetch deploy wallet balance")?;

    println!(
        "payer={} balance_lamports={} balance_sol={:.9} binary_len={} buffer_len={} buffer_lamports={} buffer_sol={:.9}",
        payer.pubkey(),
        payer_balance,
        payer_balance as f64 / 1_000_000_000.0,
        program_bytes.len(),
        buffer_len,
        buffer_lamports,
        buffer_lamports as f64 / 1_000_000_000.0
    );

    if payer_balance < buffer_lamports {
        return Err(anyhow!(
            "insufficient deploy balance: need {} lamports for the upgrade buffer, have {}",
            buffer_lamports,
            payer_balance
        ));
    }

    let create_buffer = bpf_loader_upgradeable::create_buffer(
        &payer.pubkey(),
        &buffer.pubkey(),
        &payer.pubkey(),
        buffer_lamports,
        program_bytes.len(),
    )
    .context("failed to build create buffer instructions")?;

    let signature = send_transaction(client, payer, create_buffer, &[&buffer])?;
    println!("buffer={} create_sig={signature}", buffer.pubkey());

    let total_chunks = program_bytes.len().div_ceil(chunk_size);
    for (index, chunk) in program_bytes.chunks(chunk_size).enumerate() {
        let offset = (index * chunk_size) as u32;
        let instruction =
            bpf_loader_upgradeable::write(&buffer.pubkey(), &payer.pubkey(), offset, chunk.to_vec());
        let signature = send_transaction(client, payer, vec![instruction], &[])?;

        if index == 0 || (index + 1) % 25 == 0 || index + 1 == total_chunks {
            println!(
                "write_chunk={}/{} offset={} bytes={} sig={}",
                index + 1,
                total_chunks,
                offset,
                chunk.len(),
                signature
            );
        }

        // A tiny pause keeps the devnet write loop from tripping rate limits.
        sleep(Duration::from_millis(75));
    }

    let upgrade_ix =
        bpf_loader_upgradeable::upgrade(&program_id, &buffer.pubkey(), &payer.pubkey(), &payer.pubkey());
    let upgrade_signature = send_transaction(client, payer, vec![upgrade_ix], &[])?;
    println!("upgrade_sig={upgrade_signature}");
    Ok(())
}

fn upgrade_from_existing_buffer(
    client: &RpcClient,
    payer: &Keypair,
    program_id: Pubkey,
    buffer: Pubkey,
) -> Result<()> {
    let payer_balance = client
        .get_balance(&payer.pubkey())
        .context("failed to fetch deploy wallet balance")?;
    println!(
        "payer={} balance_lamports={} balance_sol={:.9} existing_buffer={}",
        payer.pubkey(),
        payer_balance,
        payer_balance as f64 / 1_000_000_000.0,
        buffer
    );

    let upgrade_ix =
        bpf_loader_upgradeable::upgrade(&program_id, &buffer, &payer.pubkey(), &payer.pubkey());
    let upgrade_signature = send_transaction(client, payer, vec![upgrade_ix], &[])?;
    println!("upgrade_sig={upgrade_signature}");
    Ok(())
}

fn upload_to_existing_buffer(
    client: &RpcClient,
    payer: &Keypair,
    buffer: Pubkey,
    binary_path: &Path,
    chunk_size: usize,
) -> Result<()> {
    let program_bytes = fs::read(binary_path)
        .with_context(|| format!("failed to read program binary {}", binary_path.display()))?;
    let total_chunks = program_bytes.len().div_ceil(chunk_size);
    let payer_balance = client
        .get_balance(&payer.pubkey())
        .context("failed to fetch deploy wallet balance")?;

    println!(
        "payer={} balance_lamports={} balance_sol={:.9} buffer={} binary_len={} chunk_size={} total_chunks={}",
        payer.pubkey(),
        payer_balance,
        payer_balance as f64 / 1_000_000_000.0,
        buffer,
        program_bytes.len(),
        chunk_size,
        total_chunks
    );

    for (index, chunk) in program_bytes.chunks(chunk_size).enumerate() {
        let offset = (index * chunk_size) as u32;
        let instruction =
            bpf_loader_upgradeable::write(&buffer, &payer.pubkey(), offset, chunk.to_vec());
        let signature = send_transaction(client, payer, vec![instruction], &[])?;

        if index == 0 || (index + 1) % 25 == 0 || index + 1 == total_chunks {
            println!(
                "write_chunk={}/{} offset={} bytes={} sig={}",
                index + 1,
                total_chunks,
                offset,
                chunk.len(),
                signature
            );
        }

        sleep(Duration::from_millis(75));
    }

    Ok(())
}

fn extend_program_for_binary(
    client: &RpcClient,
    payer: &Keypair,
    program_id: Pubkey,
    binary_path: &Path,
) -> Result<()> {
    let program_bytes = fs::read(binary_path)
        .with_context(|| format!("failed to read program binary {}", binary_path.display()))?;
    let (program_data_address, _) =
        Pubkey::find_program_address(&[program_id.as_ref()], &bpf_loader_upgradeable::id());
    let program_data_account = client
        .get_account(&program_data_address)
        .with_context(|| format!("failed to fetch program data account {program_data_address}"))?;
    let required_len = UpgradeableLoaderState::size_of_programdata(program_bytes.len());

    println!(
        "program_id={} programdata={} current_len={} required_len={} binary_len={}",
        program_id,
        program_data_address,
        program_data_account.data.len(),
        required_len,
        program_bytes.len()
    );

    if required_len <= program_data_account.data.len() {
        println!("no_extend_needed");
        return Ok(());
    }

    let additional_bytes = (required_len - program_data_account.data.len()) as u32;
    let required_lamports = client
        .get_minimum_balance_for_rent_exemption(required_len)
        .context("failed to fetch rent exemption for extended program data")?;
    let delta_lamports = required_lamports.saturating_sub(program_data_account.lamports);
    let payer_balance = client
        .get_balance(&payer.pubkey())
        .context("failed to fetch deploy wallet balance")?;

    println!(
        "additional_bytes={} delta_lamports={} delta_sol={:.9} payer_balance={} payer_sol={:.9}",
        additional_bytes,
        delta_lamports,
        delta_lamports as f64 / 1_000_000_000.0,
        payer_balance,
        payer_balance as f64 / 1_000_000_000.0
    );

    if payer_balance < delta_lamports {
        return Err(anyhow!(
            "insufficient deploy balance: need {} lamports to extend the program, have {}",
            delta_lamports,
            payer_balance
        ));
    }

    let extend_ix =
        bpf_loader_upgradeable::extend_program(&program_id, Some(&payer.pubkey()), additional_bytes);
    let signature = send_transaction(client, payer, vec![extend_ix], &[])?;
    println!("extend_sig={signature}");
    Ok(())
}

fn set_difficulty(
    client: &RpcClient,
    payer: &Keypair,
    program_id: Pubkey,
    args: UpdateDifficultyParamsArgs,
) -> Result<()> {
    let (config_pda, _) = Pubkey::find_program_address(&[CONFIG_SEED], &program_id);
    let (current_block_pda, _) = Pubkey::find_program_address(&[CURRENT_BLOCK_SEED], &program_id);

    let mut data = instruction_discriminator("update_difficulty_params").to_vec();
    args.serialize(&mut data)
        .context("failed to serialize difficulty params")?;

    let instruction = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new(config_pda, false),
            AccountMeta::new(current_block_pda, false),
        ],
        data,
    };

    let signature = send_transaction(client, payer, vec![instruction], &[])?;
    println!("set_difficulty_sig={signature}");
    println!(
        "target_block_time={} adjustment_interval={} bits={} min={} max={}",
        args.target_block_time_sec,
        args.adjustment_interval,
        args.difficulty_bits,
        args.min_difficulty_bits,
        args.max_difficulty_bits
    );
    Ok(())
}

fn show_config(client: &RpcClient, program_id: Pubkey) -> Result<()> {
    let (config_pda, _) = Pubkey::find_program_address(&[CONFIG_SEED], &program_id);
    let (current_block_pda, _) = Pubkey::find_program_address(&[CURRENT_BLOCK_SEED], &program_id);

    let config_account = client
        .get_account(&config_pda)
        .with_context(|| format!("failed to fetch config account {config_pda}"))?;
    let current_block_account = client
        .get_account(&current_block_pda)
        .with_context(|| format!("failed to fetch current block account {current_block_pda}"))?;

    let mut config_data = config_account.data.as_slice();
    let mut current_block_data = current_block_account.data.as_slice();
    let config = ProtocolConfig::try_deserialize(&mut config_data)
        .context("failed to deserialize protocol config")?;
    let current_block = CurrentBlock::try_deserialize(&mut current_block_data)
        .context("failed to deserialize current block")?;

    println!("program_id={program_id}");
    println!("config_pda={config_pda}");
    println!("current_block_pda={current_block_pda}");
    println!("target_block_time_sec={}", config.target_block_time_sec);
    println!("adjustment_interval={}", config.adjustment_interval);
    println!("difficulty_bits={}", config.difficulty_bits);
    println!("min_difficulty_bits={}", config.min_difficulty_bits);
    println!("max_difficulty_bits={}", config.max_difficulty_bits);
    println!("difficulty_target={}", hex::encode(config.difficulty_target));
    println!("current_block_number={}", config.current_block_number);
    println!("current_block_bits={}", current_block.difficulty_bits);
    let era = reward_era_for_block(current_block.block_number);
    println!("current_era_index={}", era.index);
    println!("current_era_name={}", decode_era_name(era.name));
    println!("current_reward={}", current_block.block_reward);
    println!("current_block_opened_at={}", current_block.opened_at);
    println!("current_block_status={}", current_block.status);
    Ok(())
}

fn reset_protocol(client: &RpcClient, payer: &Keypair, program_id: Pubkey) -> Result<()> {
    let (config_pda, _) = Pubkey::find_program_address(&[CONFIG_SEED], &program_id);
    let (current_block_pda, _) = Pubkey::find_program_address(&[CURRENT_BLOCK_SEED], &program_id);

    let instruction = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new(config_pda, false),
            AccountMeta::new(current_block_pda, false),
        ],
        data: instruction_discriminator("reset_protocol").to_vec(),
    };

    let signature = send_transaction(client, payer, vec![instruction], &[])?;
    println!("reset_protocol_sig={signature}");
    Ok(())
}

fn decode_era_name(name: [u8; ERA_NAME_LEN]) -> String {
    let end = name.iter().position(|byte| *byte == 0).unwrap_or(name.len());
    String::from_utf8_lossy(&name[..end]).into_owned()
}

fn show_wallet(client: &RpcClient, payer: &Keypair) -> Result<()> {
    let balance = client
        .get_balance(&payer.pubkey())
        .context("failed to fetch wallet balance")?;
    println!("wallet={}", payer.pubkey());
    println!("lamports={balance}");
    println!("sol={:.9}", balance as f64 / 1_000_000_000.0);
    Ok(())
}

fn instruction_discriminator(name: &str) -> [u8; 8] {
    let hash = Sha256::digest(format!("global:{name}"));
    let mut out = [0u8; 8];
    out.copy_from_slice(&hash[..8]);
    out
}
