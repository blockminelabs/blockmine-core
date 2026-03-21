pub const CONFIG_SEED: &[u8] = b"config";
pub const CURRENT_BLOCK_SEED: &[u8] = b"current_block";
pub const VAULT_AUTHORITY_SEED: &[u8] = b"vault_authority";
pub const MINER_STATS_SEED: &[u8] = b"miner_stats";
pub const BLOCK_HISTORY_SEED: &[u8] = b"block_history_v2";
pub const MINING_SESSION_SEED: &[u8] = b"mining_session";

pub const BLOCK_STATUS_OPEN: u8 = 1;
pub const BLOCK_STATUS_CLOSED: u8 = 2;

pub const MAX_NICKNAME_LEN: usize = 32;

pub const FIXED_SUBMIT_FEE_LAMPORTS: u64 = 10_000_000;
