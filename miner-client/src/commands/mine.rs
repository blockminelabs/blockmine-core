use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use solana_sdk::signature::{Keypair, Signer};
use spl_associated_token_account::get_associated_token_address;

use crate::config::CliConfig;
use crate::engine::cpu::CpuMiner;
use crate::engine::{BackendMode, SearchInput};
use crate::miner_loop::{build_engines, format_rate, run_search_round, EngineSelectionConfig};
use crate::rpc::RpcFacade;
use crate::submitter;
use crate::ui::{format_bloc, MineUi, MineUiSnapshot};
use crate::wallet::load_keypair;

#[derive(Debug, Clone, Copy)]
pub struct MineOptions {
    pub backend: BackendMode,
    pub batch_size: u64,
    pub gpu_batch_size: Option<u64>,
    pub cpu_threads: usize,
    pub gpu_platform: usize,
    pub gpu_device: usize,
    pub gpu_local_work_size: Option<usize>,
    pub start_nonce: Option<u64>,
}

pub fn run(
    config: &CliConfig,
    backend: BackendMode,
    batch_size: u64,
    gpu_batch_size: Option<u64>,
    cpu_threads: usize,
    gpu_platform: usize,
    gpu_device: usize,
    gpu_local_work_size: Option<usize>,
    start_nonce: Option<u64>,
) -> Result<()> {
    let signer = load_keypair(config)?;
    run_with_signer(
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
            start_nonce,
        },
    )
}

pub fn run_with_signer(config: &CliConfig, signer: Keypair, options: MineOptions) -> Result<()> {
    let rpc = RpcFacade::new(config);
    if rpc.fetch_miner_stats(&signer.pubkey()).is_err() {
        let _ = submitter::register_miner(&rpc, &signer, [0u8; 32])?;
    }

    let protocol = rpc.fetch_protocol_config()?;
    let current_block = rpc.fetch_current_block()?;
    let miner_stats = rpc.fetch_miner_stats(&signer.pubkey())?;
    let miner_token_account = get_associated_token_address(&signer.pubkey(), &protocol.bloc_mint);

    let mut ui = MineUi::new()?;
    let stop_requested = Arc::new(AtomicBool::new(false));
    let stop_signal = Arc::clone(&stop_requested);
    ctrlc::set_handler(move || {
        stop_signal.store(true, Ordering::Relaxed);
    })
    .context("failed to install Ctrl+C handler")?;

    let engines = build_engines(EngineSelectionConfig {
        mode: options.backend,
        cpu_threads: options.cpu_threads,
        cpu_core_ids: None,
        gpu_devices: Vec::new(),
        gpu_platform: options.gpu_platform,
        gpu_device: options.gpu_device,
        gpu_local_work_size: options.gpu_local_work_size,
    });
    let mut next_nonce = options.start_nonce.unwrap_or_else(CpuMiner::random_nonce);
    let resolved_gpu_batch_size = options.gpu_batch_size.unwrap_or(options.batch_size);
    let mut session = SessionState::new(
        options.backend,
        signer.pubkey().to_string(),
        miner_token_account.to_string(),
        current_block,
        miner_stats,
        protocol,
    );
    ui.render(&session.snapshot())?;

    while !stop_requested.load(Ordering::Relaxed) {
        let current_block = rpc.fetch_current_block()?;
        if should_rotate_stale_block(&current_block) {
            session.status = format!("Rotating stale block {}", current_block.block_number);
            session.last_event =
                "Current block exceeded TTL, requesting stale-block recovery".to_string();
            ui.render(&session.snapshot())?;

            match submitter::rotate_stale_block(&rpc, &signer) {
                Ok(signature) => {
                    let latest_protocol = rpc.fetch_protocol_config()?;
                    let latest_block = rpc.fetch_current_block()?;
                    session.protocol_blocks_mined = latest_protocol.total_blocks_mined;
                    session.protocol_treasury_fees =
                        latest_protocol.total_treasury_fees_distributed;
                    session.update_block(&latest_block);
                    session.last_signature = Some(signature.to_string());
                    session.status =
                        format!("Recovered stale block {}", current_block.block_number);
                    session.last_event = format!(
                        "Opened block {} after stale rotation",
                        latest_block.block_number
                    );
                }
                Err(error) => {
                    session.status = "Stale-block rotation raced".to_string();
                    session.last_event =
                        format!("Rotate error: {}", first_line(&error.to_string()));
                }
            }

            ui.render(&session.snapshot())?;
            continue;
        }
        session.update_block(&current_block);
        session.status = format!("Mining block {}", current_block.block_number);
        session.last_event = "Searching for a valid nonce".to_string();
        ui.render(&session.snapshot())?;

        let input = SearchInput {
            challenge: current_block.challenge,
            miner: signer.pubkey(),
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
        session.session_hashes = session.session_hashes.saturating_add(round_hashes);
        session.last_hashrate = format_rate(round_hashes, round_elapsed);
        session.recent_reports = outcome
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
            session.status = format!("Submitting solution for block {}", solved_block);
            session.last_nonce = Some(solution.nonce);
            session.last_hash = Some(solution.hash);
            session.last_event = format!(
                "Block candidate found via {} after {} attempts",
                solution.backend, solution.attempts
            );
            ui.render(&session.snapshot())?;

            match submitter::submit_solution(&rpc, &signer, solution.nonce) {
                Ok(signature) => {
                    let latest_protocol = rpc.fetch_protocol_config()?;
                    let latest_block = rpc.fetch_current_block()?;
                    let latest_miner_stats = rpc.fetch_miner_stats(&signer.pubkey())?;

                    let mined_delta = latest_miner_stats
                        .total_rewards_earned
                        .saturating_sub(session.wallet_tokens_mined);
                    let block_delta = latest_miner_stats
                        .valid_blocks_found
                        .saturating_sub(session.wallet_blocks_mined);

                    session.session_tokens_mined =
                        session.session_tokens_mined.saturating_add(mined_delta);
                    session.session_blocks_mined =
                        session.session_blocks_mined.saturating_add(block_delta);
                    session.wallet_blocks_mined = latest_miner_stats.valid_blocks_found;
                    session.wallet_tokens_mined = latest_miner_stats.total_rewards_earned;
                    session.protocol_blocks_mined = latest_protocol.total_blocks_mined;
                    session.protocol_treasury_fees =
                        latest_protocol.total_treasury_fees_distributed;
                    session.update_block(&latest_block);
                    session.last_signature = Some(signature.to_string());
                    session.status = format!("Block {} accepted", solved_block);
                    session.last_event = format!(
                        "Accepted on-chain. Session reward +{} BLOC",
                        format_bloc(mined_delta)
                    );
                }
                Err(error) => {
                    session.status = "Submission failed".to_string();
                    session.last_event =
                        format!("Submit error: {}", first_line(&error.to_string()));
                }
            }

            ui.render(&session.snapshot())?;
        } else {
            session.status = format!("Mining block {}", current_block.block_number);
            session.last_event = "No valid nonce in last batch, continuing".to_string();
            ui.render(&session.snapshot())?;
        }

        next_nonce = outcome.next_nonce;
    }

    session.status = "Stopped".to_string();
    session.last_event = "Mining stopped by user".to_string();
    ui.render(&session.snapshot())?;
    ui.shutdown()?;

    println!("BlockMine miner stopped.");
    println!("Session blocks mined : {}", session.session_blocks_mined);
    println!(
        "Session BLOC mined   : {}",
        format_bloc(session.session_tokens_mined)
    );
    println!(
        "Last tx              : {}",
        session.last_signature.unwrap_or_else(|| "-".to_string())
    );
    Ok(())
}

struct SessionState {
    status: String,
    backend: BackendMode,
    wallet: String,
    bloc_ata: String,
    current_block_number: u64,
    current_reward: u64,
    difficulty_bits: u8,
    challenge: [u8; 32],
    target: [u8; 32],
    session_blocks_mined: u64,
    session_tokens_mined: u64,
    session_hashes: u64,
    wallet_blocks_mined: u64,
    wallet_tokens_mined: u64,
    protocol_blocks_mined: u64,
    protocol_treasury_fees: u64,
    last_hashrate: String,
    last_nonce: Option<u64>,
    last_hash: Option<[u8; 32]>,
    last_signature: Option<String>,
    last_event: String,
    recent_reports: Vec<String>,
}

impl SessionState {
    fn new(
        backend: BackendMode,
        wallet: String,
        bloc_ata: String,
        current_block: blockmine_program::state::CurrentBlock,
        miner_stats: blockmine_program::state::MinerStats,
        protocol: blockmine_program::state::ProtocolConfig,
    ) -> Self {
        Self {
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
            last_nonce: None,
            last_hash: None,
            last_signature: None,
            last_event: "Miner initialized".to_string(),
            recent_reports: Vec::new(),
        }
    }

    fn update_block(&mut self, block: &blockmine_program::state::CurrentBlock) {
        self.current_block_number = block.block_number;
        self.current_reward = block.block_reward;
        self.difficulty_bits = block.difficulty_bits;
        self.challenge = block.challenge;
        self.target = block.difficulty_target;
    }

    fn snapshot(&self) -> MineUiSnapshot {
        MineUiSnapshot {
            status: self.status.clone(),
            backend: self.backend,
            wallet: self.wallet.clone(),
            bloc_ata: self.bloc_ata.clone(),
            current_block_number: self.current_block_number,
            current_reward: self.current_reward,
            difficulty_bits: self.difficulty_bits,
            challenge: self.challenge,
            target: self.target,
            session_blocks_mined: self.session_blocks_mined,
            session_tokens_mined: self.session_tokens_mined,
            session_hashes: self.session_hashes,
            wallet_blocks_mined: self.wallet_blocks_mined,
            wallet_tokens_mined: self.wallet_tokens_mined,
            protocol_blocks_mined: self.protocol_blocks_mined,
            protocol_treasury_fees: self.protocol_treasury_fees,
            last_hashrate: self.last_hashrate.clone(),
            last_nonce: self.last_nonce,
            last_hash: self.last_hash,
            last_signature: self.last_signature.clone(),
            last_event: self.last_event.clone(),
            recent_reports: self.recent_reports.clone(),
        }
    }
}

fn first_line(input: &str) -> &str {
    input.lines().next().unwrap_or(input)
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
