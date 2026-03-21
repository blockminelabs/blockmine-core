use anyhow::Result;

use crate::config::CliConfig;
use crate::engine::BackendMode;
use crate::miner_loop::{build_engines, format_rate, run_benchmarks, EngineSelectionConfig};

pub fn run(
    _config: &CliConfig,
    backend: BackendMode,
    seconds: u64,
    cpu_threads: usize,
    gpu_platform: usize,
    gpu_device: usize,
    gpu_local_work_size: Option<usize>,
) -> Result<()> {
    let engines = build_engines(EngineSelectionConfig {
        mode: backend,
        cpu_threads,
        cpu_core_ids: None,
        gpu_devices: Vec::new(),
        gpu_platform,
        gpu_device,
        gpu_local_work_size,
    });
    let reports = run_benchmarks(&engines, seconds)?;
    let mut aggregate_hashes = 0u64;
    let mut aggregate_elapsed = 0.0f64;

    for report in &reports {
        println!("backend={}", report.backend);
        println!("duration_s={}", report.elapsed.as_secs_f64());
        println!("hashes={}", report.hashes);
        println!("hashrate={}", format_rate(report.hashes, report.elapsed));
        println!();
        aggregate_hashes = aggregate_hashes.saturating_add(report.hashes);
        aggregate_elapsed = aggregate_elapsed.max(report.elapsed.as_secs_f64());
    }

    if reports.len() > 1 {
        println!("backend=aggregate");
        println!("duration_s={aggregate_elapsed}");
        println!("hashes={aggregate_hashes}");
        println!(
            "hashrate={}",
            format_rate(
                aggregate_hashes,
                std::time::Duration::from_secs_f64(aggregate_elapsed.max(0.000_001))
            )
        );
    }

    Ok(())
}
