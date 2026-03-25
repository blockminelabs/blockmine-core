pub mod cpu;
pub mod gpu;

use std::{fmt, time::Duration};

use anyhow::Result;
use clap::ValueEnum;
use solana_sdk::pubkey::Pubkey;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum BackendMode {
    Cpu,
    Gpu,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    Cpu,
    Gpu,
}

impl fmt::Display for BackendKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cpu => write!(f, "cpu"),
            Self::Gpu => write!(f, "gpu"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SearchInput {
    pub challenge: [u8; 32],
    pub miner: Pubkey,
    pub target: [u8; 32],
    pub start_nonce: u64,
    pub max_attempts: u64,
}

#[derive(Debug, Clone)]
pub struct FoundSolution {
    pub backend: BackendKind,
    pub nonce: u64,
    pub hash: [u8; 32],
    pub attempts: u64,
    pub elapsed: Duration,
}

#[derive(Debug, Clone)]
pub struct BenchmarkReport {
    pub backend: BackendKind,
    pub hashes: u64,
    pub elapsed: Duration,
}

pub trait MiningEngine: Send + Sync + 'static {
    fn kind(&self) -> BackendKind;
    fn search_batch(&self, input: &SearchInput) -> Result<Option<FoundSolution>>;
    fn benchmark(&self, seconds: u64) -> Result<BenchmarkReport>;
}
