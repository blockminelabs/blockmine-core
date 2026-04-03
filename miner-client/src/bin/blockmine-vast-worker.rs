use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use blockmine_miner::config::CliConfig;
use blockmine_miner::engine::BackendMode;
use blockmine_miner::miner_loop::GpuDeviceSelection;
use blockmine_miner::mining_service::{MiningHandle, MiningRuntimeOptions, MiningSnapshot, MiningUpdate};
use blockmine_miner::rpc::RpcFacade;
use blockmine_miner::rig_probe::{detect_nvidia_devices, summarize_nvidia_devices};
use blockmine_miner::ui::format_bloc;
use blockmine_miner::vast_wallet::{ensure_vast_worker_wallet, worker_wallet_backup_acknowledged};
use blockmine_miner::wallet_store::load_managed_keypair;
use blockmine_program::math::rewards::reward_era_for_block;
use clap::Parser;
use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey};

const DEFAULT_PROGRAM_ID: &str = "FgRe73gAkZPhxpiCFHMYMfLR4dabDaB1FDVFazVTcCtv";
const DEFAULT_RPC_URL: &str = "https://api.mainnet-beta.solana.com";
const DEFAULT_SITE_URL: &str = "https://blockmine.dev";

#[derive(Debug, Parser)]
#[command(name = "blockmine-vast-worker", about = "Headless Blockmine worker for Vast.ai")]
struct Cli {
    #[arg(long, env = "BLOCKMINE_RPC_URL", default_value = DEFAULT_RPC_URL)]
    rpc: String,
    #[arg(long, env = "BLOCKMINE_PROGRAM_ID", default_value = DEFAULT_PROGRAM_ID)]
    program_id: String,
    #[arg(long, env = "BLOCKMINE_SITE_URL", default_value = DEFAULT_SITE_URL)]
    site_url: String,
    #[arg(long, env = "BLOCKMINE_LEADERBOARD_INGEST_URL")]
    leaderboard_ingest_url: Option<String>,
    #[arg(long, env = "BLOCKMINE_WORKER_LABEL", default_value = "vast-worker")]
    worker_label: String,
    #[arg(long, env = "BLOCKMINE_PLATFORM_DETAIL", default_value = "Mining Rig - Vast.ai")]
    platform_detail: String,
    #[arg(long, env = "BLOCKMINE_HARDWARE_SUMMARY")]
    hardware_summary: Option<String>,
    #[arg(long, value_enum, env = "BLOCKMINE_BACKEND", default_value_t = BackendMode::Gpu)]
    backend: BackendMode,
    #[arg(long, env = "BLOCKMINE_BATCH_SIZE", default_value_t = 250_000)]
    batch_size: u64,
    #[arg(long, env = "BLOCKMINE_GPU_BATCH_SIZE")]
    gpu_batch_size: Option<u64>,
    #[arg(long, env = "BLOCKMINE_CPU_THREADS", default_value_t = 0)]
    cpu_threads: usize,
    #[arg(long, env = "BLOCKMINE_GPU_PLATFORM", default_value_t = 0)]
    gpu_platform: usize,
    #[arg(long, env = "BLOCKMINE_GPU_DEVICE", default_value_t = 0)]
    gpu_device: usize,
    #[arg(long, env = "BLOCKMINE_GPU_LOCAL_WORK_SIZE")]
    gpu_local_work_size: Option<usize>,
    #[arg(long, env = "BLOCKMINE_GPU_DEVICES", value_delimiter = ',')]
    gpu_devices: Vec<String>,
    #[arg(long, env = "BLOCKMINE_MIN_START_SOL", default_value_t = 0.05)]
    min_start_sol: f64,
    #[arg(long, env = "BLOCKMINE_FUNDING_POLL_SECONDS", default_value_t = 5)]
    funding_poll_seconds: u64,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = cli_config(&cli.rpc, &cli.program_id)?;
    let rpc = RpcFacade::new(&config);
    let ensured = ensure_vast_worker_wallet()?;
    let wallet = ensured.wallet;

    println!("Blockmine Vast.ai worker");
    println!("Worker label  : {}", cli.worker_label);
    println!("Wallet        : {}", wallet.pubkey);
    println!("RPC           : {}", cli.rpc);
    println!("Program ID    : {}", cli.program_id);
    println!("Site          : {}", cli.site_url);
    println!();

    wait_for_backup_confirmation(ensured.created)?;
    wait_for_funding(&cli, &config, &rpc, &wallet.pubkey)?;

    let signer = load_managed_keypair(&wallet)?;
    let gpu_devices = parse_gpu_devices(&cli.gpu_devices)?;
    let detected_nvidia_devices = detect_nvidia_devices();
    let hardware_summary = cli
        .hardware_summary
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| summarize_nvidia_devices(&detected_nvidia_devices));
    let ingest_url = cli
        .leaderboard_ingest_url
        .clone()
        .or_else(|| derive_leaderboard_ingest_url(&cli.site_url));

    println!("Starting mining service.");
    println!(
        "Leaderboard ingest: {}",
        ingest_url
            .as_deref()
            .unwrap_or("disabled")
    );
    if !hardware_summary.is_empty() {
        println!("Hardware      : {}", hardware_summary);
    }
    println!();

    let handle = MiningHandle::start(
        config.clone(),
        signer,
        MiningRuntimeOptions {
            backend: cli.backend,
            batch_size: cli.batch_size,
            gpu_batch_size: cli.gpu_batch_size,
            cpu_threads: cli.cpu_threads,
            cpu_core_ids: None,
            gpu_devices,
            gpu_platform: cli.gpu_platform,
            gpu_device: cli.gpu_device,
            gpu_local_work_size: cli.gpu_local_work_size,
            start_nonce: None,
            miner_override: None,
            leaderboard_ingest_url: ingest_url,
            platform_detail: Some(cli.platform_detail.clone()),
            hardware_summary: Some(hardware_summary),
        },
    )?;

    let interrupted = Arc::new(AtomicBool::new(false));
    let interrupt_flag = Arc::clone(&interrupted);
    ctrlc::set_handler(move || {
        interrupt_flag.store(true, Ordering::Relaxed);
    })
    .context("failed to install Ctrl+C handler")?;

    let mut last_snapshot: Option<MiningSnapshot> = None;
    let mut stop_requested = false;

    loop {
        if interrupted.load(Ordering::Relaxed) && !stop_requested {
            println!("Stopping worker...");
            handle.stop();
            stop_requested = true;
        }

        match handle.try_recv() {
            Ok(MiningUpdate::Snapshot(snapshot)) => {
                maybe_log_snapshot(last_snapshot.as_ref(), &snapshot);
                last_snapshot = Some(snapshot);
            }
            Ok(MiningUpdate::Stopped { snapshot, error }) => {
                maybe_log_snapshot(last_snapshot.as_ref(), &snapshot);
                if let Some(error) = error {
                    anyhow::bail!(error);
                }
                break;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                thread::sleep(Duration::from_millis(250));
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                anyhow::bail!("mining worker channel disconnected");
            }
        }
    }

    Ok(())
}

fn wait_for_backup_confirmation(created: bool) -> Result<()> {
    let mut last_notice = Instant::now()
        .checked_sub(Duration::from_secs(30))
        .unwrap_or_else(Instant::now);

    while !worker_wallet_backup_acknowledged()? {
        if last_notice.elapsed() >= Duration::from_secs(15) {
            if created {
                println!("A fresh worker wallet was created for this instance.");
            }
            println!("Recovery phrase confirmation is still pending.");
            println!("Open a Jupyter or SSH terminal and run: blockmine-wallet reveal");
            println!("Mining will start automatically after the backup is confirmed.");
            println!();
            last_notice = Instant::now();
        }
        thread::sleep(Duration::from_secs(5));
    }

    println!("Recovery material confirmed.");
    println!();
    Ok(())
}

fn wait_for_funding(cli: &Cli, config: &CliConfig, rpc: &RpcFacade, wallet: &str) -> Result<()> {
    let wallet_pubkey = wallet
        .parse::<Pubkey>()
        .context("invalid worker wallet pubkey")?;
    let required_lamports = ((cli.min_start_sol * 1_000_000_000.0).ceil() as u64).max(10_005_000);
    let mut last_notice = Instant::now()
        .checked_sub(Duration::from_secs(30))
        .unwrap_or_else(Instant::now);

    loop {
        let protocol = rpc.fetch_protocol_config()?;
        let current_block = rpc.fetch_current_block()?;
        let balance_lamports = rpc
            .client()
            .get_balance(&wallet_pubkey)
            .with_context(|| format!("failed to fetch balance for {}", wallet_pubkey))?;

        if balance_lamports >= required_lamports {
            println!(
                "Funding detected: {:.6} SOL",
                balance_lamports as f64 / 1_000_000_000.0
            );
            println!();
            break;
        }

        if last_notice.elapsed() >= Duration::from_secs(15) {
            print_funding_hint(wallet, &protocol, &current_block);
            println!(
                "Current balance: {:.6} SOL",
                balance_lamports as f64 / 1_000_000_000.0
            );
            println!(
                "Waiting for at least {:.6} SOL before mining starts.",
                required_lamports as f64 / 1_000_000_000.0
            );
            println!();
            last_notice = Instant::now();
        }

        thread::sleep(Duration::from_secs(cli.funding_poll_seconds.max(1)));
    }

    let _ = config;
    Ok(())
}

fn print_funding_hint(
    wallet: &str,
    protocol: &blockmine_program::state::ProtocolConfig,
    current_block: &blockmine_program::state::CurrentBlock,
) {
    let current_era = reward_era_for_block(protocol.total_blocks_mined);
    println!("Deposit SOL to:");
    println!("{}", wallet);
    println!();
    println!(
        "Accepted block fee: {:.2} SOL",
        protocol.submit_fee_lamports as f64 / 1_000_000_000.0
    );
    println!("Current gross reward: {} BLOC", format_bloc(current_block.block_reward));
    println!("Current era: {}", trim_era_name(current_era.name));
    println!("Current block: #{}", current_block.block_number);
}

fn maybe_log_snapshot(previous: Option<&MiningSnapshot>, snapshot: &MiningSnapshot) {
    let blocks_advanced = previous
        .map(|last| snapshot.wallet_blocks_mined > last.wallet_blocks_mined)
        .unwrap_or(true);
    if blocks_advanced {
        println!(
            "Accepted blocks: {} | Wallet mined: {} BLOC | Last tx: {}",
            snapshot.wallet_blocks_mined,
            format_bloc(snapshot.wallet_tokens_mined),
            snapshot
                .last_signature
                .clone()
                .unwrap_or_else(|| "-".to_string())
        );
        return;
    }

    let block_changed = previous
        .map(|last| last.current_block_number != snapshot.current_block_number)
        .unwrap_or(true);
    let status_changed = previous
        .map(|last| last.status != snapshot.status)
        .unwrap_or(true);
    let rate_changed = previous
        .map(|last| last.last_hashrate != snapshot.last_hashrate)
        .unwrap_or(true);

    if block_changed || status_changed || rate_changed {
        println!(
            "{} | block #{} | rate {} | session mined {} BLOC",
            snapshot.status,
            snapshot.current_block_number,
            snapshot.last_hashrate,
            format_bloc(snapshot.session_tokens_mined)
        );
    }
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

fn parse_gpu_devices(values: &[String]) -> Result<Vec<GpuDeviceSelection>> {
    let mut devices = Vec::new();
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }
        let (platform, device) = trimmed
            .split_once(':')
            .with_context(|| format!("invalid gpu device selection `{trimmed}`; expected <platform>:<device>"))?;
        devices.push(GpuDeviceSelection {
            platform_index: platform
                .parse::<usize>()
                .with_context(|| format!("invalid gpu platform index in `{trimmed}`"))?,
            device_index: device
                .parse::<usize>()
                .with_context(|| format!("invalid gpu device index in `{trimmed}`"))?,
        });
    }
    Ok(devices)
}

fn trim_era_name(raw: [u8; 16]) -> String {
    let end = raw.iter().position(|byte| *byte == 0).unwrap_or(raw.len());
    String::from_utf8_lossy(&raw[..end]).trim().to_string()
}

fn derive_site_origin(raw_url: &str) -> Option<String> {
    let trimmed = raw_url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }

    let scheme_index = trimmed.find("://")?;
    let authority_start = scheme_index + 3;
    let path_start = trimmed[authority_start..]
        .find('/')
        .map(|offset| authority_start + offset);

    Some(match path_start {
        Some(index) => trimmed[..index].to_string(),
        None => trimmed.to_string(),
    })
}

fn derive_leaderboard_ingest_url(raw_url: &str) -> Option<String> {
    derive_site_origin(raw_url).map(|origin| format!("{origin}/api/leaderboard/heartbeat"))
}
