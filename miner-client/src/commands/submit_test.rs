use anyhow::Result;

use crate::config::CliConfig;
use crate::rpc::RpcFacade;
use crate::submitter;
use crate::wallet::load_keypair;

pub fn run(config: &CliConfig, nonce: u64) -> Result<()> {
    let signer = load_keypair(config)?;
    let rpc = RpcFacade::new(config);
    let signature = submitter::submit_solution(&rpc, &signer, nonce)?;
    println!("signature={signature}");
    Ok(())
}
