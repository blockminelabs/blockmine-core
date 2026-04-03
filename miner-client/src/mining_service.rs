use std::collections::VecDeque;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver},
    Arc,
};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use blockmine_program::constants::BLOCK_STATUS_OPEN;
use reqwest::blocking::Client;
use serde::Serialize;
use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, Signer},
};
use spl_associated_token_account::get_associated_token_address;

use crate::config::CliConfig;
use crate::engine::cpu::CpuMiner;
use crate::engine::BackendMode;
use crate::miner_loop::{
    build_engines, format_rate, run_search_round, EngineSelectionConfig, GpuDeviceSelection,
};
use crate::rpc::RpcFacade;
use crate::submitter;

const RPC_RETRY_ATTEMPTS: usize = 6;
const RPC_RETRY_DELAY: Duration = Duration::from_millis(800);
const LIVE_HASHRATE_WINDOW: Duration = Duration::from_secs(15);
const BLOCK_REFRESH_INTERVAL: Duration = Duration::from_millis(1_500);
const LEADERBOARD_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);

#[cfg(target_os = "windows")]
const LEADERBOARD_PLATFORM_LABEL: &str = "windows";
#[cfg(target_os = "macos")]
const LEADERBOARD_PLATFORM_LABEL: &str = "macos";
#[cfg(target_os = "linux")]
const LEADERBOARD_PLATFORM_LABEL: &str = "linux";
#[cfg(all(
    not(target_os = "windows"),
    not(target_os = "macos"),
    not(target_os = "linux")
))]
const LEADERBOARD_PLATFORM_LABEL: &str = "unknown";

#[derive(Debug, Clone)]
pub struct MiningRuntimeOptions {
    pub backend: BackendMode,
    pub batch_size: u64,
    pub gpu_batch_size: Option<u64>,
    pub cpu_threads: usize,
    pub cpu_core_ids: Option<Vec<usize>>,
    pub gpu_devices: Vec<GpuDeviceSelection>,
    pub gpu_platform: usize,
    pub gpu_device: usize,
    pub gpu_local_work_size: Option<usize>,
    pub start_nonce: Option<u64>,
    pub miner_override: Option<Pubkey>,
    pub leaderboard_ingest_url: Option<String>,
    pub platform_detail: Option<String>,
    pub hardware_summary: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MiningSnapshot {
    pub status: String,
    pub backend: BackendMode,
    pub wallet: String,
    pub bloc_ata: String,
    pub current_block_number: u64,
    pub current_reward: u64,
    pub difficulty_bits: u8,
    pub challenge: [u8; 32],
    pub target: [u8; 32],
    pub session_blocks_mined: u64,
    pub session_tokens_mined: u64,
    pub session_hashes: u64,
    pub wallet_blocks_mined: u64,
    pub wallet_tokens_mined: u64,
    pub protocol_blocks_mined: u64,
    pub protocol_treasury_fees: u64,
    pub last_hashrate: String,
    pub last_hashrate_hps: f64,
    pub last_nonce: Option<u64>,
    pub last_hash: Option<[u8; 32]>,
    pub last_signature: Option<String>,
    pub last_event: String,
    pub recent_reports: Vec<String>,
    pub last_error: Option<String>,
}

impl Default for MiningSnapshot {
    fn default() -> Self {
        Self {
            status: "Idle".to_string(),
            backend: BackendMode::Cpu,
            wallet: "-".to_string(),
            bloc_ata: "-".to_string(),
            current_block_number: 0,
            current_reward: 0,
            difficulty_bits: 0,
            challenge: [0u8; 32],
            target: [0u8; 32],
            session_blocks_mined: 0,
            session_tokens_mined: 0,
            session_hashes: 0,
            wallet_blocks_mined: 0,
            wallet_tokens_mined: 0,
            protocol_blocks_mined: 0,
            protocol_treasury_fees: 0,
            last_hashrate: "-".to_string(),
            last_hashrate_hps: 0.0,
            last_nonce: None,
            last_hash: None,
            last_signature: None,
            last_event: "Miner idle".to_string(),
            recent_reports: Vec::new(),
            last_error: None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum MiningUpdate {
    Snapshot(MiningSnapshot),
    Stopped {
        snapshot: MiningSnapshot,
        error: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize)]
struct LeaderboardHeartbeatPayload {
    miner: String,
    reporter: String,
    backend: String,
    platform: String,
    #[serde(rename = "platformDetail")]
    platform_detail: String,
    #[serde(rename = "hardwareSummary")]
    hardware_summary: String,
    #[serde(rename = "hashrateHps")]
    hashrate_hps: u64,
    #[serde(rename = "sessionTokensMined")]
    session_tokens_mined: String,
    #[serde(rename = "sessionBlocksMined")]
    session_blocks_mined: u64,
    #[serde(rename = "walletTokensMined")]
    wallet_tokens_mined: String,
    #[serde(rename = "walletBlocksMined")]
    wallet_blocks_mined: u64,
    #[serde(rename = "currentBlockNumber")]
    current_block_number: u64,
    #[serde(rename = "updatedAt")]
    updated_at: u64,
    #[serde(rename = "signatureHex")]
    signature_hex: String,
}

struct LeaderboardReporter {
    client: Option<Client>,
    ingest_url: Option<String>,
    last_sent_at: Option<Instant>,
    miner_pubkey: Pubkey,
    reporter_pubkey: Pubkey,
    platform_detail: String,
    hardware_summary: String,
}

impl LeaderboardReporter {
    fn new(
        ingest_url: Option<String>,
        miner_pubkey: Pubkey,
        reporter_pubkey: Pubkey,
        platform_detail: Option<String>,
        hardware_summary: Option<String>,
    ) -> Result<Self> {
        let trimmed_url = ingest_url.and_then(|value| {
            let trimmed = value.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        });
        let client = if trimmed_url.is_some() {
            Some(
                Client::builder()
                    .timeout(Duration::from_secs(10))
                    .user_agent("Blockmine Miner/1.0 (+https://blockmine.dev)")
                    .build()
                    .context("failed to build the leaderboard HTTP client")?,
            )
        } else {
            None
        };

        Ok(Self {
            client,
            ingest_url: trimmed_url,
            last_sent_at: None,
            miner_pubkey,
            reporter_pubkey,
            platform_detail: sanitize_metadata_label(platform_detail),
            hardware_summary: sanitize_metadata_label(hardware_summary),
        })
    }

    fn maybe_send(&mut self, signer: &Keypair, snapshot: &MiningSnapshot, force: bool) {
        if self.client.is_none() || self.ingest_url.is_none() {
            return;
        }

        if !force
            && self
                .last_sent_at
                .is_some_and(|last_sent_at| last_sent_at.elapsed() < LEADERBOARD_HEARTBEAT_INTERVAL)
        {
            return;
        }

        let payload = build_leaderboard_heartbeat_payload(
            self.miner_pubkey,
            self.reporter_pubkey,
            snapshot,
            &self.platform_detail,
            &self.hardware_summary,
        );
        let message = build_leaderboard_heartbeat_message(&payload);
        let signature_hex = hex::encode(signer.sign_message(message.as_bytes()).as_ref());
        let signed_payload = LeaderboardHeartbeatPayload {
            signature_hex,
            ..payload
        };

        if let (Some(client), Some(url)) = (&self.client, &self.ingest_url) {
            let response = client
                .post(url)
                .json(&signed_payload)
                .send()
                .and_then(|response| response.error_for_status());
            if response.is_err() {
                return;
            }
        }

        self.last_sent_at = Some(Instant::now());
    }
}

pub struct MiningHandle {
    stop_requested: Arc<AtomicBool>,
    receiver: Receiver<MiningUpdate>,
}

impl MiningHandle {
    pub fn start(
        config: CliConfig,
        signer: Keypair,
        options: MiningRuntimeOptions,
    ) -> Result<Self> {
        let stop_requested = Arc::new(AtomicBool::new(false));
        let stop_signal = Arc::clone(&stop_requested);
        let (sender, receiver) = mpsc::channel();

        thread::Builder::new()
            .name("blockmine-gui-miner".to_string())
            .spawn(move || {
                let mut latest_snapshot = MiningSnapshot::default();
                let result = worker_loop(
                    &config,
                    signer,
                    options,
                    &stop_signal,
                    &sender,
                    &mut latest_snapshot,
                );

                if let Err(error) = result {
                    latest_snapshot.status = "Stopped".to_string();
                    latest_snapshot.last_error = Some(error.to_string());
                    latest_snapshot.last_event = first_line(&error.to_string()).to_string();
                    let _ = sender.send(MiningUpdate::Stopped {
                        snapshot: latest_snapshot,
                        error: Some(error.to_string()),
                    });
                }
            })
            .map_err(|error| anyhow::anyhow!("failed to spawn the miner thread: {error}"))?;

        Ok(Self {
            stop_requested,
            receiver,
        })
    }

    pub fn stop(&self) {
        self.stop_requested.store(true, Ordering::Relaxed);
    }

    pub fn try_recv(&self) -> std::result::Result<MiningUpdate, mpsc::TryRecvError> {
        self.receiver.try_recv()
    }
}

fn worker_loop(
    config: &CliConfig,
    signer: Keypair,
    options: MiningRuntimeOptions,
    stop_requested: &Arc<AtomicBool>,
    sender: &mpsc::Sender<MiningUpdate>,
    latest_snapshot: &mut MiningSnapshot,
) -> Result<()> {
    let rpc = RpcFacade::new(config);
    let miner_pubkey = options.miner_override.unwrap_or_else(|| signer.pubkey());
    let session_mode = miner_pubkey != signer.pubkey();

    if !session_mode && fetch_miner_stats_with_retry(&rpc, &miner_pubkey).is_err() {
        let _ = submitter::register_miner(&rpc, &signer, [0u8; 32])?;
    }

    let protocol = fetch_protocol_config_with_retry(&rpc)?;
    let mut current_block = fetch_current_block_with_retry(&rpc)?;
    let miner_stats = fetch_miner_stats_with_retry(&rpc, &miner_pubkey)?;
    let miner_token_account = get_associated_token_address(&miner_pubkey, &protocol.bloc_mint);

    let engines = build_engines(EngineSelectionConfig {
        mode: options.backend,
        cpu_threads: options.cpu_threads,
        cpu_core_ids: options.cpu_core_ids.clone(),
        gpu_devices: options.gpu_devices.clone(),
        gpu_platform: options.gpu_platform,
        gpu_device: options.gpu_device,
        gpu_local_work_size: options.gpu_local_work_size,
    });
    let mut next_nonce = options.start_nonce.unwrap_or_else(CpuMiner::random_nonce);
    let resolved_gpu_batch_size = options.gpu_batch_size.unwrap_or(options.batch_size);
    let mut offchain_hashrate_samples: VecDeque<(Instant, f64)> = VecDeque::new();
    let mut stable_offchain_hashrate_hps = 0.0f64;
    let mut last_block_refresh_at = Instant::now();
    let (block_refresh_sender, block_refresh_receiver) = mpsc::channel();
    let block_refresh_config = config.clone();
    let block_refresh_stop = Arc::clone(stop_requested);
    thread::Builder::new()
        .name("blockmine-block-refresh".to_string())
        .spawn(move || {
            let rpc = RpcFacade::new(&block_refresh_config);
            while !block_refresh_stop.load(Ordering::Relaxed) {
                let update =
                    fetch_current_block_with_retry(&rpc).map_err(|error| error.to_string());
                let _ = block_refresh_sender.send(update);

                let poll_chunks = ((BLOCK_REFRESH_INTERVAL.as_millis() / 100).max(1)) as usize;
                for _ in 0..poll_chunks {
                    if block_refresh_stop.load(Ordering::Relaxed) {
                        break;
                    }
                    thread::sleep(Duration::from_millis(100));
                }
            }
        })
        .context("failed to spawn the block refresh thread")?;
    let mut state = WorkerState::new(
        options.backend,
        miner_pubkey.to_string(),
        miner_token_account.to_string(),
        current_block.clone(),
        miner_stats,
        protocol,
    );
    let mut leaderboard_reporter = LeaderboardReporter::new(
        options.leaderboard_ingest_url.clone(),
        miner_pubkey,
        signer.pubkey(),
        options.platform_detail.clone(),
        options.hardware_summary.clone(),
    )?;
    state.emit(sender, latest_snapshot);
    leaderboard_reporter.maybe_send(&signer, &state.snapshot, true);

    while !stop_requested.load(Ordering::Relaxed) {
        while let Ok(update) = block_refresh_receiver.try_recv() {
            match update {
                Ok(block) => {
                    current_block = block;
                    last_block_refresh_at = Instant::now();
                    state.snapshot.last_error = None;
                }
                Err(error) => {
                    state.snapshot.last_error = Some(error);
                }
            }
        }

        if last_block_refresh_at.elapsed() >= Duration::from_secs(8)
            && state.snapshot.last_error.is_none()
        {
            state.snapshot.last_error = Some("Relay state refresh is delayed.".to_string());
        }

        if should_rotate_stale_block(&current_block) {
            state.snapshot.status = format!("Rotating stale block {}", current_block.block_number);
            state.snapshot.last_event =
                "Current block exceeded TTL, requesting stale-block recovery".to_string();
            state.snapshot.last_error = None;
            state.emit(sender, latest_snapshot);

            match submitter::rotate_stale_block(&rpc, &signer) {
                Ok(signature) => {
                    let latest_protocol = fetch_protocol_config_with_retry(&rpc)?;
                    let latest_block = fetch_current_block_with_retry(&rpc)?;
                    current_block = latest_block.clone();
                    last_block_refresh_at = Instant::now();
                    state.snapshot.protocol_blocks_mined = latest_protocol.total_blocks_mined;
                    state.snapshot.protocol_treasury_fees =
                        latest_protocol.total_treasury_fees_distributed;
                    state.update_block(&latest_block);
                    state.snapshot.last_signature = Some(signature.to_string());
                    state.snapshot.status =
                        format!("Recovered stale block {}", current_block.block_number);
                    state.snapshot.last_event = format!(
                        "Opened block {} after stale rotation",
                        latest_block.block_number
                    );
                    state.snapshot.last_error = None;
                }
                Err(error) => {
                    state.snapshot.status = "Stale-block rotation raced".to_string();
                    state.snapshot.last_event =
                        format!("Rotate error: {}", first_line(&error.to_string()));
                    state.snapshot.last_error = Some(error.to_string());
                }
            }

            state.emit(sender, latest_snapshot);
            leaderboard_reporter.maybe_send(&signer, &state.snapshot, false);
            thread::sleep(Duration::from_millis(300));
            continue;
        }
        state.update_block(&current_block);
        state.snapshot.status = format!("Mining block {}", current_block.block_number);
        state.snapshot.last_event = "Searching for a valid nonce".to_string();
        state.snapshot.last_error = None;
        state.emit(sender, latest_snapshot);
        leaderboard_reporter.maybe_send(&signer, &state.snapshot, false);

        let input = crate::engine::SearchInput {
            challenge: current_block.challenge,
            miner: miner_pubkey,
            target: current_block.difficulty_target,
            start_nonce: next_nonce,
            max_attempts: options.batch_size,
        };

        let outcome = run_search_round(
            &engines,
            options.backend,
            options.batch_size,
            resolved_gpu_batch_size,
            &input,
        )?;
        let round_hashes = outcome
            .reports
            .iter()
            .map(|report| report.attempts)
            .sum::<u64>();
        let round_elapsed = outcome
            .reports
            .iter()
            .map(|report| report.elapsed)
            .max()
            .unwrap_or_else(|| Duration::from_secs(1));
        let clean_round_hashrate_hps = clean_round_hashrate_hps(&outcome.reports);
        if let Some(clean_hashrate_hps) = clean_round_hashrate_hps {
            stable_offchain_hashrate_hps =
                update_live_hashrate_window(&mut offchain_hashrate_samples, clean_hashrate_hps);
        } else if stable_offchain_hashrate_hps <= 0.0 {
            stable_offchain_hashrate_hps =
                round_hashes as f64 / round_elapsed.as_secs_f64().max(0.000_001);
        }

        state.snapshot.session_hashes = state.snapshot.session_hashes.saturating_add(round_hashes);
        state.snapshot.last_hashrate = format_hashrate_hps(stable_offchain_hashrate_hps);
        state.snapshot.last_hashrate_hps = stable_offchain_hashrate_hps;
        state.snapshot.recent_reports = outcome
            .reports
            .iter()
            .map(|report| {
                format!(
                    "{} | attempts={} | found={} | hashrate={}",
                    report.backend,
                    report.attempts,
                    report.found,
                    format_rate(report.attempts, report.elapsed)
                )
            })
            .collect();

        if let Some(solution) = outcome.solution {
            let solved_block = current_block.block_number;
            state.snapshot.status = format!("Submitting solution for block {}", solved_block);
            state.snapshot.last_nonce = Some(solution.nonce);
            state.snapshot.last_hash = Some(solution.hash);
            state.snapshot.last_event = format!(
                "Block candidate found via {} after {} attempts",
                solution.backend, solution.attempts
            );
            state.snapshot.last_error = None;
            state.emit(sender, latest_snapshot);

            let latest_chain_block = fetch_current_block_with_retry(&rpc)?;
            current_block = latest_chain_block.clone();
            last_block_refresh_at = Instant::now();
            if latest_chain_block.block_number != solved_block
                || latest_chain_block.status != BLOCK_STATUS_OPEN
            {
                state.update_block(&latest_chain_block);
                state.snapshot.status = "Candidate raced".to_string();
                state.snapshot.last_event = format!(
                    "Block {} moved before submission. Resuming on block {}.",
                    solved_block, latest_chain_block.block_number
                );
                state.snapshot.last_error = None;
                state.emit(sender, latest_snapshot);
                leaderboard_reporter.maybe_send(&signer, &state.snapshot, false);
                next_nonce = outcome.next_nonce;
                continue;
            }

            if latest_chain_block.expires_at > 0
                && unix_timestamp_now() > latest_chain_block.expires_at
            {
                state.snapshot.status = "Candidate expired".to_string();
                state.snapshot.last_event = format!(
                    "Block {} expired before submission. Rotating stale block.",
                    solved_block
                );
                state.snapshot.last_error = None;
                state.emit(sender, latest_snapshot);

                match submitter::rotate_stale_block(&rpc, &signer) {
                    Ok(signature) => {
                        let latest_protocol = fetch_protocol_config_with_retry(&rpc)?;
                        let latest_block = fetch_current_block_with_retry(&rpc)?;
                        current_block = latest_block.clone();
                        last_block_refresh_at = Instant::now();
                        state.snapshot.protocol_blocks_mined = latest_protocol.total_blocks_mined;
                        state.snapshot.protocol_treasury_fees =
                            latest_protocol.total_treasury_fees_distributed;
                        state.update_block(&latest_block);
                        state.snapshot.last_signature = Some(signature.to_string());
                        state.snapshot.status = format!("Recovered stale block {}", solved_block);
                        state.snapshot.last_event = format!(
                            "Opened block {} after stale rotation",
                            latest_block.block_number
                        );
                        state.snapshot.last_error = None;
                    }
                    Err(error) => {
                        state.snapshot.status = "Stale-block rotation raced".to_string();
                        state.snapshot.last_event =
                            format!("Rotate error: {}", first_line(&error.to_string()));
                        state.snapshot.last_error = Some(error.to_string());
                    }
                }

                state.emit(sender, latest_snapshot);
                leaderboard_reporter.maybe_send(&signer, &state.snapshot, false);
                next_nonce = outcome.next_nonce;
                continue;
            }

            let submit_result = if session_mode {
                submitter::submit_solution_with_session(&rpc, &signer, miner_pubkey, solution.nonce)
            } else {
                submitter::submit_solution(&rpc, &signer, solution.nonce)
            };

            match submit_result {
                Ok(signature) => {
                    let latest_protocol = fetch_protocol_config_with_retry(&rpc)?;
                    let latest_block = fetch_current_block_with_retry(&rpc)?;
                    let latest_miner_stats = fetch_miner_stats_with_retry(&rpc, &miner_pubkey)?;
                    current_block = latest_block.clone();
                    last_block_refresh_at = Instant::now();

                    let mined_delta = latest_miner_stats
                        .total_rewards_earned
                        .saturating_sub(state.snapshot.wallet_tokens_mined);
                    let block_delta = latest_miner_stats
                        .valid_blocks_found
                        .saturating_sub(state.snapshot.wallet_blocks_mined);

                    state.snapshot.session_tokens_mined = state
                        .snapshot
                        .session_tokens_mined
                        .saturating_add(mined_delta);
                    state.snapshot.session_blocks_mined = state
                        .snapshot
                        .session_blocks_mined
                        .saturating_add(block_delta);
                    state.snapshot.wallet_blocks_mined = latest_miner_stats.valid_blocks_found;
                    state.snapshot.wallet_tokens_mined = latest_miner_stats.total_rewards_earned;
                    state.snapshot.protocol_blocks_mined = latest_protocol.total_blocks_mined;
                    state.snapshot.protocol_treasury_fees =
                        latest_protocol.total_treasury_fees_distributed;
                    state.update_block(&latest_block);
                    state.snapshot.last_signature = Some(signature.to_string());
                    state.snapshot.status = format!("Block {} accepted", solved_block);
                    state.snapshot.last_event = format!(
                        "Accepted on-chain. Session reward +{} BLOC",
                        crate::ui::format_bloc(mined_delta)
                    );
                    state.snapshot.last_error = None;
                }
                Err(error) => {
                    let full_error = error.to_string();
                    let friendly_error = friendly_submit_error(
                        session_mode,
                        &full_error,
                        state.snapshot.current_block_number,
                    );
                    state.snapshot.status = friendly_error.status;
                    state.snapshot.last_event = friendly_error.last_event;
                    state.snapshot.last_error = None;
                }
            }
        } else {
            state.snapshot.status = format!("Mining block {}", current_block.block_number);
            state.snapshot.last_event = "No valid nonce in last batch, continuing".to_string();
            state.snapshot.last_error = None;
        }

        next_nonce = outcome.next_nonce;
        state.emit(sender, latest_snapshot);
        leaderboard_reporter.maybe_send(&signer, &state.snapshot, false);
    }

    state.snapshot.status = "Stopped".to_string();
    state.snapshot.last_event = "Mining stopped by user".to_string();
    state.snapshot.last_error = None;
    leaderboard_reporter.maybe_send(&signer, &state.snapshot, true);
    latest_snapshot.clone_from(&state.snapshot);
    let _ = sender.send(MiningUpdate::Stopped {
        snapshot: state.snapshot,
        error: None,
    });

    Ok(())
}

fn fetch_protocol_config_with_retry(
    rpc: &RpcFacade,
) -> Result<blockmine_program::state::ProtocolConfig> {
    retry_rpc_call("protocol config", || rpc.fetch_protocol_config())
}

fn fetch_current_block_with_retry(
    rpc: &RpcFacade,
) -> Result<blockmine_program::state::CurrentBlock> {
    retry_rpc_call("current block", || rpc.fetch_current_block())
}

fn fetch_miner_stats_with_retry(
    rpc: &RpcFacade,
    miner: &Pubkey,
) -> Result<blockmine_program::state::MinerStats> {
    retry_rpc_call("miner stats", || rpc.fetch_miner_stats(miner))
}

fn retry_rpc_call<T, F>(label: &str, mut operation: F) -> Result<T>
where
    F: FnMut() -> Result<T>,
{
    let mut last_error = None;

    for attempt in 1..=RPC_RETRY_ATTEMPTS {
        match operation() {
            Ok(value) => return Ok(value),
            Err(error) => {
                last_error = Some(error);
                if attempt < RPC_RETRY_ATTEMPTS {
                    thread::sleep(RPC_RETRY_DELAY);
                }
            }
        }
    }

    Err(last_error.unwrap()).with_context(|| {
        format!(
            "failed to fetch {label} after {} attempts",
            RPC_RETRY_ATTEMPTS
        )
    })
}

fn update_live_hashrate_window(
    samples: &mut VecDeque<(Instant, f64)>,
    next_hashrate_hps: f64,
) -> f64 {
    let now = Instant::now();
    let sanitized = if next_hashrate_hps.is_finite() && next_hashrate_hps > 0.0 {
        next_hashrate_hps
    } else {
        0.0
    };

    if sanitized > 0.0 {
        if let Some((bucket_started_at, bucket_peak_hps)) = samples.back_mut() {
            if now.duration_since(*bucket_started_at) < Duration::from_secs(1) {
                *bucket_peak_hps = bucket_peak_hps.max(sanitized);
            } else {
                samples.push_back((now, sanitized));
            }
        } else {
            samples.push_back((now, sanitized));
        }
    }

    while let Some((timestamp, _)) = samples.front() {
        if now.duration_since(*timestamp) > LIVE_HASHRATE_WINDOW {
            samples.pop_front();
        } else {
            break;
        }
    }

    if samples.is_empty() {
        return 0.0;
    }

    let mut window: Vec<f64> = samples.iter().map(|(_, value)| *value).collect();
    window.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    let mid = window.len() / 2;
    if window.len() % 2 == 0 {
        (window[mid - 1] + window[mid]) * 0.5
    } else {
        window[mid]
    }
}

fn clean_round_hashrate_hps(reports: &[crate::miner_loop::BatchReport]) -> Option<f64> {
    let clean_reports: Vec<&crate::miner_loop::BatchReport> =
        reports.iter().filter(|report| !report.found).collect();
    if clean_reports.is_empty() {
        return None;
    }

    let total_hashes = clean_reports
        .iter()
        .map(|report| report.attempts)
        .sum::<u64>();
    let max_elapsed = clean_reports
        .iter()
        .map(|report| report.elapsed)
        .max()
        .unwrap_or_else(|| Duration::from_secs(1));

    Some(total_hashes as f64 / max_elapsed.as_secs_f64().max(0.000_001))
}

fn format_hashrate_hps(rate_hps: f64) -> String {
    if !rate_hps.is_finite() || rate_hps <= 0.0 {
        return "0 H/s".to_string();
    }

    let units = [
        ("TH/s", 1_000_000_000_000.0_f64),
        ("GH/s", 1_000_000_000.0_f64),
        ("MH/s", 1_000_000.0_f64),
        ("kH/s", 1_000.0_f64),
    ];

    for (label, scale) in units {
        if rate_hps >= scale {
            return format!("{:.2} {}", rate_hps / scale, label);
        }
    }

    format!("{:.0} H/s", rate_hps)
}

fn build_leaderboard_heartbeat_payload(
    miner_pubkey: Pubkey,
    reporter_pubkey: Pubkey,
    snapshot: &MiningSnapshot,
    platform_detail: &str,
    hardware_summary: &str,
) -> LeaderboardHeartbeatPayload {
    LeaderboardHeartbeatPayload {
        miner: miner_pubkey.to_string(),
        reporter: reporter_pubkey.to_string(),
        backend: backend_label(snapshot.backend).to_string(),
        platform: LEADERBOARD_PLATFORM_LABEL.to_string(),
        platform_detail: platform_detail.to_string(),
        hardware_summary: hardware_summary.to_string(),
        hashrate_hps: snapshot.last_hashrate_hps.max(0.0).round() as u64,
        session_tokens_mined: snapshot.session_tokens_mined.to_string(),
        session_blocks_mined: snapshot.session_blocks_mined,
        wallet_tokens_mined: snapshot.wallet_tokens_mined.to_string(),
        wallet_blocks_mined: snapshot.wallet_blocks_mined,
        current_block_number: snapshot.current_block_number,
        updated_at: unix_timestamp_now().max(0) as u64,
        signature_hex: String::new(),
    }
}

fn build_leaderboard_heartbeat_message(payload: &LeaderboardHeartbeatPayload) -> String {
    [
        "v3".to_string(),
        payload.miner.clone(),
        payload.reporter.clone(),
        payload.backend.clone(),
        payload.platform.clone(),
        payload.platform_detail.clone(),
        payload.hardware_summary.clone(),
        payload.hashrate_hps.to_string(),
        payload.session_tokens_mined.clone(),
        payload.session_blocks_mined.to_string(),
        payload.wallet_tokens_mined.clone(),
        payload.wallet_blocks_mined.to_string(),
        payload.current_block_number.to_string(),
        payload.updated_at.to_string(),
    ]
    .join("|")
}

fn sanitize_metadata_label(value: Option<String>) -> String {
    value
        .unwrap_or_default()
        .chars()
        .filter(|character| !character.is_control())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(120)
        .collect()
}

fn backend_label(mode: BackendMode) -> &'static str {
    match mode {
        BackendMode::Cpu => "cpu",
        BackendMode::Gpu => "gpu",
        BackendMode::Both => "both",
    }
}

struct WorkerState {
    snapshot: MiningSnapshot,
}

impl WorkerState {
    fn new(
        backend: BackendMode,
        wallet: String,
        bloc_ata: String,
        current_block: blockmine_program::state::CurrentBlock,
        miner_stats: blockmine_program::state::MinerStats,
        protocol: blockmine_program::state::ProtocolConfig,
    ) -> Self {
        Self {
            snapshot: MiningSnapshot {
                status: format!("Ready to mine block {}", current_block.block_number),
                backend,
                wallet,
                bloc_ata,
                current_block_number: current_block.block_number,
                current_reward: current_block.block_reward,
                difficulty_bits: current_block.difficulty_bits,
                challenge: current_block.challenge,
                target: current_block.difficulty_target,
                session_blocks_mined: 0,
                session_tokens_mined: 0,
                session_hashes: 0,
                wallet_blocks_mined: miner_stats.valid_blocks_found,
                wallet_tokens_mined: miner_stats.total_rewards_earned,
                protocol_blocks_mined: protocol.total_blocks_mined,
                protocol_treasury_fees: protocol.total_treasury_fees_distributed,
                last_hashrate: "-".to_string(),
                last_hashrate_hps: 0.0,
                last_nonce: None,
                last_hash: None,
                last_signature: None,
                last_event: "Miner initialized".to_string(),
                recent_reports: Vec::new(),
                last_error: None,
            },
        }
    }

    fn update_block(&mut self, block: &blockmine_program::state::CurrentBlock) {
        self.snapshot.current_block_number = block.block_number;
        self.snapshot.current_reward = block.block_reward;
        self.snapshot.difficulty_bits = block.difficulty_bits;
        self.snapshot.challenge = block.challenge;
        self.snapshot.target = block.difficulty_target;
    }

    fn emit(&self, sender: &mpsc::Sender<MiningUpdate>, latest_snapshot: &mut MiningSnapshot) {
        latest_snapshot.clone_from(&self.snapshot);
        let _ = sender.send(MiningUpdate::Snapshot(self.snapshot.clone()));
    }
}

fn first_line(input: &str) -> &str {
    input.lines().next().unwrap_or(input)
}

struct FriendlySubmitError {
    status: String,
    last_event: String,
}

fn friendly_submit_error(
    session_mode: bool,
    error: &str,
    block_number: u64,
) -> FriendlySubmitError {
    let lower = error.to_ascii_lowercase();
    if lower.contains("insufficient lamports") {
        if session_mode {
            return FriendlySubmitError {
                status: "Session needs SOL top-up".to_string(),
                last_event: format!(
                    "The Phantom session key ran out of SOL on block {}. Reconnect Phantom to preload a fresh desktop session budget.",
                    block_number
                ),
            };
        }

        return FriendlySubmitError {
            status: "Wallet needs more SOL".to_string(),
            last_event: format!(
                "The mining wallet did not have enough SOL to submit block {} and pay the protocol fee.",
                block_number
            ),
        };
    }

    if lower.contains("blockexpired") || lower.contains("block expired") {
        return FriendlySubmitError {
            status: "Block expired".to_string(),
            last_event: format!(
                "Block {} expired before the submit landed on-chain. The miner will rotate to the next live block.",
                block_number
            ),
        };
    }

    if lower.contains("blockclosed") || lower.contains("block closed") {
        return FriendlySubmitError {
            status: "Block already moved".to_string(),
            last_event: format!(
                "Another on-chain result closed block {} before this submit landed. Mining will resume on the current block.",
                block_number
            ),
        };
    }

    if lower.contains("sessioninactive") || lower.contains("session inactive") {
        return FriendlySubmitError {
            status: "Session cap reached".to_string(),
            last_event: "The delegated wallet session is no longer active. Reconnect your wallet to arm a fresh mining session.".to_string(),
        };
    }

    if lower.contains("sessionexpired") || lower.contains("session expired") {
        return FriendlySubmitError {
            status: "Session expired".to_string(),
            last_event: "The delegated wallet session expired. Reconnect your wallet to start a fresh session.".to_string(),
        };
    }

    FriendlySubmitError {
        status: "Submission failed".to_string(),
        last_event: format!("Submit error: {}", first_line(error)),
    }
}

fn should_rotate_stale_block(block: &blockmine_program::state::CurrentBlock) -> bool {
    block.expires_at > 0 && unix_timestamp_now() > block.expires_at
}

fn unix_timestamp_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}
