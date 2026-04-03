use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::engine::BackendMode;

const DEFAULT_PROGRAM_ID: &str = "FgRe73gAkZPhxpiCFHMYMfLR4dabDaB1FDVFazVTcCtv";
const DEFAULT_RPC_URL: &str = "auto";

#[derive(Debug, Parser)]
#[command(
    name = "blockmine-miner",
    about = "Open-source CPU/GPU miner for BlockMine"
)]
pub struct Cli {
    #[arg(long, default_value = DEFAULT_RPC_URL)]
    pub rpc: String,
    #[arg(long)]
    pub keypair: Option<PathBuf>,
    #[arg(long, default_value = DEFAULT_PROGRAM_ID)]
    pub program_id: String,
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    InitProtocol {
        #[arg(long)]
        mint: String,
        #[arg(long)]
        treasury_authority: Option<String>,
        #[arg(long, default_value_t = 20_000_000_000_000_000)]
        max_supply: u64,
        #[arg(long, default_value_t = 21_000_000_000)]
        initial_block_reward: u64,
        #[arg(long, default_value_t = 100)]
        treasury_fee_bps: u16,
        #[arg(long, default_value_t = 210_000)]
        halving_interval: u64,
        #[arg(long, default_value_t = 10)]
        target_block_time_sec: u64,
        #[arg(long, default_value_t = 20)]
        adjustment_interval: u64,
        #[arg(long, default_value_t = 18)]
        initial_difficulty_bits: u8,
        #[arg(long, default_value_t = 8)]
        min_difficulty_bits: u8,
        #[arg(long, default_value_t = 28)]
        max_difficulty_bits: u8,
        #[arg(long, default_value_t = 10_000_000)]
        submit_fee_lamports: u64,
        #[arg(long, default_value_t = 60)]
        block_ttl_sec: i64,
        #[arg(long, default_value_t = 9)]
        token_decimals: u8,
    },
    Mine {
        #[arg(long, value_enum, default_value_t = BackendMode::Cpu)]
        backend: BackendMode,
        #[arg(long, default_value_t = 250_000)]
        batch_size: u64,
        #[arg(long)]
        gpu_batch_size: Option<u64>,
        #[arg(long, default_value_t = 0)]
        cpu_threads: usize,
        #[arg(long, default_value_t = 0)]
        gpu_platform: usize,
        #[arg(long, default_value_t = 0)]
        gpu_device: usize,
        #[arg(long)]
        gpu_local_work_size: Option<usize>,
        #[arg(long)]
        start_nonce: Option<u64>,
    },
    Desktop {
        #[arg(long, value_enum, default_value_t = BackendMode::Cpu)]
        backend: BackendMode,
        #[arg(long, default_value_t = 250_000)]
        batch_size: u64,
        #[arg(long)]
        gpu_batch_size: Option<u64>,
        #[arg(long, default_value_t = 0)]
        cpu_threads: usize,
        #[arg(long, default_value_t = 0)]
        gpu_platform: usize,
        #[arg(long, default_value_t = 0)]
        gpu_device: usize,
        #[arg(long)]
        gpu_local_work_size: Option<usize>,
    },
    Benchmark {
        #[arg(long, value_enum, default_value_t = BackendMode::Cpu)]
        backend: BackendMode,
        #[arg(long, default_value_t = 10)]
        seconds: u64,
        #[arg(long, default_value_t = 0)]
        cpu_threads: usize,
        #[arg(long, default_value_t = 0)]
        gpu_platform: usize,
        #[arg(long, default_value_t = 0)]
        gpu_device: usize,
        #[arg(long)]
        gpu_local_work_size: Option<usize>,
    },
    ListDevices,
    ProtocolState,
    WalletStats,
    SubmitTest {
        #[arg(long)]
        nonce: u64,
    },
    Register {
        #[arg(long, default_value = "")]
        nickname: String,
    },
}
