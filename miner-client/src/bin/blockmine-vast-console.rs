use std::io::{self, stdout, Stdout, Write};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use blockmine_miner::config::CliConfig;
use blockmine_miner::engine::{gpu, BackendMode};
use blockmine_miner::miner_loop::GpuDeviceSelection;
use blockmine_miner::mining_service::{MiningHandle, MiningRuntimeOptions, MiningSnapshot, MiningUpdate};
use blockmine_miner::rpc::RpcFacade;
use blockmine_miner::rig_probe::{detect_nvidia_devices, summarize_nvidia_devices};
use blockmine_miner::session_wallet::{load_managed_wallet_balances, sweep_single_session_delegate_wallet};
use blockmine_miner::ui::{format_bloc, format_u64};
use blockmine_miner::vast_wallet::{
    acknowledge_worker_wallet_backup, ensure_vast_worker_wallet, load_vast_worker_seed_phrase,
    worker_wallet_backup_acknowledged,
};
use blockmine_miner::wallet_store::{load_managed_keypair, ManagedWallet};
use blockmine_program::math::rewards::reward_era_for_block;
use clap::Parser;
use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::style::{Attribute, Print, SetAttribute};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, size, Clear, ClearType, EnterAlternateScreen,
    LeaveAlternateScreen,
};
use crossterm::{execute, queue};
use solana_sdk::{
    commitment_config::CommitmentConfig,
    pubkey::Pubkey,
};

const DEFAULT_PROGRAM_ID: &str = "FgRe73gAkZPhxpiCFHMYMfLR4dabDaB1FDVFazVTcCtv";
const DEFAULT_RPC_URL: &str = "https://api.mainnet-beta.solana.com";
const DEFAULT_SITE_URL: &str = "https://blockmine.dev";

#[derive(Debug, Parser, Clone)]
#[command(name = "blockmine-vast-console", about = "Interactive SSH/Jupyter console for Blockmine Vast.ai miners")]
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
    #[arg(long, env = "BLOCKMINE_UI_POLL_MS", default_value_t = 1200)]
    ui_poll_ms: u64,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let wallet = ensure_vast_worker_wallet()?.wallet;
    reveal_wallet_if_needed(&cli, &wallet)?;

    let mut app = VastConsole::new(cli, wallet)?;
    app.run()
}

fn reveal_wallet_if_needed(cli: &Cli, wallet: &ManagedWallet) -> Result<()> {
    if worker_wallet_backup_acknowledged()? {
        return Ok(());
    }

    let recovery_phrase = load_vast_worker_seed_phrase()?;
    let keypair = load_managed_keypair(wallet)?;
    let config = cli_config(&cli.rpc, &cli.program_id)?;
    let rpc = RpcFacade::new(&config);
    let protocol = rpc.fetch_protocol_config()?;
    let current_block = rpc.fetch_current_block()?;
    let current_era = reward_era_for_block(protocol.total_blocks_mined);

    println!("A new Blockmine worker wallet has been created for this instance.");
    println!("This recovery material controls every mined payout for this worker.");
    println!("If it is not stored now, the wallet cannot be recovered later.");
    println!();
    println!("Type YES to display the recovery material:");
    if prompt_line()?.trim() != "YES" {
        println!("Aborted.");
        std::process::exit(1);
    }

    println!();
    println!("Public address");
    println!("{}", wallet.pubkey);
    println!();
    println!("Recovery phrase");
    if let Some(phrase) = recovery_phrase {
        println!("{phrase}");
    } else {
        println!("not available (this wallet was imported from a private key)");
    }
    println!();
    println!("Private key (base58)");
    println!("{}", bs58::encode(keypair.to_bytes()).into_string());
    println!();
    println!("Type YES once the recovery material has been stored safely:");
    if prompt_line()?.trim() != "YES" {
        println!("Backup confirmation was not written. Run the console again once the recovery material is saved.");
        std::process::exit(1);
    }

    acknowledge_worker_wallet_backup()?;
    clear_plain_screen()?;

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
    println!();
    println!("Opening the live console...");
    std::thread::sleep(Duration::from_millis(1200));
    Ok(())
}

struct VastConsole {
    cli: Cli,
    wallet: ManagedWallet,
    config: CliConfig,
    rpc: RpcFacade,
    stdout: Stdout,
    started_at: Instant,
    last_refresh_at: Instant,
    protocol_submit_fee_lamports: u64,
    protocol_current_reward: u64,
    protocol_current_block: u64,
    protocol_difficulty_bits: u8,
    protocol_era: String,
    wallet_sol_lamports: u64,
    wallet_bloc_raw: u64,
    wallet_bloc_decimals: u8,
    wallet_blocks_mined: u64,
    wallet_tokens_mined: u64,
    nvidia_devices: Vec<String>,
    hardware_summary: String,
    opencl_devices: Vec<String>,
    gpu_status: String,
    last_message: String,
    manual_stop: bool,
    mining: Option<MiningHandle>,
    mining_snapshot: MiningSnapshot,
}

impl VastConsole {
    fn new(cli: Cli, wallet: ManagedWallet) -> Result<Self> {
        let config = cli_config(&cli.rpc, &cli.program_id)?;
        let rpc = RpcFacade::new(&config);
        let (nvidia_devices, opencl_devices, gpu_status) = detect_gpu_environment();
        let hardware_summary = cli
            .hardware_summary
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| summarize_nvidia_devices(&nvidia_devices));

        Ok(Self {
            cli,
            wallet,
            config,
            rpc,
            stdout: stdout(),
            started_at: Instant::now(),
            last_refresh_at: Instant::now()
                .checked_sub(Duration::from_secs(5))
                .unwrap_or_else(Instant::now),
            protocol_submit_fee_lamports: 10_000_000,
            protocol_current_reward: 0,
            protocol_current_block: 0,
            protocol_difficulty_bits: 0,
            protocol_era: "-".to_string(),
            wallet_sol_lamports: 0,
            wallet_bloc_raw: 0,
            wallet_bloc_decimals: 9,
            wallet_blocks_mined: 0,
            wallet_tokens_mined: 0,
            nvidia_devices,
            hardware_summary,
            opencl_devices,
            gpu_status,
            last_message: "Waiting for wallet funding.".to_string(),
            manual_stop: false,
            mining: None,
            mining_snapshot: MiningSnapshot::default(),
        })
    }

    fn run(&mut self) -> Result<()> {
        enable_raw_mode()?;
        execute!(self.stdout, EnterAlternateScreen, Hide)?;

        let result = self.run_loop();

        if let Some(handle) = &self.mining {
            handle.stop();
        }
        execute!(self.stdout, Show, LeaveAlternateScreen)?;
        disable_raw_mode()?;

        result
    }

    fn run_loop(&mut self) -> Result<()> {
        loop {
            self.refresh_from_chain(false);
            self.poll_mining_updates();
            self.maybe_auto_start()?;
            self.render()?;

            if event::poll(Duration::from_millis(150))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        match key.code {
                            KeyCode::Char('q') => break,
                            KeyCode::Char('r') => {
                                self.refresh_gpu_details();
                                self.refresh_from_chain(true);
                            }
                            KeyCode::Char('s') => self.toggle_mining()?,
                            KeyCode::Char('w') => self.withdraw_flow()?,
                            _ => {}
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn refresh_from_chain(&mut self, force: bool) {
        if !force && self.last_refresh_at.elapsed() < Duration::from_millis(self.cli.ui_poll_ms) {
            return;
        }

        if let Ok(protocol) = self.rpc.fetch_protocol_config() {
            self.protocol_submit_fee_lamports = protocol.submit_fee_lamports;
            self.wallet_bloc_decimals = protocol.token_decimals;
            self.protocol_era = trim_era_name(reward_era_for_block(protocol.total_blocks_mined).name);
        }

        if let Ok(current_block) = self.rpc.fetch_current_block() {
            self.protocol_current_reward = current_block.block_reward;
            self.protocol_current_block = current_block.block_number;
            self.protocol_difficulty_bits = current_block.difficulty_bits;
        }

        if let Ok(summary) =
            load_managed_wallet_balances(&self.cli.rpc, self.config.program_id, &self.wallet)
        {
            if let Some(balance) = summary.balances.first() {
                self.wallet_sol_lamports = balance.balance_lamports;
                self.wallet_bloc_raw = balance.bloc_balance_raw;
            }
            self.wallet_bloc_decimals = summary.bloc_decimals;
        }

        if let Ok(miner_pubkey) = self.wallet.pubkey.parse::<Pubkey>() {
            if let Ok(stats) = self.rpc.fetch_miner_stats(&miner_pubkey) {
                self.wallet_blocks_mined = stats.valid_blocks_found;
                self.wallet_tokens_mined = stats.total_rewards_earned;
            }
        }

        self.last_refresh_at = Instant::now();
    }

    fn maybe_auto_start(&mut self) -> Result<()> {
        if self.mining.is_some() || self.manual_stop {
            return Ok(());
        }

        if !self.wallet_is_funded() {
            return Ok(());
        }

        if matches!(self.cli.backend, BackendMode::Gpu | BackendMode::Both) && self.opencl_devices.is_empty() {
            self.last_message = "GPU selected, but no usable OpenCL device is visible in the container. Fix the template or relaunch with CPU.".to_string();
            return Ok(());
        }

        self.start_mining()
    }

    fn wallet_is_funded(&self) -> bool {
        let required = ((self.cli.min_start_sol * 1_000_000_000.0).ceil() as u64)
            .max(self.protocol_submit_fee_lamports.saturating_add(5_000));
        self.wallet_sol_lamports >= required
    }

    fn start_mining(&mut self) -> Result<()> {
        if self.mining.is_some() {
            return Ok(());
        }

        let signer = load_managed_keypair(&self.wallet)?;
        let gpu_devices = parse_gpu_devices(&self.cli.gpu_devices)?;
        let ingest_url = self
            .cli
            .leaderboard_ingest_url
            .clone()
            .or_else(|| derive_leaderboard_ingest_url(&self.cli.site_url));

        let handle = MiningHandle::start(
            self.config.clone(),
            signer,
            MiningRuntimeOptions {
                backend: self.cli.backend,
                batch_size: self.cli.batch_size,
                gpu_batch_size: self.cli.gpu_batch_size,
                cpu_threads: self.cli.cpu_threads,
                cpu_core_ids: None,
                gpu_devices,
                gpu_platform: self.cli.gpu_platform,
                gpu_device: self.cli.gpu_device,
                gpu_local_work_size: self.cli.gpu_local_work_size,
                start_nonce: None,
                miner_override: None,
                leaderboard_ingest_url: ingest_url,
                platform_detail: Some(self.cli.platform_detail.clone()),
                hardware_summary: Some(self.hardware_summary.clone()),
            },
        )?;

        self.mining = Some(handle);
        self.manual_stop = false;
        self.last_message = "Mining started.".to_string();
        Ok(())
    }

    fn stop_mining(&mut self) {
        if let Some(handle) = &self.mining {
            handle.stop();
            self.last_message = "Stopping mining worker...".to_string();
            self.manual_stop = true;
        }
    }

    fn toggle_mining(&mut self) -> Result<()> {
        if self.mining.is_some() {
            self.stop_mining();
            return Ok(());
        }

        if !self.wallet_is_funded() {
            self.last_message = "Wallet balance is still below the mining threshold.".to_string();
            return Ok(());
        }

        self.start_mining()
    }

    fn poll_mining_updates(&mut self) {
        let mut stopped = false;
        if let Some(handle) = &self.mining {
            loop {
                match handle.try_recv() {
                    Ok(MiningUpdate::Snapshot(snapshot)) => {
                        self.mining_snapshot = snapshot;
                    }
                    Ok(MiningUpdate::Stopped { snapshot, error }) => {
                        self.mining_snapshot = snapshot;
                        self.last_message = error.unwrap_or_else(|| "Mining stopped.".to_string());
                        stopped = true;
                        break;
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        self.last_message = "Mining worker disconnected.".to_string();
                        stopped = true;
                        break;
                    }
                }
            }
        }

        if stopped {
            self.mining = None;
        }
    }

    fn render(&mut self) -> Result<()> {
        let (width, height) = size().unwrap_or((120, 40));
        let funded_line = if self.wallet_is_funded() {
            "funded"
        } else {
            "waiting for deposit"
        };
        let mining_running = self.mining.is_some();
        let runtime = format_runtime(self.started_at.elapsed());
        let last_rate = if self.mining_snapshot.last_hashrate == "-" {
            "0 H/s".to_string()
        } else {
            self.mining_snapshot.last_hashrate.clone()
        };
        let nvidia_runtime = if self.nvidia_devices.is_empty() {
            "none detected".to_string()
        } else {
            self.nvidia_devices.join(" | ")
        };
        let opencl_runtime = if self.opencl_devices.is_empty() {
            "none detected".to_string()
        } else {
            self.opencl_devices.join(" | ")
        };

        queue!(self.stdout, MoveTo(0, 0), Clear(ClearType::All))?;
        queue!(
            self.stdout,
            SetAttribute(Attribute::Bold),
            Print("Blockmine Vast Console\n"),
            SetAttribute(Attribute::Reset),
            Print("[S] Start/Stop  [W] Withdraw  [R] Refresh GPU probe  [Q] Quit\n"),
            Print(format!("Worker label : {}\n", self.cli.worker_label)),
            Print(format!("Backend      : {:?}\n", self.cli.backend)),
            Print(format!("Runtime      : {}\n", runtime)),
            Print(format!("Console      : {}\n", if mining_running { "mining live" } else { "idle" })),
            Print(format!("Wallet state : {}\n\n", funded_line)),
            SetAttribute(Attribute::Bold),
            Print("Wallet\n"),
            SetAttribute(Attribute::Reset),
            Print(format!("Address      : {}\n", self.wallet.pubkey)),
            Print(format!("SOL balance  : {}\n", format_sol(self.wallet_sol_lamports))),
            Print(format!("BLOC balance : {}\n", format_token_amount(self.wallet_bloc_raw, self.wallet_bloc_decimals))),
            Print(format!("Wallet mined : {} BLOC across {} blocks\n\n", format_bloc(self.wallet_tokens_mined), self.wallet_blocks_mined)),
            SetAttribute(Attribute::Bold),
            Print("Funding target\n"),
            SetAttribute(Attribute::Reset),
            Print(format!("Deposit SOL to: {}\n", self.wallet.pubkey)),
            Print(format!("Accepted block fee : {}\n", format_sol(self.protocol_submit_fee_lamports))),
            Print(format!("Current reward     : {} BLOC\n", format_bloc(self.protocol_current_reward))),
            Print(format!("Current era        : {}\n", self.protocol_era)),
            Print(format!("Current block      : #{}\n", self.protocol_current_block)),
            Print(format!("Difficulty         : {} bits\n\n", self.protocol_difficulty_bits)),
            SetAttribute(Attribute::Bold),
            Print("Detected GPUs\n"),
            SetAttribute(Attribute::Reset),
            Print(format!("Rig summary    : {}\n", if self.hardware_summary.is_empty() { "-".to_string() } else { self.hardware_summary.clone() })),
            Print("NVIDIA runtime\n"),
        )?;

        for line in wrap_text(&nvidia_runtime, width.saturating_sub(4) as usize).into_iter() {
            queue!(self.stdout, Print(format!("  {line}\n")))?;
        }

        queue!(
            self.stdout,
            Print("OpenCL devices\n"),
        )?;

        for line in wrap_text(&opencl_runtime, width.saturating_sub(4) as usize).into_iter() {
            queue!(self.stdout, Print(format!("  {line}\n")))?;
        }

        queue!(
            self.stdout,
            Print(format!("GPU status     : {}\n\n", self.gpu_status)),
            SetAttribute(Attribute::Bold),
            Print("Mining telemetry\n"),
            SetAttribute(Attribute::Reset),
            Print(format!("Status         : {}\n", if mining_running { &self.mining_snapshot.status } else { "Idle" })),
            Print(format!("Live rate      : {}\n", last_rate)),
            Print(format!("Attempts       : {}\n", format_u64(self.mining_snapshot.session_hashes))),
            Print(format!("Session mined  : {} BLOC across {} blocks\n", format_bloc(self.mining_snapshot.session_tokens_mined), self.mining_snapshot.session_blocks_mined)),
            Print(format!("Last tx        : {}\n", self.mining_snapshot.last_signature.clone().unwrap_or_else(|| "-".to_string()))),
            Print(format!("Last event     : {}\n", self.mining_snapshot.last_event)),
            Print(format!("Console note   : {}\n", self.last_message)),
        )?;

        if height > 30 {
            queue!(self.stdout, Print("\nRecent reports\n"))?;
            if self.mining_snapshot.recent_reports.is_empty() {
                queue!(self.stdout, Print("  waiting for clean hashrate samples...\n"))?;
            } else {
                for report in self.mining_snapshot.recent_reports.iter().take(4) {
                    queue!(self.stdout, Print(format!("  {report}\n")))?;
                }
            }
        }

        self.stdout.flush()?;
        Ok(())
    }

    fn refresh_gpu_details(&mut self) {
        let (nvidia_devices, opencl_devices, gpu_status) = detect_gpu_environment();
        self.nvidia_devices = nvidia_devices;
        if self
            .cli
            .hardware_summary
            .as_deref()
            .is_none_or(|value| value.trim().is_empty())
        {
            self.hardware_summary = summarize_nvidia_devices(&self.nvidia_devices);
        }
        self.opencl_devices = opencl_devices;
        self.gpu_status = gpu_status;
    }

    fn withdraw_flow(&mut self) -> Result<()> {
        self.stop_mining();
        self.wait_for_worker_shutdown(Duration::from_secs(8));

        execute!(self.stdout, Show, LeaveAlternateScreen)?;
        disable_raw_mode()?;

        let result = self.withdraw_prompt();

        enable_raw_mode()?;
        execute!(self.stdout, EnterAlternateScreen, Hide)?;
        self.refresh_from_chain(true);
        result
    }

    fn wait_for_worker_shutdown(&mut self, timeout: Duration) {
        let started = Instant::now();
        while self.mining.is_some() && started.elapsed() < timeout {
            self.poll_mining_updates();
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    fn withdraw_prompt(&mut self) -> Result<()> {
        clear_plain_screen()?;
        println!("Blockmine withdrawal");
        println!();
        println!("Current wallet   : {}", self.wallet.pubkey);
        println!("SOL balance      : {}", format_sol(self.wallet_sol_lamports));
        println!(
            "BLOC balance     : {}",
            format_token_amount(self.wallet_bloc_raw, self.wallet_bloc_decimals)
        );
        println!();
        println!("Destination wallet:");
        let destination_raw = prompt_line()?;
        let destination = destination_raw
            .trim()
            .parse::<Pubkey>()
            .context("invalid destination wallet address")?;

        println!("SOL amount to withdraw (blank = 0, MAX = all spendable):");
        let sol_raw = prompt_line()?;
        println!("BLOC amount to withdraw (blank = 0, MAX = full wallet balance):");
        let bloc_raw = prompt_line()?;

        let requested_sol_lamports = if sol_raw.trim().eq_ignore_ascii_case("MAX") {
            u64::MAX
        } else {
            parse_decimal_amount(&sol_raw, 9)?
        };
        let requested_bloc_raw = if bloc_raw.trim().eq_ignore_ascii_case("MAX") {
            self.wallet_bloc_raw
        } else {
            parse_decimal_amount(&bloc_raw, self.wallet_bloc_decimals)?
        };

        let summary = sweep_single_session_delegate_wallet(
            &self.cli.rpc,
            self.config.program_id,
            &self.wallet,
            destination,
            requested_sol_lamports,
            requested_bloc_raw,
        )?;

        clear_plain_screen()?;
        if let Some(result) = summary.results.first() {
            if let Some(signature) = &result.signature {
                println!("Withdrawal confirmed.");
                println!();
                println!("Signature   : {signature}");
                println!("SOL sent    : {}", format_sol(result.sent_lamports));
                println!(
                    "BLOC sent   : {}",
                    format_token_amount(result.sent_bloc_raw, self.wallet_bloc_decimals)
                );
                self.last_message = format!("Withdrawal confirmed: {signature}");
            } else {
                println!("Withdrawal skipped.");
                println!();
                println!(
                    "{}",
                    result
                        .skipped_reason
                        .clone()
                        .unwrap_or_else(|| "no transfer was created".to_string())
                );
                self.last_message = result
                    .skipped_reason
                    .clone()
                    .unwrap_or_else(|| "Withdrawal skipped.".to_string());
            }
        }

        println!();
        println!("Press Enter to return to the live console.");
        let _ = prompt_line()?;
        Ok(())
    }
}

fn detect_gpu_environment() -> (Vec<String>, Vec<String>, String) {
    let nvidia_devices = detect_nvidia_devices();
    let opencl_devices = gpu::list_devices()
        .map(|items| {
            items.into_iter()
                .map(|item| format!("{}:{} {}", item.platform_index, item.device_index, item.device_name))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let gpu_status = if !nvidia_devices.is_empty() && !opencl_devices.is_empty() {
        "NVIDIA runtime and OpenCL device detection are both live.".to_string()
    } else if !nvidia_devices.is_empty() {
        "NVIDIA runtime is visible, but OpenCL is still unavailable inside the container.".to_string()
    } else if !opencl_devices.is_empty() {
        "OpenCL devices are visible, but nvidia-smi did not report a mounted NVIDIA runtime.".to_string()
    } else {
        "No GPU runtime was detected yet.".to_string()
    };

    (nvidia_devices, opencl_devices, gpu_status)
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

fn parse_decimal_amount(input: &str, decimals: u8) -> Result<u64> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(0);
    }

    let Some((whole_part, fractional_part)) = trimmed.split_once('.') else {
        return trimmed
            .parse::<u64>()
            .map(|value| value.saturating_mul(10u64.saturating_pow(decimals as u32)))
            .context("invalid numeric amount");
    };

    let whole = if whole_part.trim().is_empty() {
        0
    } else {
        whole_part
            .trim()
            .parse::<u64>()
            .context("invalid whole amount")?
    };
    let mut fractional = fractional_part.trim().to_string();
    if fractional.len() > decimals as usize {
        anyhow::bail!("too many decimal places");
    }
    while fractional.len() < decimals as usize {
        fractional.push('0');
    }

    let fractional_value = if fractional.is_empty() {
        0
    } else {
        fractional.parse::<u64>().context("invalid fractional amount")?
    };

    Ok(whole
        .saturating_mul(10u64.saturating_pow(decimals as u32))
        .saturating_add(fractional_value))
}

fn format_sol(lamports: u64) -> String {
    let whole = lamports / 1_000_000_000;
    let fractional = lamports % 1_000_000_000;
    format!("{whole}.{fractional:09} SOL")
}

fn format_token_amount(raw: u64, decimals: u8) -> String {
    let scale = 10u64.saturating_pow(decimals as u32);
    let whole = raw / scale;
    let fractional = raw % scale;
    format!("{whole}.{fractional:0width$} BLOC", width = decimals as usize)
}

fn format_runtime(duration: Duration) -> String {
    let total = duration.as_secs();
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let seconds = total % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}")
}

fn prompt_line() -> Result<String> {
    let mut input = String::new();
    io::stdout().flush().context("failed to flush stdout")?;
    io::stdin()
        .read_line(&mut input)
        .context("failed to read input")?;
    Ok(input.trim_end().to_string())
}

fn clear_plain_screen() -> Result<()> {
    execute!(stdout(), Clear(ClearType::All), MoveTo(0, 0))?;
    Ok(())
}

fn wrap_text(input: &str, max_width: usize) -> Vec<String> {
    if max_width < 10 || input.len() <= max_width {
        return vec![input.to_string()];
    }

    let mut lines = Vec::new();
    let mut current = String::new();

    for part in input.split(" | ") {
        let candidate = if current.is_empty() {
            part.to_string()
        } else {
            format!("{current} | {part}")
        };

        if candidate.len() > max_width && !current.is_empty() {
            lines.push(current);
            current = part.to_string();
        } else {
            current = candidate;
        }
    }

    if !current.is_empty() {
        lines.push(current);
    }

    if lines.is_empty() {
        vec![input.to_string()]
    } else {
        lines
    }
}
