use anyhow::Result;
use clap::Parser;
use tracing_subscriber::EnvFilter;

use blockmine_miner::cli::{Cli, Commands};
use blockmine_miner::commands;
use blockmine_miner::config::CliConfig;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .init();

    let cli = Cli::parse();
    let config = CliConfig::from_cli(&cli)?;

    match cli.command {
        Commands::InitProtocol {
            mint,
            treasury_authority,
            max_supply,
            initial_block_reward,
            treasury_fee_bps,
            halving_interval,
            target_block_time_sec,
            adjustment_interval,
            initial_difficulty_bits,
            min_difficulty_bits,
            max_difficulty_bits,
            submit_fee_lamports,
            block_ttl_sec,
            token_decimals,
        } => commands::init_protocol::run(
            &config,
            commands::init_protocol::InitProtocolCommand {
                mint,
                treasury_authority,
                max_supply,
                initial_block_reward,
                treasury_fee_bps,
                halving_interval,
                target_block_time_sec,
                adjustment_interval,
                initial_difficulty_bits,
                min_difficulty_bits,
                max_difficulty_bits,
                submit_fee_lamports,
                block_ttl_sec,
                token_decimals,
            },
        ),
        Commands::Mine {
            backend,
            batch_size,
            gpu_batch_size,
            cpu_threads,
            gpu_platform,
            gpu_device,
            gpu_local_work_size,
            start_nonce,
        } => commands::mine::run(
            &config,
            backend,
            batch_size,
            gpu_batch_size,
            cpu_threads,
            gpu_platform,
            gpu_device,
            gpu_local_work_size,
            start_nonce,
        ),
        Commands::Desktop {
            backend,
            batch_size,
            gpu_batch_size,
            cpu_threads,
            gpu_platform,
            gpu_device,
            gpu_local_work_size,
        } => commands::desktop::run(
            &config,
            backend,
            batch_size,
            gpu_batch_size,
            cpu_threads,
            gpu_platform,
            gpu_device,
            gpu_local_work_size,
        ),
        Commands::Benchmark {
            backend,
            seconds,
            cpu_threads,
            gpu_platform,
            gpu_device,
            gpu_local_work_size,
        } => commands::benchmark::run(
            &config,
            backend,
            seconds,
            cpu_threads,
            gpu_platform,
            gpu_device,
            gpu_local_work_size,
        ),
        Commands::ListDevices => commands::list_devices::run(),
        Commands::ProtocolState => commands::protocol_state::run(&config),
        Commands::WalletStats => commands::wallet_stats::run(&config),
        Commands::SubmitTest { nonce } => commands::submit_test::run(&config, nonce),
        Commands::Register { nickname } => commands::register::run(&config, &nickname),
    }
}
