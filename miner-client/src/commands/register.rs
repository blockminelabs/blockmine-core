use anyhow::Result;

use crate::config::CliConfig;
use crate::rpc::RpcFacade;
use crate::submitter;
use crate::wallet::load_keypair;

pub fn run(config: &CliConfig, nickname: &str) -> Result<()> {
    let signer = load_keypair(config)?;
    let rpc = RpcFacade::new(config);
    let mut nickname_bytes = [0u8; 32];
    let source = nickname.as_bytes();
    let copy_len = source.len().min(32);
    nickname_bytes[..copy_len].copy_from_slice(&source[..copy_len]);
    let signature = submitter::register_miner(&rpc, &signer, nickname_bytes)?;
    println!("signature={signature}");
    Ok(())
}
