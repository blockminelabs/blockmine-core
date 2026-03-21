use anyhow::Result;
use blockmine_program::math::rewards::{reward_era_for_block, ERA_NAME_LEN};

use crate::config::CliConfig;
use crate::rpc::RpcFacade;

pub fn run(config: &CliConfig) -> Result<()> {
    let rpc = RpcFacade::new(config);
    let protocol = rpc.fetch_protocol_config()?;
    let block = rpc.fetch_current_block()?;

    println!("admin={}", protocol.admin);
    println!("mint={}", protocol.bloc_mint);
    println!("reward_vault={}", protocol.reward_vault);
    println!("treasury_authority={}", protocol.treasury_authority);
    println!("treasury_vault={}", protocol.treasury_vault);
    println!("treasury_fee_bps={}", protocol.treasury_fee_bps);
    println!("submit_fee_lamports={}", protocol.submit_fee_lamports);
    println!("block_ttl_sec={}", protocol.block_ttl_sec);
    println!("current_block_number={}", protocol.current_block_number);
    println!("total_blocks_mined={}", protocol.total_blocks_mined);
    println!("total_rewards_distributed={}", protocol.total_rewards_distributed);
    println!(
        "total_treasury_fees_distributed={}",
        protocol.total_treasury_fees_distributed
    );
    println!("difficulty_bits={}", protocol.difficulty_bits);
    println!("current_reward={}", block.block_reward);
    let era = reward_era_for_block(block.block_number);
    println!("current_era_index={}", era.index);
    println!("current_era_name={}", decode_era_name(era.name));
    println!("challenge=0x{}", hex::encode(block.challenge));
    println!("target=0x{}", hex::encode(block.difficulty_target));
    Ok(())
}

fn decode_era_name(name: [u8; ERA_NAME_LEN]) -> String {
    let end = name.iter().position(|byte| *byte == 0).unwrap_or(name.len());
    String::from_utf8_lossy(&name[..end]).into_owned()
}
