use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc,
    Arc,
};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;
use rand::Rng;

use crate::engine::{BackendKind, BenchmarkReport, FoundSolution, MiningEngine, SearchInput};
use crate::hashing::{build_solution_payload, compute_solution_hash_from_payload, hash_meets_target};

#[derive(Debug, Clone)]
pub struct CpuMiner {
    threads: usize,
    selected_cores: Option<Vec<usize>>,
}

impl Default for CpuMiner {
    fn default() -> Self {
        Self::new(0)
    }
}

impl CpuMiner {
    pub fn new(threads: usize) -> Self {
        Self::with_affinity(threads, None)
    }

    pub fn with_affinity(threads: usize, selected_cores: Option<Vec<usize>>) -> Self {
        let available_cores = logical_core_count();
        let normalized_cores = normalize_selected_cores(selected_cores, available_cores);
        let resolved_threads = if threads == 0 {
            normalized_cores
                .as_ref()
                .map(|cores| cores.len())
                .unwrap_or(available_cores)
        } else {
            normalized_cores
                .as_ref()
                .map(|cores| threads.min(cores.len()))
                .unwrap_or(threads)
        };

        Self {
            threads: resolved_threads.max(1),
            selected_cores: normalized_cores,
        }
    }

    pub fn random_nonce() -> u64 {
        rand::thread_rng().gen()
    }
}

impl MiningEngine for CpuMiner {
    fn kind(&self) -> BackendKind {
        BackendKind::Cpu
    }

    fn search_batch(&self, input: &SearchInput) -> Result<Option<FoundSolution>> {
        let started = Instant::now();
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        let mut workers = Vec::with_capacity(self.threads);

        for worker_index in 0..self.threads {
            let tx = tx.clone();
            let stop = Arc::clone(&stop);
            let challenge = input.challenge;
            let miner = input.miner;
            let target = input.target;
            let thread_count = self.threads as u64;
            let base_nonce = input.start_nonce.wrapping_add(worker_index as u64);
            let assigned_core = self
                .selected_cores
                .as_ref()
                .and_then(|cores| cores.get(worker_index % cores.len()))
                .copied();
            let worker_attempts = if worker_index as u64 >= input.max_attempts {
                0
            } else {
                (input.max_attempts - worker_index as u64 + thread_count - 1) / thread_count
            };

            workers.push(thread::spawn(move || {
                pin_thread_to_logical_core(assigned_core);
                let mut attempts = 0u64;
                let mut nonce = base_nonce;
                let deadline_attempts = worker_attempts;
                let mut payload = build_solution_payload(&challenge, &miner);

                while attempts < deadline_attempts && !stop.load(Ordering::Relaxed) {
                    let hash = compute_solution_hash_from_payload(&mut payload, nonce);
                    attempts = attempts.saturating_add(1);

                    if hash_meets_target(&hash, &target) {
                        let _ = tx.send(Some((nonce, hash, attempts)));
                        stop.store(true, Ordering::Relaxed);
                        return;
                    }

                    nonce = nonce.wrapping_add(thread_count);
                }

                let _ = tx.send(None);
            }));
        }

        drop(tx);

        let mut pending = self.threads;
        let mut solution = None;

        while pending > 0 {
            match rx.recv() {
                Ok(Some((nonce, hash, attempts))) => {
                    solution = Some(FoundSolution {
                        backend: BackendKind::Cpu,
                        nonce,
                        hash,
                        attempts,
                        elapsed: started.elapsed(),
                    });
                    break;
                }
                Ok(None) => pending -= 1,
                Err(_) => break,
            }
        }

        stop.store(true, Ordering::Relaxed);
        for worker in workers {
            let _ = worker.join();
        }

        Ok(solution)
    }

    fn benchmark(&self, seconds: u64) -> Result<BenchmarkReport> {
        let started = Instant::now();
        let deadline = started + Duration::from_secs(seconds);
        let challenge = [7u8; 32];
        let target = [0xffu8; 32];
        let miner = solana_sdk::pubkey::Pubkey::new_unique();
        let (tx, rx) = mpsc::channel();
        let mut workers = Vec::with_capacity(self.threads);
        let thread_count = self.threads as u64;

        for worker_index in 0..self.threads {
            let tx = tx.clone();
            let challenge = challenge;
            let target = target;
            let miner = miner;
            let deadline = deadline;
            let assigned_core = self
                .selected_cores
                .as_ref()
                .and_then(|cores| cores.get(worker_index % cores.len()))
                .copied();
            workers.push(thread::spawn(move || {
                pin_thread_to_logical_core(assigned_core);
                let mut hashes = 0u64;
                let mut nonce = worker_index as u64;
                let mut payload = build_solution_payload(&challenge, &miner);
                while Instant::now() < deadline {
                    let hash = compute_solution_hash_from_payload(&mut payload, nonce);
                    let _ = hash_meets_target(&hash, &target);
                    hashes = hashes.saturating_add(1);
                    nonce = nonce.wrapping_add(thread_count);
                }
                let _ = tx.send(hashes);
            }));
        }
        drop(tx);

        let mut hashes = 0u64;
        for count in rx {
            hashes = hashes.saturating_add(count);
        }
        for worker in workers {
            let _ = worker.join();
        }

        Ok(BenchmarkReport {
            backend: BackendKind::Cpu,
            hashes,
            elapsed: started.elapsed(),
        })
    }
}

fn logical_core_count() -> usize {
    thread::available_parallelism()
        .map(|parallelism| parallelism.get())
        .unwrap_or(1)
}

fn normalize_selected_cores(selected_cores: Option<Vec<usize>>, available_cores: usize) -> Option<Vec<usize>> {
    let mut cores = selected_cores?;
    cores.retain(|core| *core < available_cores);
    cores.sort_unstable();
    cores.dedup();
    (!cores.is_empty()).then_some(cores)
}

#[cfg(target_os = "windows")]
fn pin_thread_to_logical_core(core_index: Option<usize>) {
    use windows_sys::Win32::System::Threading::{GetCurrentThread, SetThreadAffinityMask};

    let Some(core_index) = core_index else {
        return;
    };

    if core_index >= usize::BITS as usize {
        return;
    }

    unsafe {
        let _ = SetThreadAffinityMask(GetCurrentThread(), 1usize << core_index);
    }
}

#[cfg(not(target_os = "windows"))]
fn pin_thread_to_logical_core(_core_index: Option<usize>) {}
