use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use anyhow::Result;

use crate::engine::cpu::CpuMiner;
use crate::engine::gpu::GpuMiner;
use crate::engine::{BackendKind, BackendMode, BenchmarkReport, FoundSolution, MiningEngine, SearchInput};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GpuDeviceSelection {
    pub platform_index: usize,
    pub device_index: usize,
}

#[derive(Debug, Clone)]
pub struct EngineSelectionConfig {
    pub mode: BackendMode,
    pub cpu_threads: usize,
    pub cpu_core_ids: Option<Vec<usize>>,
    pub gpu_devices: Vec<GpuDeviceSelection>,
    pub gpu_platform: usize,
    pub gpu_device: usize,
    pub gpu_local_work_size: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct BatchReport {
    pub backend: BackendKind,
    pub attempts: u64,
    pub elapsed: Duration,
    pub found: bool,
}

#[derive(Debug, Clone)]
pub struct RoundOutcome {
    pub solution: Option<FoundSolution>,
    pub reports: Vec<BatchReport>,
    pub next_nonce: u64,
}

pub fn build_engines(selection: EngineSelectionConfig) -> Vec<Box<dyn MiningEngine>> {
    let mut engines: Vec<Box<dyn MiningEngine>> = Vec::new();

    if matches!(selection.mode, BackendMode::Cpu | BackendMode::Both) {
        engines.push(Box::new(CpuMiner::with_affinity(
            selection.cpu_threads,
            selection.cpu_core_ids.clone(),
        )));
    }

    if matches!(selection.mode, BackendMode::Gpu | BackendMode::Both) {
        if selection.gpu_devices.is_empty() {
            engines.push(Box::new(GpuMiner::new(
                selection.gpu_platform,
                selection.gpu_device,
                selection.gpu_local_work_size,
            )));
        } else {
            for device in selection.gpu_devices {
                engines.push(Box::new(GpuMiner::new(
                    device.platform_index,
                    device.device_index,
                    selection.gpu_local_work_size,
                )));
            }
        }
    }

    engines
}

pub fn run_search_round(
    engines: &[Box<dyn MiningEngine>],
    mode: BackendMode,
    batch_size: u64,
    gpu_batch_size: u64,
    input: &SearchInput,
) -> Result<RoundOutcome> {
    let jobs = build_jobs(engines, mode, batch_size, gpu_batch_size, input);
    let next_nonce = input
        .start_nonce
        .wrapping_add(jobs.iter().map(|job| job.input.max_attempts).sum::<u64>());

    let (tx, rx) = mpsc::channel();

    thread::scope(|scope| {
        for job in jobs {
            let tx = tx.clone();
            scope.spawn(move || {
                let started = std::time::Instant::now();
                let result = job.engine.search_batch(&job.input);
                let elapsed = started.elapsed();
                let _ = tx.send((job.backend, job.input.max_attempts, elapsed, result));
            });
        }
        drop(tx);

        let mut reports = Vec::new();
        let mut solution: Option<FoundSolution> = None;

        for (backend, attempts, elapsed, result) in rx {
            let maybe_solution = result?;
            reports.push(BatchReport {
                backend,
                attempts,
                elapsed,
                found: maybe_solution.is_some(),
            });

            if let Some(candidate) = maybe_solution {
                match &solution {
                    Some(current) if current.elapsed <= candidate.elapsed => {}
                    _ => solution = Some(candidate),
                }
            }
        }

        Ok::<RoundOutcome, anyhow::Error>(RoundOutcome {
            solution,
            reports,
            next_nonce,
        })
    })
}

pub fn run_benchmarks(
    engines: &[Box<dyn MiningEngine>],
    seconds: u64,
) -> Result<Vec<BenchmarkReport>> {
    let (tx, rx) = mpsc::channel();

    thread::scope(|scope| {
        for engine in engines {
            let tx = tx.clone();
            scope.spawn(move || {
                let result = engine.benchmark(seconds);
                let _ = tx.send(result);
            });
        }
        drop(tx);

        let mut reports = Vec::new();
        for result in rx {
            reports.push(result?);
        }
        Ok::<Vec<BenchmarkReport>, anyhow::Error>(reports)
    })
}

pub fn format_rate(hashes: u64, elapsed: Duration) -> String {
    let rate = hashes as f64 / elapsed.as_secs_f64().max(0.000_001);
    format_hashrate(rate)
}

fn format_hashrate(rate_hps: f64) -> String {
    if !rate_hps.is_finite() || rate_hps <= 0.0 {
        return "0 H/s".to_string();
    }

    let units = [
        ("TH/s", 1_000_000_000_000.0_f64),
        ("GH/s", 1_000_000_000.0_f64),
        ("MH/s", 1_000_000.0_f64),
        ("kH/s", 1_000.0_f64),
    ];

    for (label, scale) in units {
        if rate_hps >= scale {
            return format!("{:.2} {}", rate_hps / scale, label);
        }
    }

    format!("{:.0} H/s", rate_hps)
}

struct SearchJob<'a> {
    backend: BackendKind,
    engine: &'a dyn MiningEngine,
    input: SearchInput,
}

fn build_jobs<'a>(
    engines: &'a [Box<dyn MiningEngine>],
    mode: BackendMode,
    batch_size: u64,
    gpu_batch_size: u64,
    input: &SearchInput,
) -> Vec<SearchJob<'a>> {
    let mut jobs = Vec::new();
    let mut next_nonce = input.start_nonce;

    for engine in engines {
        let attempts = match engine.kind() {
            BackendKind::Cpu => batch_size,
            // GPU search should always use the GPU-tuned batch size.
            // Falling back to the generic CPU batch size makes the card run
            // tiny batches, which tanks effective off-chain throughput.
            BackendKind::Gpu => gpu_batch_size,
        };

        jobs.push(SearchJob {
            backend: engine.kind(),
            engine: engine.as_ref(),
            input: SearchInput {
                challenge: input.challenge,
                miner: input.miner,
                target: input.target,
                start_nonce: next_nonce,
                max_attempts: attempts,
            },
        });
        next_nonce = next_nonce.wrapping_add(attempts);
    }

    jobs
}
