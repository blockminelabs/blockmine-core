use anyhow::Result;
use solana_sdk::signature::Signer;

use crate::config::CliConfig;
use crate::rpc::RpcFacade;
use crate::wallet::load_keypair;

pub fn run(config: &CliConfig) -> Result<()> {
    let signer = load_keypair(config)?;
    let rpc = RpcFacade::new(config);
    let stats = rpc.fetch_miner_stats(&signer.pubkey())?;

    println!("miner={}", stats.miner);
    println!("total_submissions={}", stats.total_submissions);
    println!("valid_blocks_found={}", stats.valid_blocks_found);
    println!("total_rewards_earned={}", stats.total_rewards_earned);
    println!("last_submission_time={}", stats.last_submission_time);
    println!("nickname={}", String::from_utf8_lossy(&stats.nickname).trim_matches(char::from(0)));
    Ok(())
}
