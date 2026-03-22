#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::{BTreeSet, VecDeque};
use std::fs;
use std::io::{Cursor, Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use blockmine_program::math::rewards::{reward_era_for_block, ERA_NAME_LEN};
use blockmine_miner::config::CliConfig;
use blockmine_miner::engine::gpu::{list_devices as list_gpu_devices, GpuDeviceInfo, GpuMiner};
use blockmine_miner::engine::BackendMode;
use blockmine_miner::miner_loop::GpuDeviceSelection;
use blockmine_miner::mining_service::{MiningHandle, MiningRuntimeOptions, MiningSnapshot, MiningUpdate};
use blockmine_miner::session_wallet::{
    load_session_delegate_balances, sweep_single_session_delegate_wallet, SessionBalanceSummary,
    SessionSweepSummary,
};
use blockmine_miner::ui::format_bloc;
use blockmine_miner::wallet_store::{
    app_storage_dir, create_session_delegate_wallet, load_managed_keypair, load_session_delegate_wallet,
    ManagedWallet, WalletSource,
};
use eframe::egui::{self, Align, Color32, IconData, RichText, TextEdit, TextureHandle, TextureOptions};
use eframe::{App, Frame, NativeOptions};
use image::{AnimationDecoder, ImageReader};
use image::codecs::gif::GifDecoder;
use rand::Rng;
use rfd::FileDialog;
use serde::{Deserialize, Serialize};
use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey};

const DEFAULT_RPC_URL: &str = "https://api.devnet.solana.com";
const DEFAULT_PROGRAM_ID: &str = "HQCgF9XWsJPH3uEfRdRGW1rARwWqDpV361ZpaXUostfw";
const DEFAULT_BROWSER_MINE_URL: &str = "https://blockmine.dev/desktop-bridge";
const DEFAULT_PHANTOM_SESSION_MAX_BLOCKS: u64 = 60;
const MAX_PHANTOM_SESSION_BLOCKS: u64 = 1000;
const TREASURY_FEE_PER_BLOCK_LAMPORTS: u64 = 10_000_000;
const SESSION_NETWORK_BUFFER_BASE_LAMPORTS: u64 = 500_000;
const SESSION_NETWORK_BUFFER_PER_BLOCK_LAMPORTS: u64 = 25_000;
const CHART_POINT_COUNT: usize = 34;
const CHART_WINDOW: Duration = Duration::from_secs(30);
const BACKGROUND_LINK_DISTANCE: f32 = 112.0;
const BACKGROUND_CURSOR_DISTANCE: f32 = 152.0;
const BACKGROUND_PARTICLE_DENSITY: f32 = 0.0000455;
const BACKGROUND_MIN_PARTICLES: usize = 44;
const BACKGROUND_MAX_PARTICLES: usize = 117;
const APP_ICON_PNG: &[u8] = include_bytes!("../../img/logocircle.png");

#[cfg(target_os = "windows")]
const DESKTOP_PLATFORM_LABEL: &str = "Windows Miner";
#[cfg(target_os = "macos")]
const DESKTOP_PLATFORM_LABEL: &str = "MacOS Miner";
#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
const DESKTOP_PLATFORM_LABEL: &str = "Desktop Miner";

#[derive(Clone, Copy)]
struct EraScheduleRow {
    era: u8,
    name: &'static str,
    block_range: &'static str,
    reward_per_block: &'static str,
    era_emissions: &'static str,
    cumulative_emissions: &'static str,
}

const ERA_SCHEDULE_ROWS: [EraScheduleRow; 15] = [
    EraScheduleRow {
        era: 0,
        name: "Genesis",
        block_range: "0 - 9,999",
        reward_per_block: "21.0",
        era_emissions: "210,000",
        cumulative_emissions: "210,000",
    },
    EraScheduleRow {
        era: 1,
        name: "Aurum",
        block_range: "10,000 - 99,999",
        reward_per_block: "12.0",
        era_emissions: "1,080,000",
        cumulative_emissions: "1,290,000",
    },
    EraScheduleRow {
        era: 2,
        name: "Phoenix",
        block_range: "100,000 - 299,999",
        reward_per_block: "7.0",
        era_emissions: "1,400,000",
        cumulative_emissions: "2,690,000",
    },
    EraScheduleRow {
        era: 3,
        name: "Horizon",
        block_range: "300,000 - 599,999",
        reward_per_block: "5.0",
        era_emissions: "1,500,000",
        cumulative_emissions: "4,190,000",
    },
    EraScheduleRow {
        era: 4,
        name: "Quasar",
        block_range: "600,000 - 999,999",
        reward_per_block: "3.8",
        era_emissions: "1,520,000",
        cumulative_emissions: "5,710,000",
    },
    EraScheduleRow {
        era: 5,
        name: "Pulsar",
        block_range: "1,000,000 - 1,499,999",
        reward_per_block: "3.0",
        era_emissions: "1,500,000",
        cumulative_emissions: "7,210,000",
    },
    EraScheduleRow {
        era: 6,
        name: "Voidfall",
        block_range: "1,500,000 - 2,099,999",
        reward_per_block: "2.3",
        era_emissions: "1,380,000",
        cumulative_emissions: "8,590,000",
    },
    EraScheduleRow {
        era: 7,
        name: "Eclipse",
        block_range: "2,100,000 - 2,999,999",
        reward_per_block: "1.8",
        era_emissions: "1,620,000",
        cumulative_emissions: "10,210,000",
    },
    EraScheduleRow {
        era: 8,
        name: "Mythos",
        block_range: "3,000,000 - 4,199,999",
        reward_per_block: "1.4",
        era_emissions: "1,680,000",
        cumulative_emissions: "11,890,000",
    },
    EraScheduleRow {
        era: 9,
        name: "Paragon",
        block_range: "4,200,000 - 5,799,999",
        reward_per_block: "1.1",
        era_emissions: "1,760,000",
        cumulative_emissions: "13,650,000",
    },
    EraScheduleRow {
        era: 10,
        name: "Hyperion",
        block_range: "5,800,000 - 7,499,999",
        reward_per_block: "0.9",
        era_emissions: "1,530,000",
        cumulative_emissions: "15,180,000",
    },
    EraScheduleRow {
        era: 11,
        name: "Singularity",
        block_range: "7,500,000 - 9,499,999",
        reward_per_block: "0.7",
        era_emissions: "1,400,000",
        cumulative_emissions: "16,580,000",
    },
    EraScheduleRow {
        era: 12,
        name: "Eternal I",
        block_range: "9,500,000 - 11,999,999",
        reward_per_block: "0.5",
        era_emissions: "1,250,000",
        cumulative_emissions: "17,830,000",
    },
    EraScheduleRow {
        era: 13,
        name: "Eternal II",
        block_range: "12,000,000 - 15,999,999",
        reward_per_block: "0.3",
        era_emissions: "1,200,000",
        cumulative_emissions: "19,030,000",
    },
    EraScheduleRow {
        era: 14,
        name: "Scarcity",
        block_range: "starts at 16,000,000",
        reward_per_block: "nominally 0.15",
        era_emissions: "remaining 970,000",
        cumulative_emissions: "20,000,000",
    },
];

#[derive(Debug, Clone)]
struct HashrateChartState {
    committed_peaks: VecDeque<f64>,
    current_window_peak: f64,
    last_window_peak: f64,
    last_commit_at: Instant,
}

#[derive(Debug, Clone)]
struct BackgroundParticle {
    position: egui::Pos2,
    velocity: egui::Vec2,
    radius: f32,
}

#[derive(Debug, Clone)]
struct MouseParticleFieldState {
    particles: Vec<BackgroundParticle>,
    last_bounds: egui::Vec2,
}

#[derive(Debug, Clone)]
struct PhantomSessionLink {
    miner_pubkey: Pubkey,
    delegate_wallet: ManagedWallet,
    authorization_signature: String,
}

#[derive(Debug, Clone)]
struct PendingPhantomBridge {
    token: String,
    delegate_wallet: ManagedWallet,
}

#[derive(Debug)]
struct PhantomBridgeCompletion {
    token: String,
    miner_pubkey: Pubkey,
    delegate_pubkey: Pubkey,
    signature: String,
}

#[derive(Debug, Clone)]
struct GpuAutotuneCandidate {
    batch_size: u64,
    local_work_size: Option<usize>,
    hashrate_hps: f64,
}

#[derive(Debug, Clone)]
struct GpuAutotuneOutcome {
    best: GpuAutotuneCandidate,
    tested: Vec<GpuAutotuneCandidate>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MinerControlsMode {
    Fast,
    Advanced,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DepositMethod {
    Web3Wallet,
    ManualSend,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DesktopUiPreferences {
    batch_size: String,
    gpu_batch_size: String,
    cpu_threads: String,
    gpu_local_work_size: String,
    selected_gpu_keys: Vec<String>,
}

#[derive(Clone)]
struct AnimatedFrameTexture {
    texture: TextureHandle,
    duration: Duration,
}

#[derive(Clone)]
struct AnimatedTexture {
    frames: Vec<AnimatedFrameTexture>,
    total_duration: Duration,
}

#[derive(Debug, Clone)]
enum GpuAutotuneMessage {
    Progress(String),
    Finished(Result<GpuAutotuneOutcome, String>),
}

impl HashrateChartState {
    fn new() -> Self {
        Self {
            committed_peaks: VecDeque::with_capacity(CHART_POINT_COUNT),
            current_window_peak: 0.0,
            last_window_peak: 0.0,
            last_commit_at: Instant::now(),
        }
    }

    fn reset(&mut self) {
        self.committed_peaks.clear();
        self.current_window_peak = 0.0;
        self.last_window_peak = 0.0;
        self.last_commit_at = Instant::now();
    }

    fn tick(&mut self, hashrate_hps: f64, is_mining: bool) {
        let sanitized_hashrate = if hashrate_hps.is_finite() && hashrate_hps > 0.0 {
            hashrate_hps
        } else {
            0.0
        };
        if is_mining {
            self.current_window_peak = self.current_window_peak.max(sanitized_hashrate);
        }

        let now = Instant::now();
        while now.duration_since(self.last_commit_at) >= CHART_WINDOW {
            let next_point = if is_mining {
                self.current_window_peak.max(sanitized_hashrate)
            } else {
                0.0
            };
            self.last_window_peak = next_point;
            self.push_peak(next_point);
            self.current_window_peak = 0.0;
            self.last_commit_at += CHART_WINDOW;
        }

        if !is_mining {
            self.current_window_peak = 0.0;
        }
    }

    fn push_peak(&mut self, peak: f64) {
        if self.committed_peaks.len() == CHART_POINT_COUNT {
            self.committed_peaks.pop_front();
        }
        self.committed_peaks.push_back(peak.max(0.0));
    }

    fn real_points(&self) -> Vec<f64> {
        self.committed_peaks
            .iter()
            .copied()
            .filter(|value| *value > 0.0)
            .collect()
    }

    fn display_series(&self) -> Vec<f64> {
        if self.committed_peaks.len() < 2 {
            return vec![0.0; CHART_POINT_COUNT];
        }

        let mut padded = vec![0.0; CHART_POINT_COUNT.saturating_sub(self.committed_peaks.len())];
        padded.extend(self.committed_peaks.iter().copied());
        padded
    }

    fn chart_average(&self, fallback_hashrate: f64) -> f64 {
        let real_points = self.real_points();
        if real_points.is_empty() {
            return self.last_window_peak.max(fallback_hashrate).max(0.0);
        }

        real_points.iter().sum::<f64>() / real_points.len() as f64
    }
}

impl MouseParticleFieldState {
    fn new() -> Self {
        Self {
            particles: Vec::new(),
            last_bounds: egui::Vec2::ZERO,
        }
    }

    fn ensure_particles(&mut self, rect: egui::Rect) {
        let size = rect.size();
        let needs_rebuild = self.particles.is_empty()
            || (size.x - self.last_bounds.x).abs() > 40.0
            || (size.y - self.last_bounds.y).abs() > 40.0;

        if !needs_rebuild {
            return;
        }

        self.last_bounds = size;
        let mut rng = rand::thread_rng();
        let area = (size.x.max(1.0) * size.y.max(1.0)) * BACKGROUND_PARTICLE_DENSITY;
        let count = area
            .round()
            .clamp(BACKGROUND_MIN_PARTICLES as f32, BACKGROUND_MAX_PARTICLES as f32)
            as usize;

        self.particles = (0..count)
            .map(|_| BackgroundParticle {
                position: egui::pos2(
                    rect.left() + rng.gen_range(0.0..size.x.max(1.0)),
                    rect.top() + rng.gen_range(0.0..size.y.max(1.0)),
                ),
                velocity: egui::vec2(rng.gen_range(-12.0..12.0), rng.gen_range(-12.0..12.0)),
                radius: rng.gen_range(1.0..2.0),
            })
            .collect();
    }

    fn tick(&mut self, rect: egui::Rect, pointer: Option<egui::Pos2>, dt: f32) {
        self.ensure_particles(rect);
        let step = dt.clamp(1.0 / 240.0, 1.0 / 20.0);

        for particle in &mut self.particles {
            particle.position += particle.velocity * step;

            if particle.position.x <= rect.left() || particle.position.x >= rect.right() {
                particle.velocity.x *= -1.0;
                particle.position.x = particle.position.x.clamp(rect.left(), rect.right());
            }
            if particle.position.y <= rect.top() || particle.position.y >= rect.bottom() {
                particle.velocity.y *= -1.0;
                particle.position.y = particle.position.y.clamp(rect.top(), rect.bottom());
            }

            if let Some(cursor) = pointer {
                let offset = cursor - particle.position;
                let distance = offset.length();
                if distance < BACKGROUND_CURSOR_DISTANCE && distance > 0.001 {
                    let pull = (1.0 - distance / BACKGROUND_CURSOR_DISTANCE) * 22.0 * step;
                    particle.position += offset.normalized() * pull;
                }
            }
        }
    }
}

fn main() -> eframe::Result<()> {
    let mut native_options = NativeOptions::default();
    native_options.viewport = egui::ViewportBuilder::default()
        .with_inner_size([1320.0, 860.0])
        .with_min_inner_size([980.0, 700.0])
        .with_title("BlockMine Studio")
        .with_icon(load_app_icon());

    eframe::run_native(
        "BlockMine Studio",
        native_options,
        Box::new(|cc| Box::new(BlockMineStudioApp::new(cc))),
    )
}

fn load_app_icon() -> IconData {
    let image = image::load_from_memory(APP_ICON_PNG)
        .expect("failed to decode app icon PNG")
        .into_rgba8();
    let (width, height) = image.dimensions();

    IconData {
        rgba: image.into_raw(),
        width,
        height,
    }
}

struct BlockMineStudioApp {
    rpc_url: String,
    program_id: String,
    browser_mine_url: String,
    phantom_session_max_blocks: u64,
    backend: BackendMode,
    batch_size: String,
    gpu_batch_size: String,
    cpu_threads: String,
    gpu_platform: String,
    gpu_device: String,
    gpu_local_work_size: String,
    gpu_devices: Vec<GpuDeviceInfo>,
    selected_gpu_keys: BTreeSet<String>,
    gpu_devices_error: Option<String>,
    available_cpu_cores: Vec<usize>,
    selected_cpu_cores: BTreeSet<usize>,
    active_wallet: Option<ManagedWallet>,
    phantom_session: Option<PhantomSessionLink>,
    pending_phantom_bridge: Option<PendingPhantomBridge>,
    phantom_bridge_receiver: Option<Receiver<Result<PhantomBridgeCompletion, String>>>,
    session_balance_receiver: Option<Receiver<Result<SessionBalanceSummary, String>>>,
    session_sweep_receiver: Option<Receiver<Result<SessionSweepSummary, String>>>,
    gpu_autotune_receiver: Option<Receiver<GpuAutotuneMessage>>,
    gpu_autotune_status: Option<String>,
    gpu_autotune_best: Option<GpuAutotuneCandidate>,
    active_runtime_wallet: Option<ManagedWallet>,
    active_runtime_miner: Option<Pubkey>,
    session_balance_summary: Option<SessionBalanceSummary>,
    last_session_balance_refresh_at: Instant,
    show_deposit_modal: bool,
    show_withdrawal_modal: bool,
    show_era_schedule_modal: bool,
    deposit_method: DepositMethod,
    withdrawal_target_wallet: String,
    withdrawal_sol_amount: String,
    withdrawal_bloc_amount: String,
    miner_controls_mode: MinerControlsMode,
    logo_circle_texture: Option<TextureHandle>,
    logo_wordmark_texture: Option<TextureHandle>,
    money_animation: Option<AnimatedTexture>,
    wallet_animation: Option<AnimatedTexture>,
    block_found_animation_started_at: Option<Instant>,
    wallet_animation_started_at: Instant,
    last_seen_session_blocks_mined: u64,
    latest_snapshot: MiningSnapshot,
    display_hashrate_hps: f64,
    mining_handle: Option<MiningHandle>,
    hashrate_chart: HashrateChartState,
    mouse_particle_field: MouseParticleFieldState,
    mining_started_at: Option<Instant>,
    status: String,
    error: Option<String>,
}

impl BlockMineStudioApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        apply_brand_visuals(&cc.egui_ctx);
        let desktop_wallet = create_session_delegate_wallet(Some("desktop-session")).ok();
        let preferences = load_desktop_ui_preferences().ok().flatten();
        let selected_gpu_keys = preferences
            .as_ref()
            .map(|prefs| prefs.selected_gpu_keys.iter().cloned().collect())
            .unwrap_or_default();

        let mut app = Self {
            rpc_url: DEFAULT_RPC_URL.to_string(),
            program_id: DEFAULT_PROGRAM_ID.to_string(),
            browser_mine_url: DEFAULT_BROWSER_MINE_URL.to_string(),
            phantom_session_max_blocks: DEFAULT_PHANTOM_SESSION_MAX_BLOCKS,
            backend: BackendMode::Cpu,
            batch_size: preferences
                .as_ref()
                .map(|prefs| prefs.batch_size.clone())
                .unwrap_or_else(|| "250000".to_string()),
            gpu_batch_size: preferences
                .as_ref()
                .map(|prefs| prefs.gpu_batch_size.clone())
                .unwrap_or_else(|| "1048576".to_string()),
            cpu_threads: preferences
                .as_ref()
                .map(|prefs| prefs.cpu_threads.clone())
                .unwrap_or_else(|| "0".to_string()),
            gpu_platform: "0".to_string(),
            gpu_device: "0".to_string(),
            gpu_local_work_size: preferences
                .as_ref()
                .map(|prefs| prefs.gpu_local_work_size.clone())
                .unwrap_or_default(),
            gpu_devices: Vec::new(),
            selected_gpu_keys,
            gpu_devices_error: None,
            available_cpu_cores: detect_available_cpu_cores(),
            selected_cpu_cores: BTreeSet::new(),
            active_wallet: desktop_wallet,
            phantom_session: None,
            pending_phantom_bridge: None,
            phantom_bridge_receiver: None,
            session_balance_receiver: None,
            session_sweep_receiver: None,
            gpu_autotune_receiver: None,
            gpu_autotune_status: None,
            gpu_autotune_best: None,
            active_runtime_wallet: None,
            active_runtime_miner: None,
            session_balance_summary: None,
            last_session_balance_refresh_at: Instant::now(),
            show_deposit_modal: false,
            show_withdrawal_modal: false,
            show_era_schedule_modal: false,
            deposit_method: DepositMethod::Web3Wallet,
            withdrawal_target_wallet: String::new(),
            withdrawal_sol_amount: String::new(),
            withdrawal_bloc_amount: String::new(),
            miner_controls_mode: MinerControlsMode::Fast,
            logo_circle_texture: load_embedded_texture(
                &cc.egui_ctx,
                "brand-circle",
                include_bytes!("../../img/logocircle.png"),
            )
            .ok(),
            logo_wordmark_texture: load_embedded_texture(
                &cc.egui_ctx,
                "brand-wordmark",
                include_bytes!("../../img/blockmine-logo-final.png"),
            )
            .ok(),
            money_animation: load_embedded_gif(
                &cc.egui_ctx,
                "money-win",
                include_bytes!("../../img/Money.gif"),
            )
            .ok(),
            wallet_animation: load_embedded_gif(
                &cc.egui_ctx,
                "wallet-connect",
                include_bytes!("../../img/Wallet animation.gif"),
            )
            .ok(),
            block_found_animation_started_at: None,
            wallet_animation_started_at: Instant::now(),
            last_seen_session_blocks_mined: 0,
            latest_snapshot: MiningSnapshot::default(),
            display_hashrate_hps: 0.0,
            mining_handle: None,
            hashrate_chart: HashrateChartState::new(),
            mouse_particle_field: MouseParticleFieldState::new(),
            mining_started_at: None,
            status: "Desktop mining wallet ready. Deposit SOL to cover treasury fees, then start mining.".to_string(),
            error: None,
        };
        app.refresh_gpu_devices();
        app.start_session_balance_refresh();
        app
    }

    fn open_deposit_modal(&mut self) {
        if self.active_wallet.is_none() {
            match create_session_delegate_wallet(Some("desktop-session")) {
                Ok(wallet) => self.active_wallet = Some(wallet),
                Err(error) => {
                    self.error = Some(format!("Failed to prepare the desktop mining wallet: {error}"));
                    return;
                }
            }
        }
        self.show_deposit_modal = true;
        self.error = None;
    }

    fn open_web3_deposit_flow(&mut self) {
        let Some(wallet) = &self.active_wallet else {
            self.error = Some("The desktop mining wallet is not ready yet.".to_string());
            return;
        };

        let deposit_lamports = desktop_session_fee_budget_lamports(self.phantom_session_max_blocks);
        let desktop_bridge_url = format!(
            "{}?desktop_deposit=1&desktop_wallet={}&desktop_deposit_lamports={}&desktop_max_submissions={}",
            self.browser_mine_url.trim_end_matches('/'),
            wallet.pubkey,
            deposit_lamports,
            self.phantom_session_max_blocks
        );

        match open_in_default_browser(&desktop_bridge_url) {
            Ok(()) => {
                self.show_deposit_modal = false;
                self.status = format!(
                    "Web3 deposit flow opened. Approve about {} SOL in the browser wallet to fund the desktop miner.",
                    format_sol_compact(deposit_lamports)
                );
                self.error = None;
            }
            Err(error) => {
                self.error = Some(format!("Failed to open the Web3 deposit flow: {error}"));
            }
        }
    }

    fn start_session_wallet_sweep(
        &mut self,
        wallet: ManagedWallet,
        recipient: Pubkey,
        requested_sol_lamports: u64,
        requested_bloc_raw: u64,
        status: &str,
    ) {
        if self.session_sweep_receiver.is_some() {
            self.error = Some("A wallet withdrawal is already running.".to_string());
            return;
        }

        let rpc_url = self.rpc_url.trim().to_string();
        let program_id = match self.program_id.trim().parse::<Pubkey>() {
            Ok(program_id) => program_id,
            Err(error) => {
                self.error = Some(format!("Invalid program id: {error}"));
                return;
            }
        };
        let (sender, receiver) = mpsc::channel();
        self.session_sweep_receiver = Some(receiver);
        self.status = status.to_string();
        self.error = None;

        thread::spawn(move || {
            let result = sweep_single_session_delegate_wallet(
                &rpc_url,
                program_id,
                &wallet,
                recipient,
                requested_sol_lamports,
                requested_bloc_raw,
            )
            .map_err(|error| error.to_string());
            let _ = sender.send(result);
        });
    }

    fn start_session_balance_refresh(&mut self) {
        if self.session_balance_receiver.is_some() {
            return;
        }

        let rpc_url = self.rpc_url.trim().to_string();
        let program_id = match self.program_id.trim().parse::<Pubkey>() {
            Ok(program_id) => program_id,
            Err(error) => {
                self.error = Some(format!("Invalid program id: {error}"));
                return;
            }
        };
        let (sender, receiver) = mpsc::channel();
        self.session_balance_receiver = Some(receiver);
        self.last_session_balance_refresh_at = Instant::now();

        thread::spawn(move || {
            let result = load_session_delegate_balances(&rpc_url, program_id)
                .map_err(|error| error.to_string());
            let _ = sender.send(result);
        });
    }

    fn persist_ui_preferences(&self) {
        if let Err(error) = save_desktop_ui_preferences(&DesktopUiPreferences {
            batch_size: self.batch_size.clone(),
            gpu_batch_size: self.gpu_batch_size.clone(),
            cpu_threads: self.cpu_threads.clone(),
            gpu_local_work_size: self.gpu_local_work_size.clone(),
            selected_gpu_keys: self.selected_gpu_keys.iter().cloned().collect(),
        }) {
            eprintln!("failed to save desktop UI preferences: {error}");
        }
    }

    fn clear_pending_phantom_bridge(&mut self, status: &str) {
        self.pending_phantom_bridge = None;
        self.phantom_bridge_receiver = None;
        self.error = None;
        self.status = status.to_string();
    }

    fn refresh_gpu_devices(&mut self) {
        match list_gpu_devices() {
            Ok(devices) => {
                self.gpu_devices = devices;
                self.gpu_devices_error = None;
                self.sync_selected_gpu_selection();
            }
            Err(error) => {
                self.gpu_devices.clear();
                self.selected_gpu_keys.clear();
                self.gpu_devices_error = Some(error.to_string());
            }
        }
    }

    fn sync_selected_gpu_selection(&mut self) {
        if self.gpu_devices.is_empty() {
            self.selected_gpu_keys.clear();
            return;
        }

        let valid_keys: BTreeSet<String> = self
            .gpu_devices
            .iter()
            .map(device_selection_key)
            .collect();
        self.selected_gpu_keys.retain(|key| valid_keys.contains(key));
        if self.selected_gpu_keys.is_empty() {
            self.selected_gpu_keys = valid_keys;
        }

        if let Some((platform_index, device_index)) = self
            .selected_gpu_devices()
            .first()
            .map(|device| (device.platform_index, device.device_index))
        {
            self.gpu_platform = platform_index.to_string();
            self.gpu_device = device_index.to_string();
        }
    }

    fn selected_gpu_devices(&self) -> Vec<&GpuDeviceInfo> {
        self.gpu_devices
            .iter()
            .filter(|device| self.selected_gpu_keys.contains(&device_selection_key(device)))
            .collect()
    }

    fn start_mining(&mut self) {
        if self.mining_handle.is_some() {
            self.error = Some("Mining is already running.".to_string());
            return;
        }

        let Some(wallet) = self.active_wallet.clone() else {
            self.error = Some("The desktop mining wallet is not ready yet.".to_string());
            return;
        };

        let result = self.start_native_mining(wallet, None);

        if let Err(error) = result {
            self.error = Some(error.to_string());
        }
    }

    fn start_cpu_mining(&mut self) {
        self.backend = BackendMode::Cpu;
        self.start_mining();
    }

    fn start_gpu_mining(&mut self) {
        self.backend = BackendMode::Gpu;
        self.start_mining();
    }

    fn start_phantom_session_mining(&mut self) {
        if self.mining_handle.is_some() {
            self.error = Some("Mining is already running.".to_string());
            return;
        }

        let Some(session) = self.phantom_session.clone() else {
            self.error = Some("Connect a wallet first to arm a desktop mining session.".to_string());
            return;
        };

        let result = self.start_native_mining(session.delegate_wallet, Some(session.miner_pubkey));
        if let Err(error) = result {
            self.error = Some(error.to_string());
        }
    }

    fn start_gpu_autotune(&mut self) {
        if self.gpu_autotune_receiver.is_some() {
            self.error = Some("GPU auto-tune is already running.".to_string());
            return;
        }

        if self.mining_handle.is_some() {
            self.error = Some("Stop mining before starting GPU auto-tune.".to_string());
            return;
        }

        let selected_devices: Vec<GpuDeviceInfo> =
            self.selected_gpu_devices().into_iter().cloned().collect();
        if selected_devices.is_empty() {
            self.error = Some("Select at least one GPU before running auto-tune.".to_string());
            return;
        }

        let candidates = build_gpu_autotune_candidates(&selected_devices);
        if candidates.is_empty() {
            self.error = Some("No valid GPU tuning profiles were generated for the selected GPUs.".to_string());
            return;
        }

        let (sender, receiver) = mpsc::channel();
        self.gpu_autotune_receiver = Some(receiver);
        self.gpu_autotune_status = Some(format!(
            "Auto-tune started for {} GPU(s). Testing {} shared profiles...",
            selected_devices.len(),
            candidates.len()
        ));
        self.error = None;

        thread::spawn(move || {
            let result = run_gpu_autotune(selected_devices, candidates, &sender)
                .map_err(|error| error.to_string());
            let _ = sender.send(GpuAutotuneMessage::Finished(result));
        });
    }

    fn stop_mining(&mut self) {
        if let Some(handle) = &self.mining_handle {
            handle.stop();
            self.status = "Stopping native miner...".to_string();
            self.error = None;
        }
    }

    fn build_cli_config(&self, keypair_path: Option<PathBuf>) -> Result<CliConfig> {
        let program_id = self
            .program_id
            .parse::<Pubkey>()
            .with_context(|| format!("invalid program id: {}", self.program_id))?;

        Ok(CliConfig {
            rpc_url: self.rpc_url.trim().to_string(),
            program_id,
            keypair_path,
            commitment: CommitmentConfig::confirmed(),
        })
    }

    fn start_native_mining(
        &mut self,
        wallet: ManagedWallet,
        miner_override: Option<Pubkey>,
    ) -> Result<()> {
        self.persist_ui_preferences();
        let keypair = load_managed_keypair(&wallet)?;
        let config = self.build_cli_config(Some(wallet.keypair_path.clone()))?;
        let options = self.build_runtime_options(miner_override)?;
        self.latest_snapshot = MiningSnapshot::default();
        self.display_hashrate_hps = 0.0;
        self.hashrate_chart.reset();
        self.mining_handle = Some(MiningHandle::start(config, keypair, options)?);
        self.mining_started_at = Some(Instant::now());
        self.active_runtime_wallet = Some(wallet.clone());
        self.active_runtime_miner = miner_override;
        self.status = match miner_override {
            Some(miner) => format!("Native miner started for connected wallet {} via session delegate.", miner),
            None => format!("Native miner started for wallet {}.", wallet.pubkey),
        };
        self.error = None;
        Ok(())
    }

    fn build_runtime_options(&self, miner_override: Option<Pubkey>) -> Result<MiningRuntimeOptions> {
        let selected_gpu_devices: Vec<GpuDeviceSelection> = self
            .selected_gpu_devices()
            .into_iter()
            .map(|device| GpuDeviceSelection {
                platform_index: device.platform_index,
                device_index: device.device_index,
            })
            .collect();
        if matches!(self.backend, BackendMode::Gpu | BackendMode::Both) && selected_gpu_devices.is_empty() {
            anyhow::bail!("Select at least one GPU to start GPU mining.");
        }
        let fallback_device = self.selected_gpu_devices().into_iter().next();

        Ok(MiningRuntimeOptions {
            backend: self.backend,
            batch_size: parse_u64_field(&self.batch_size, "Batch size")?,
            gpu_batch_size: Some(parse_u64_field(&self.gpu_batch_size, "GPU batch size")?),
            cpu_threads: parse_usize_field(&self.cpu_threads, "CPU threads")?,
            cpu_core_ids: selected_cpu_cores(&self.selected_cpu_cores),
            gpu_devices: selected_gpu_devices,
            gpu_platform: fallback_device.map(|device| device.platform_index).unwrap_or(0),
            gpu_device: fallback_device.map(|device| device.device_index).unwrap_or(0),
            gpu_local_work_size: parse_optional_usize_field(&self.gpu_local_work_size, "GPU local work size")?,
            start_nonce: None,
            miner_override,
            leaderboard_ingest_url: derive_leaderboard_ingest_url(&self.browser_mine_url),
        })
    }

    fn poll_updates(&mut self) {
        let mut keep_handle = true;

        if let Some(handle) = &self.mining_handle {
            while let Ok(update) = handle.try_recv() {
                match update {
                    MiningUpdate::Snapshot(snapshot) => {
                        if snapshot.session_blocks_mined > self.last_seen_session_blocks_mined {
                            self.block_found_animation_started_at = Some(Instant::now());
                        }
                        self.last_seen_session_blocks_mined = snapshot.session_blocks_mined;
                        self.status = snapshot.status.clone();
                        self.error = snapshot.last_error.clone();
                        self.display_hashrate_hps = snapshot.last_hashrate_hps.max(0.0);
                        self.latest_snapshot = snapshot;
                    }
                    MiningUpdate::Stopped { snapshot, error } => {
                        self.last_seen_session_blocks_mined = snapshot.session_blocks_mined;
                        self.display_hashrate_hps = snapshot.last_hashrate_hps.max(0.0);
                        self.latest_snapshot = snapshot;
                        self.status = self.latest_snapshot.status.clone();
                        self.error = error;
                        self.mining_started_at = None;
                        keep_handle = false;
                    }
                }
            }
        }

        if !keep_handle {
            self.mining_handle = None;
            self.active_runtime_wallet = None;
            self.active_runtime_miner = None;
        }

        let bridge_result = self
            .phantom_bridge_receiver
            .as_ref()
            .and_then(|receiver| receiver.try_recv().ok());
        if let Some(result) = bridge_result {
            self.phantom_bridge_receiver = None;
            let pending = self.pending_phantom_bridge.take();
            match (result, pending) {
                (Ok(completion), Some(pending))
                    if completion.token == pending.token
                        && completion.delegate_pubkey.to_string() == pending.delegate_wallet.pubkey =>
                {
                    self.phantom_session = Some(PhantomSessionLink {
                        miner_pubkey: completion.miner_pubkey,
                        delegate_wallet: pending.delegate_wallet,
                        authorization_signature: completion.signature.clone(),
                    });
                    self.status = format!(
                        "Wallet connected: {}. Desktop mining session is armed.",
                        completion.miner_pubkey
                    );
                    self.error = None;
                    self.start_session_balance_refresh();
                }
                (Ok(_), _) => {
                    self.error = Some("Wallet bridge response did not match the pending session.".to_string());
                }
                (Err(error), _) => {
                    self.error = Some(error);
                }
            }
        }

        let sweep_result = self
            .session_sweep_receiver
            .as_ref()
            .and_then(|receiver| receiver.try_recv().ok());
        if let Some(result) = sweep_result {
            self.session_sweep_receiver = None;
            match result {
                Ok(summary) => {
                    self.status = format_session_sweep_summary(&summary);
                    self.error = None;
                    self.start_session_balance_refresh();
                }
                Err(error) => {
                    self.error = Some(error);
                }
            }
        }

        let balance_result = self
            .session_balance_receiver
            .as_ref()
            .and_then(|receiver| receiver.try_recv().ok());
        if let Some(result) = balance_result {
            self.session_balance_receiver = None;
            match result {
                Ok(summary) => {
                    self.session_balance_summary = Some(summary);
                }
                Err(error) => {
                    self.error = Some(error);
                }
            }
        }

        let autotune_messages = self
            .gpu_autotune_receiver
            .as_ref()
            .map(|receiver| receiver.try_iter().collect::<Vec<_>>())
            .unwrap_or_default();

        for message in autotune_messages {
            match message {
                GpuAutotuneMessage::Progress(status) => {
                    self.gpu_autotune_status = Some(status);
                }
                GpuAutotuneMessage::Finished(result) => {
                    self.gpu_autotune_receiver = None;
                    match result {
                        Ok(outcome) => {
                            self.gpu_batch_size = outcome.best.batch_size.to_string();
                            self.gpu_local_work_size = outcome
                                .best
                                .local_work_size
                                .map(|size| size.to_string())
                                .unwrap_or_default();
                            self.gpu_autotune_best = Some(outcome.best.clone());
                            self.gpu_autotune_status = Some(format!(
                                "Best GPU profile applied: batch {} / local {} / {} across {} tests.",
                                format_count_compact(outcome.best.batch_size),
                                format_local_work_size_label(outcome.best.local_work_size),
                                format_hashrate_compact(outcome.best.hashrate_hps),
                                outcome.tested.len()
                            ));
                            self.persist_ui_preferences();
                            self.status = "GPU auto-tune completed. Best settings applied.".to_string();
                            self.error = None;
                        }
                        Err(error) => {
                            self.gpu_autotune_status = Some("GPU auto-tune failed.".to_string());
                            self.error = Some(error);
                        }
                    }
                }
            }
        }
    }

    fn wallet_source_label(wallet: &ManagedWallet) -> &'static str {
        match wallet.source {
            WalletSource::DedicatedGenerated => "Dedicated wallet",
            WalletSource::SessionDelegate => "Phantom session delegate",
            WalletSource::ImportedFile => "Imported keypair",
        }
    }

    fn runtime_label(&self) -> String {
        let Some(started_at) = self.mining_started_at else {
            return "0s".to_string();
        };

        let elapsed = started_at.elapsed();
        let hours = elapsed.as_secs() / 3600;
        let minutes = (elapsed.as_secs() % 3600) / 60;
        let seconds = elapsed.as_secs() % 60;

        if hours > 0 {
            format!("{hours}h {minutes:02}m")
        } else if minutes > 0 {
            format!("{minutes}m {seconds:02}s")
        } else {
            format!("{seconds}s")
        }
    }
}

impl App for BlockMineStudioApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut Frame) {
        self.poll_updates();
        self.hashrate_chart
            .tick(self.display_hashrate_hps, self.mining_handle.is_some());
        let screen_rect = ctx.screen_rect();
        let pointer = ctx.input(|input| input.pointer.hover_pos());
        let stable_dt = ctx.input(|input| input.stable_dt).max(1.0 / 120.0);
        self.mouse_particle_field.tick(screen_rect, pointer, stable_dt);
        if self.session_balance_receiver.is_none()
            && self.last_session_balance_refresh_at.elapsed() >= Duration::from_secs(3)
            && (self.active_wallet.is_some()
                || self.phantom_session.is_some()
                || self.pending_phantom_bridge.is_some()
                || self.mining_handle.is_some()
                || self.session_balance_summary.is_some())
        {
            self.start_session_balance_refresh();
        }
        ctx.request_repaint_after(Duration::from_millis(33));

        egui::TopBottomPanel::top("top_bar")
            .frame(
                egui::Frame::none()
                    .fill(theme_topbar())
                    .inner_margin(egui::Margin::symmetric(18.0, 14.0)),
            )
            .show(ctx, |ui| {
            paint_mouse_particle_field(ui, &self.mouse_particle_field, screen_rect);
            ui.add_space(10.0);
            ui.horizontal_wrapped(|ui| {
                render_brand_header(ui, self.logo_circle_texture.as_ref(), self.logo_wordmark_texture.as_ref());
                if let Some(started_at) = self.block_found_animation_started_at {
                    if started_at.elapsed() <= Duration::from_secs(5) {
                        if let Some(animation) = &self.money_animation {
                            ui.add_space(6.0);
                            render_animated_texture(ui, animation, started_at, 46.0);
                        }
                    }
                }
                ui.add_space(10.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(150.0, 44.0),
                    egui::Layout::bottom_up(Align::Min),
                    |ui| {
                        ui.label(
                            RichText::new(DESKTOP_PLATFORM_LABEL)
                                .size(13.0)
                                .color(theme_accent()),
                        );
                    },
                );
            });
            ui.add_space(8.0);
            ui.label(RichText::new(&self.status).color(theme_text()));
            if let Some(error) = &self.error {
                ui.add_space(6.0);
                ui.colored_label(theme_error(), error);
            }
            ui.add_space(10.0);
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            paint_mouse_particle_field(ui, &self.mouse_particle_field, screen_rect);
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(14.0, 14.0);

                    ui.columns(2, |columns| {
                        columns[0].vertical(|ui| {
                            render_miner_controls_card(ui, self);
                            render_wallet_card(ui, self);
                        });

                        columns[1].vertical(|ui| {
                            render_hashrate_signal_card(ui, self);
                            render_live_telemetry_card(ui, self);
                        });
                    });
                });
        });

        if self.show_deposit_modal {
            let mut open = true;
            egui::Window::new("Deposit")
                .collapsible(false)
                .resizable(false)
                .open(&mut open)
                .show(ctx, |ui| {
                    let wallet_address = self
                        .active_wallet
                        .as_ref()
                        .map(|wallet| wallet.pubkey.clone())
                        .unwrap_or_else(|| "Not ready yet".to_string());
                    ui.label(
                        "Top up the desktop mining wallet with SOL. That balance is used to pay the 0.01 SOL treasury fee whenever a winning block is submitted.",
                    );
                    ui.add_space(10.0);
                    ui.horizontal_wrapped(|ui| {
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("Web3 wallet").color(theme_button_text()),
                                )
                                .fill(theme_accent())
                                .min_size(egui::vec2(180.0, 38.0)),
                            )
                            .clicked()
                        {
                            self.deposit_method = DepositMethod::Web3Wallet;
                            self.open_web3_deposit_flow();
                        }

                        if ui
                            .add(
                                egui::Button::new("Manual send")
                                    .min_size(egui::vec2(180.0, 38.0)),
                            )
                            .clicked()
                        {
                            self.deposit_method = DepositMethod::ManualSend;
                        }
                    });
                    ui.add_space(10.0);
                    egui::Frame::group(ui.style())
                        .fill(theme_card_alt())
                        .stroke(egui::Stroke::new(1.0, theme_border()))
                        .rounding(egui::Rounding::same(14.0))
                        .inner_margin(egui::Margin::same(12.0))
                        .show(ui, |ui| {
                            let hint = match self.deposit_method {
                                DepositMethod::Web3Wallet => "The browser deposit flow opens your wallet extension and builds the transfer for you, just like before.",
                                DepositMethod::ManualSend => "Copy this wallet and send SOL manually from any external address. The balance will update live in the miner.",
                            };
                            ui.label(RichText::new(hint).color(theme_muted()));
                            ui.add_space(8.0);
                            labeled_value(ui, "Desktop wallet", wallet_address.clone());
                            labeled_value(
                                ui,
                                "Suggested deposit",
                                format!(
                                    "{} SOL for up to {} winning blocks",
                                    format_sol_compact(desktop_session_fee_budget_lamports(self.phantom_session_max_blocks)),
                                    self.phantom_session_max_blocks
                                ),
                            );
                            ui.add_space(8.0);
                            if matches!(self.deposit_method, DepositMethod::ManualSend) {
                                if ui
                                    .add(
                                        egui::Button::new(
                                            RichText::new("Copy wallet address")
                                                .color(theme_button_text()),
                                        )
                                        .fill(theme_accent())
                                        .min_size(egui::vec2(190.0, 36.0)),
                                    )
                                    .clicked()
                                {
                                    ui.ctx().copy_text(wallet_address.clone());
                                    self.status = "Desktop wallet address copied. Send SOL to this address, then come back to the miner.".to_string();
                                    self.error = None;
                                }
                            }
                        });
                    ui.add_space(12.0);
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Close").clicked() {
                            self.show_deposit_modal = false;
                        }
                    });
                });
            self.show_deposit_modal = self.show_deposit_modal && open;
        }

        if self.show_withdrawal_modal {
            let mut open = true;
            egui::Window::new("Withdrawal")
                .collapsible(false)
                .resizable(false)
                .open(&mut open)
                .show(ctx, |ui| {
                    let total_sol_lamports = self
                        .session_balance_summary
                        .as_ref()
                        .map(|summary| summary.total_balance_lamports)
                        .unwrap_or(0);
                    let total_bloc_raw = self
                        .session_balance_summary
                        .as_ref()
                        .map(|summary| summary.total_bloc_balance_raw)
                        .unwrap_or(0);
                    let bloc_decimals = self
                        .session_balance_summary
                        .as_ref()
                        .map(|summary| summary.bloc_decimals)
                        .unwrap_or(9);
                    ui.label(
                        "Choose where to send the assets currently sitting in the desktop mining wallet. You can withdraw SOL, BLOC, or both in one go.",
                    );
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.label("Wallet");
                        ui.add(
                            TextEdit::singleline(&mut self.withdrawal_target_wallet)
                                .desired_width(360.0)
                                .hint_text("Paste a Solana wallet address"),
                        );
                    });
                    ui.add_space(10.0);
                    egui::Frame::group(ui.style())
                        .fill(theme_card_alt())
                        .stroke(egui::Stroke::new(1.0, theme_border()))
                        .rounding(egui::Rounding::same(14.0))
                        .inner_margin(egui::Margin::same(12.0))
                        .show(ui, |ui| {
                            labeled_value(
                                ui,
                                "Available SOL",
                                format!("{} SOL", format_sol_compact(total_sol_lamports)),
                            );
                            labeled_value(
                                ui,
                                "Available BLOC",
                                format!(
                                    "{} BLOC",
                                    format_decimal_amount_trimmed(total_bloc_raw, bloc_decimals)
                                ),
                            );
                        });
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        ui.label("SOL");
                        ui.add(
                            TextEdit::singleline(&mut self.withdrawal_sol_amount)
                                .desired_width(180.0)
                                .hint_text("0.00"),
                        );
                        if ui
                            .add_enabled(total_sol_lamports > 0, egui::Button::new("Max"))
                            .clicked()
                        {
                            self.withdrawal_sol_amount =
                                format_decimal_amount_trimmed(total_sol_lamports, 9);
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("BLOC");
                        ui.add(
                            TextEdit::singleline(&mut self.withdrawal_bloc_amount)
                                .desired_width(180.0)
                                .hint_text("0.00"),
                        );
                        if ui
                            .add_enabled(total_bloc_raw > 0, egui::Button::new("Max"))
                            .clicked()
                        {
                            self.withdrawal_bloc_amount =
                                format_decimal_amount_trimmed(total_bloc_raw, bloc_decimals);
                        }
                    });
                    ui.add_space(10.0);
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Cancel").clicked() {
                            self.show_withdrawal_modal = false;
                        }
                        if ui
                            .add_enabled(
                                !self.withdrawal_target_wallet.trim().is_empty()
                                    && (!self.withdrawal_sol_amount.trim().is_empty()
                                        || !self.withdrawal_bloc_amount.trim().is_empty())
                                    && self.session_sweep_receiver.is_none(),
                                egui::Button::new(
                                    RichText::new("Withdraw now")
                                        .color(theme_button_text()),
                                )
                                .fill(theme_accent())
                                .min_size(egui::vec2(140.0, 36.0)),
                            )
                            .clicked()
                        {
                            match (
                                self.withdrawal_target_wallet.trim().parse::<Pubkey>(),
                                parse_decimal_amount(&self.withdrawal_sol_amount, 9, "SOL"),
                                parse_decimal_amount(
                                    &self.withdrawal_bloc_amount,
                                    bloc_decimals,
                                    "BLOC",
                                ),
                            ) {
                                (Ok(recipient), Ok(sol_lamports), Ok(bloc_raw)) => {
                                    let wallet = self
                                        .active_wallet
                                        .clone()
                                        .or_else(|| create_session_delegate_wallet(Some("desktop-session")).ok());
                                    match wallet {
                                        Some(wallet) => {
                                            self.show_withdrawal_modal = false;
                                            self.start_session_wallet_sweep(
                                                wallet,
                                                recipient,
                                                sol_lamports,
                                                bloc_raw,
                                                "Withdrawing assets from the desktop mining wallet...",
                                            );
                                        }
                                        None => {
                                            self.error = Some(
                                                "Failed to load the desktop mining wallet."
                                                    .to_string(),
                                            );
                                        }
                                    }
                                }
                                (Err(error), _, _) => {
                                    self.error = Some(format!(
                                        "Invalid withdrawal wallet address: {error}"
                                    ));
                                }
                                (_, Err(error), _) | (_, _, Err(error)) => {
                                    self.error = Some(error.to_string());
                                }
                            }
                        }
                    });
                });
            self.show_withdrawal_modal = self.show_withdrawal_modal && open;
        }

        if self.show_era_schedule_modal {
            let mut open = true;
            egui::Window::new("Mining Curve")
                .collapsible(false)
                .resizable(true)
                .default_size(egui::vec2(860.0, 720.0))
                .open(&mut open)
                .show(ctx, |ui| {
                    ui.label(
                        RichText::new(
                            "This is the exact mining curve currently embedded in the protocol reset plan.",
                        )
                        .color(theme_muted()),
                    );
                    ui.add_space(10.0);
                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                        egui::Frame::group(ui.style())
                            .fill(theme_card_alt())
                            .stroke(egui::Stroke::new(1.0, theme_border()))
                            .show(ui, |ui| {
                                ui.label(RichText::new("Mining Curve").strong().color(theme_accent()));
                                ui.add_space(10.0);
                                egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
                                    egui::Grid::new("era_schedule_grid")
                                        .striped(true)
                                        .min_col_width(78.0)
                                        .spacing(egui::vec2(16.0, 10.0))
                                        .show(ui, |ui| {
                                            render_schedule_header(ui, "Era");
                                            render_schedule_header(ui, "Name");
                                            render_schedule_header(ui, "Block range");
                                            render_schedule_header(ui, "Reward per block (BLOC)");
                                            render_schedule_header(ui, "Era emissions (BLOC)");
                                            render_schedule_header(ui, "Cumulative emissions (BLOC)");
                                            render_schedule_header(ui, "BLOC mined");
                                            ui.end_row();

                                            for row in ERA_SCHEDULE_ROWS {
                                                let is_current = row.era == reward_era_for_block(self.latest_snapshot.current_block_number).index;
                                                let mined_progress = format_era_progress(row, self.latest_snapshot.current_block_number);
                                                render_schedule_cell(ui, row.era.to_string(), true, is_current);
                                                render_schedule_cell(ui, row.name, false, is_current);
                                                render_schedule_cell(ui, row.block_range, false, is_current);
                                                render_schedule_cell(ui, row.reward_per_block, true, is_current);
                                                render_schedule_cell(ui, row.era_emissions, true, is_current);
                                                render_schedule_cell(ui, row.cumulative_emissions, true, is_current);
                                                render_schedule_cell(ui, mined_progress, false, is_current);
                                                ui.end_row();
                                            }
                                        });
                                });
                            });

                        ui.add_space(12.0);
                        egui::Frame::group(ui.style())
                            .fill(theme_card())
                            .stroke(egui::Stroke::new(1.0, theme_border()))
                            .show(ui, |ui| {
                                ui.label(RichText::new("Scarcity tail").strong().color(theme_accent()));
                                ui.add_space(8.0);
                                ui.label(RichText::new("6,466,666 blocks").strong().color(theme_text()));
                                ui.label(RichText::new("at 0.15 BLOC, then 1 final block at 0.10 BLOC, then 0 forever.").color(theme_muted()));
                            });
                    });
                });
            self.show_era_schedule_modal = self.show_era_schedule_modal && open;
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {}
}

fn desktop_session_fee_budget_lamports(max_blocks: u64) -> u64 {
    let treasury_budget = max_blocks.saturating_mul(TREASURY_FEE_PER_BLOCK_LAMPORTS);
    let network_buffer = SESSION_NETWORK_BUFFER_BASE_LAMPORTS
        .saturating_add(max_blocks.saturating_mul(SESSION_NETWORK_BUFFER_PER_BLOCK_LAMPORTS));

    treasury_budget.saturating_add(network_buffer)
}

fn format_sol_compact(lamports: u64) -> String {
    let sol = lamports as f64 / 1_000_000_000.0;

    if sol >= 1.0 {
        format!("{sol:.2}")
    } else if sol >= 0.1 {
        format!("{sol:.3}")
    } else {
        format!("{sol:.4}")
    }
}

fn format_session_sweep_summary(summary: &SessionSweepSummary) -> String {
    if summary.swept == 0 {
        return "Withdrawal finished. Nothing spendable was available in the desktop mining wallet."
            .to_string();
    }

    match (summary.total_sent_lamports > 0, summary.total_sent_bloc_raw > 0) {
        (true, true) => format!(
            "Withdrawal finished. Sent {} SOL and {} BLOC from the desktop mining wallet.",
            format_sol_compact(summary.total_sent_lamports),
            format_bloc_trimmed(summary.total_sent_bloc_raw),
        ),
        (true, false) => format!(
            "Withdrawal finished. Sent {} SOL from the desktop mining wallet.",
            format_sol_compact(summary.total_sent_lamports),
        ),
        (false, true) => format!(
            "Withdrawal finished. Sent {} BLOC from the desktop mining wallet.",
            format_bloc_trimmed(summary.total_sent_bloc_raw),
        ),
        (false, false) => {
            "Withdrawal finished. Nothing spendable was available in the desktop mining wallet."
                .to_string()
        }
    }
}

fn parse_decimal_amount(input: &str, decimals: u8, label: &str) -> Result<u64> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(0);
    }

    let normalized = trimmed.replace(',', ".");
    let mut parts = normalized.split('.');
    let whole = parts
        .next()
        .unwrap_or("0")
        .parse::<u64>()
        .with_context(|| format!("invalid {label} amount"))?;
    let fractional_part = parts.next().unwrap_or("");
    if parts.next().is_some() {
        anyhow::bail!("invalid {label} amount");
    }
    if fractional_part.len() > decimals as usize {
        anyhow::bail!("{label} supports at most {decimals} decimal places");
    }

    let scale = 10u64.saturating_pow(decimals as u32);
    let mut fractional = fractional_part.to_string();
    while fractional.len() < decimals as usize {
        fractional.push('0');
    }
    let fractional_value = if fractional.is_empty() {
        0
    } else {
        fractional
            .parse::<u64>()
            .with_context(|| format!("invalid {label} amount"))?
    };

    whole
        .checked_mul(scale)
        .and_then(|value| value.checked_add(fractional_value))
        .with_context(|| format!("{label} amount is too large"))
}

fn format_decimal_amount_trimmed(amount: u64, decimals: u8) -> String {
    if decimals == 0 {
        return amount.to_string();
    }

    let scale = 10u64.saturating_pow(decimals as u32);
    let whole = amount / scale;
    let fractional = amount % scale;
    if fractional == 0 {
        return whole.to_string();
    }

    let mut rendered = format!("{whole}.{:0width$}", fractional, width = decimals as usize);
    while rendered.ends_with('0') {
        rendered.pop();
    }
    if rendered.ends_with('.') {
        rendered.pop();
    }
    rendered
}

fn render_schedule_header(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).strong().color(theme_accent()));
}

fn render_schedule_cell(
    ui: &mut egui::Ui,
    value: impl Into<String>,
    monospace: bool,
    highlight: bool,
) {
    let mut text = RichText::new(value.into()).color(theme_text());
    if monospace {
        text = text.family(egui::FontFamily::Monospace);
    }
    if highlight {
        text = text.background_color(Color32::from_rgba_premultiplied(232, 137, 48, 24));
    }
    ui.label(text);
}

fn load_desktop_ui_preferences() -> Result<Option<DesktopUiPreferences>> {
    let path = desktop_ui_preferences_path()?;
    if !path.exists() {
        return Ok(None);
    }

    let raw = fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let preferences = serde_json::from_str::<DesktopUiPreferences>(&raw)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    Ok(Some(preferences))
}

fn save_desktop_ui_preferences(preferences: &DesktopUiPreferences) -> Result<()> {
    let path = desktop_ui_preferences_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let raw = serde_json::to_string_pretty(preferences)
        .context("failed to serialize desktop UI preferences")?;
    fs::write(&path, raw).with_context(|| format!("failed to write {}", path.display()))
}

fn desktop_ui_preferences_path() -> Result<PathBuf> {
    Ok(app_storage_dir()?.join("desktop-ui-preferences.json"))
}

fn format_era_progress(row: EraScheduleRow, current_block_number: u64) -> String {
    let mined_lamports = era_mined_lamports(row.era, current_block_number);
    format!(
        "{} / {} BLOC mined",
        format_bloc_trimmed(mined_lamports),
        row.era_emissions
    )
}

fn era_mined_lamports(era: u8, current_block_number: u64) -> u64 {
    let completed_blocks = current_block_number;
    match era {
        0 => era_linear_progress(completed_blocks, 0, 10_000, 21_000_000_000),
        1 => era_linear_progress(completed_blocks, 10_000, 100_000, 12_000_000_000),
        2 => era_linear_progress(completed_blocks, 100_000, 300_000, 7_000_000_000),
        3 => era_linear_progress(completed_blocks, 300_000, 600_000, 5_000_000_000),
        4 => era_linear_progress(completed_blocks, 600_000, 1_000_000, 3_800_000_000),
        5 => era_linear_progress(completed_blocks, 1_000_000, 1_500_000, 3_000_000_000),
        6 => era_linear_progress(completed_blocks, 1_500_000, 2_100_000, 2_300_000_000),
        7 => era_linear_progress(completed_blocks, 2_100_000, 3_000_000, 1_800_000_000),
        8 => era_linear_progress(completed_blocks, 3_000_000, 4_200_000, 1_400_000_000),
        9 => era_linear_progress(completed_blocks, 4_200_000, 5_800_000, 1_100_000_000),
        10 => era_linear_progress(completed_blocks, 5_800_000, 7_500_000, 900_000_000),
        11 => era_linear_progress(completed_blocks, 7_500_000, 9_500_000, 700_000_000),
        12 => era_linear_progress(completed_blocks, 9_500_000, 12_000_000, 500_000_000),
        13 => era_linear_progress(completed_blocks, 12_000_000, 16_000_000, 300_000_000),
        14 => scarcity_progress(completed_blocks),
        _ => 0,
    }
}

fn era_linear_progress(
    completed_blocks: u64,
    start_block: u64,
    end_block_exclusive: u64,
    reward_lamports: u64,
) -> u64 {
    if completed_blocks <= start_block {
        return 0;
    }
    let completed_in_era = completed_blocks
        .saturating_sub(start_block)
        .min(end_block_exclusive.saturating_sub(start_block));
    completed_in_era.saturating_mul(reward_lamports)
}

fn scarcity_progress(completed_blocks: u64) -> u64 {
    const SCARCITY_START_BLOCK: u64 = 16_000_000;
    const SCARCITY_FULL_REWARD_BLOCKS: u64 = 6_466_666;
    const SCARCITY_FULL_REWARD_LAMPORTS: u64 = 150_000_000;
    const SCARCITY_FINAL_REWARD_LAMPORTS: u64 = 100_000_000;

    if completed_blocks <= SCARCITY_START_BLOCK {
        return 0;
    }

    let completed_in_era = completed_blocks.saturating_sub(SCARCITY_START_BLOCK);
    let full_blocks = completed_in_era.min(SCARCITY_FULL_REWARD_BLOCKS);
    let mut mined = full_blocks.saturating_mul(SCARCITY_FULL_REWARD_LAMPORTS);
    if completed_in_era > SCARCITY_FULL_REWARD_BLOCKS {
        mined = mined.saturating_add(SCARCITY_FINAL_REWARD_LAMPORTS);
    }
    mined.min(970_000_000_000_000)
}

fn format_bloc_trimmed(amount: u64) -> String {
    let raw = format_bloc(amount);
    raw.trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

fn theme_bg() -> Color32 {
    Color32::from_rgb(20, 20, 23)
}

fn theme_topbar() -> Color32 {
    Color32::from_rgba_premultiplied(22, 22, 26, 226)
}

fn theme_card() -> Color32 {
    Color32::from_rgba_premultiplied(28, 28, 34, 198)
}

fn theme_card_alt() -> Color32 {
    Color32::from_rgba_premultiplied(36, 36, 44, 216)
}

fn theme_border() -> Color32 {
    Color32::from_rgba_premultiplied(86, 86, 96, 118)
}

fn theme_text() -> Color32 {
    Color32::from_rgb(233, 233, 236)
}

fn theme_muted() -> Color32 {
    Color32::from_rgb(161, 161, 170)
}

fn theme_subtle() -> Color32 {
    Color32::from_rgb(118, 118, 128)
}

fn theme_accent() -> Color32 {
    Color32::from_rgb(232, 137, 48)
}

fn theme_accent_soft() -> Color32 {
    Color32::from_rgb(181, 110, 42)
}

fn theme_glow_outer() -> Color32 {
    Color32::TRANSPARENT
}

fn theme_glow_inner() -> Color32 {
    Color32::TRANSPARENT
}

fn theme_button_text() -> Color32 {
    Color32::from_rgb(28, 18, 10)
}

fn theme_error() -> Color32 {
    Color32::from_rgb(232, 122, 107)
}

fn theme_success() -> Color32 {
    Color32::from_rgb(160, 197, 150)
}

fn paint_mouse_particle_field(
    ui: &mut egui::Ui,
    field: &MouseParticleFieldState,
    screen_rect: egui::Rect,
) {
    let rect = ui.max_rect().intersect(screen_rect);
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }

    let painter = ui.painter_at(rect);
    let cursor = ui.ctx().input(|input| input.pointer.hover_pos());

    for (index, particle) in field.particles.iter().enumerate() {
        if !rect.contains(particle.position) {
            continue;
        }

        for other in field.particles.iter().skip(index + 1) {
            if !rect.contains(other.position) {
                continue;
            }

            let distance = particle.position.distance(other.position);
            if distance < BACKGROUND_LINK_DISTANCE {
                let alpha = ((1.0 - distance / BACKGROUND_LINK_DISTANCE) * 24.0).round() as u8;
                painter.line_segment(
                    [particle.position, other.position],
                    egui::Stroke::new(
                        1.0,
                        Color32::from_rgba_premultiplied(232, 137, 48, alpha),
                    ),
                );
            }
        }

        if let Some(pointer) = cursor {
            if rect.contains(pointer) {
                let distance = particle.position.distance(pointer);
                if distance < BACKGROUND_CURSOR_DISTANCE {
                    let alpha =
                        ((1.0 - distance / BACKGROUND_CURSOR_DISTANCE) * 52.0).round() as u8;
                    painter.line_segment(
                        [particle.position, pointer],
                        egui::Stroke::new(
                            1.1,
                            Color32::from_rgba_premultiplied(255, 167, 74, alpha),
                        ),
                    );
                }
            }
        }

        painter.circle_filled(
            particle.position,
            particle.radius,
            Color32::from_rgba_premultiplied(255, 168, 70, 160),
        );
        painter.circle_filled(
            particle.position,
            particle.radius + 3.0,
            Color32::from_rgba_premultiplied(255, 150, 46, 18),
        );
    }
}

fn apply_brand_visuals(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.override_text_color = Some(theme_text());
    visuals.panel_fill = theme_bg();
    visuals.window_fill = theme_bg();
    visuals.faint_bg_color = theme_card_alt();
    visuals.extreme_bg_color = theme_card_alt();
    visuals.code_bg_color = theme_card_alt();
    visuals.hyperlink_color = theme_accent();
    visuals.selection.bg_fill = theme_accent_soft();
    visuals.widgets.noninteractive.bg_fill = theme_card_alt();
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, theme_border());
    visuals.widgets.inactive.bg_fill = theme_card_alt();
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, theme_border());
    visuals.widgets.hovered.bg_fill = Color32::from_rgba_premultiplied(44, 44, 52, 228);
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, theme_accent_soft());
    visuals.widgets.active.bg_fill = Color32::from_rgba_premultiplied(54, 54, 64, 236);
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, theme_accent());
    ctx.set_visuals(visuals);
}

fn load_embedded_texture(
    ctx: &egui::Context,
    name: &str,
    bytes: &[u8],
) -> Result<TextureHandle> {
    let image = ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .context("failed to detect embedded image format")?
        .decode()
        .context("failed to decode embedded image")?
        .to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    let pixels = image.into_raw();
    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, &pixels);
    Ok(ctx.load_texture(name.to_string(), color_image, TextureOptions::LINEAR))
}

fn load_embedded_gif(
    ctx: &egui::Context,
    name: &str,
    bytes: &[u8],
) -> Result<AnimatedTexture> {
    let decoder = GifDecoder::new(Cursor::new(bytes)).context("failed to decode embedded gif")?;
    let frames = decoder
        .into_frames()
        .collect_frames()
        .context("failed to read embedded gif frames")?;

    let mut textures = Vec::new();
    let mut total_duration = Duration::ZERO;

    for (index, frame) in frames.into_iter().enumerate() {
        let delay = frame.delay();
        let (numer_ms, denom_ms) = delay.numer_denom_ms();
        let frame_duration_ms = if denom_ms == 0 {
            100
        } else {
            (numer_ms / denom_ms).max(60)
        };
        let duration = Duration::from_millis(frame_duration_ms as u64);
        total_duration += duration;

        let rgba = image::DynamicImage::ImageRgba8(frame.into_buffer()).to_rgba8();
        let size = [rgba.width() as usize, rgba.height() as usize];
        let pixels = rgba.into_raw();
        let color_image = egui::ColorImage::from_rgba_unmultiplied(size, &pixels);
        let texture = ctx.load_texture(
            format!("{name}-{index}"),
            color_image,
            TextureOptions::LINEAR,
        );
        textures.push(AnimatedFrameTexture { texture, duration });
    }

    Ok(AnimatedTexture {
        frames: textures,
        total_duration,
    })
}

fn render_brand_header(
    ui: &mut egui::Ui,
    circle_texture: Option<&TextureHandle>,
    wordmark_texture: Option<&TextureHandle>,
) {
    ui.horizontal(|ui| {
        if let Some(texture) = circle_texture {
            ui.add(
                egui::Image::new(texture)
                    .fit_to_exact_size(egui::vec2(52.0, 52.0))
            );
        }

        if let Some(texture) = wordmark_texture {
            let size = texture.size_vec2();
            let target_height = 44.0;
            let target_width = (size.x / size.y.max(1.0)) * target_height;
            ui.add(
                egui::Image::new(texture)
                    .fit_to_exact_size(egui::vec2(target_width, target_height))
            );
        } else {
            ui.heading(RichText::new("BlockMine").size(30.0).color(theme_text()));
        }
    });
}

fn render_animated_texture(
    ui: &mut egui::Ui,
    animation: &AnimatedTexture,
    started_at: Instant,
    target_height: f32,
) {
    if animation.frames.is_empty() {
        return;
    }

    let frame = current_animation_frame(animation, started_at);
    let size = frame.texture.size_vec2();
    let target_width = (size.x / size.y.max(1.0)) * target_height;
    ui.add(
        egui::Image::new(&frame.texture)
            .fit_to_exact_size(egui::vec2(target_width, target_height)),
    );
}

fn current_animation_frame(
    animation: &AnimatedTexture,
    started_at: Instant,
) -> &AnimatedFrameTexture {
    if animation.frames.len() == 1 || animation.total_duration.is_zero() {
        return &animation.frames[0];
    }

    let total_ms = animation.total_duration.as_millis().max(1);
    let mut cursor_ms = started_at.elapsed().as_millis() % total_ms;
    for frame in &animation.frames {
        let frame_ms = frame.duration.as_millis().max(1);
        if cursor_ms < frame_ms {
            return frame;
        }
        cursor_ms = cursor_ms.saturating_sub(frame_ms);
    }

    &animation.frames[animation.frames.len() - 1]
}

fn paint_frame_glow(ui: &egui::Ui, rect: egui::Rect, rounding: egui::Rounding) {
    let painter = ui.painter();
    painter.rect_stroke(
        rect.expand(7.0),
        rounding,
        egui::Stroke::new(12.0, theme_glow_outer()),
    );
    painter.rect_stroke(
        rect.expand(2.5),
        rounding,
        egui::Stroke::new(2.0, theme_glow_inner()),
    );
}

fn card_frame(ui: &mut egui::Ui, title: &str, add_contents: impl FnOnce(&mut egui::Ui)) {
    let rounding = egui::Rounding::same(18.0);
    let response = egui::Frame::group(ui.style())
        .fill(theme_card())
        .stroke(egui::Stroke::new(1.0, theme_border()))
        .rounding(rounding)
        .inner_margin(egui::Margin::same(16.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading(RichText::new(title).color(theme_text()));
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |_ui| {});
            });
            ui.add_space(10.0);
            add_contents(ui);
        });
    paint_frame_glow(ui, response.response.rect, rounding);
}

fn labeled_value(ui: &mut egui::Ui, label: &str, value: impl Into<String>) {
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(format!("{label}:")).color(theme_muted()));
        ui.label(RichText::new(value.into()).color(theme_text()));
    });
}

fn metric(ui: &mut egui::Ui, label: &str, value: impl Into<String>) {
    ui.group(|ui| {
        ui.set_min_height(72.0);
        ui.label(RichText::new(label).size(12.0).color(theme_accent()));
        ui.add_space(4.0);
        ui.label(RichText::new(value.into()).size(22.0).color(theme_text()));
    });
}

fn metric_chip(ui: &mut egui::Ui, label: &str, value: impl Into<String>) {
    let rounding = egui::Rounding::same(16.0);
    let response = egui::Frame::group(ui.style())
        .fill(theme_card_alt())
        .stroke(egui::Stroke::new(1.0, theme_border()))
        .rounding(rounding)
        .inner_margin(egui::Margin::same(12.0))
        .show(ui, |ui| {
            ui.label(RichText::new(label).size(12.0).color(theme_muted()));
            ui.add_space(6.0);
            ui.label(RichText::new(value.into()).size(24.0).color(theme_text()));
        });
    paint_frame_glow(ui, response.response.rect, rounding);
}

fn decode_era_name(name: [u8; ERA_NAME_LEN]) -> String {
    let end = name.iter().position(|byte| *byte == 0).unwrap_or(name.len());
    String::from_utf8_lossy(&name[..end]).into_owned()
}

fn render_wallet_card(ui: &mut egui::Ui, app: &mut BlockMineStudioApp) {
    card_frame(ui, "Wallet", |ui| {
        let estimated_session_preload_lamports =
            desktop_session_fee_budget_lamports(app.phantom_session_max_blocks);
        let total_session_balance_lamports = app
            .session_balance_summary
            .as_ref()
            .map(|summary| summary.total_balance_lamports)
            .unwrap_or(0);
        let total_bloc_balance_raw = app
            .session_balance_summary
            .as_ref()
            .map(|summary| summary.total_bloc_balance_raw)
            .unwrap_or(0);
        let bloc_decimals = app
            .session_balance_summary
            .as_ref()
            .map(|summary| summary.bloc_decimals)
            .unwrap_or(9);
        let desktop_wallet_address = app
            .active_wallet
            .as_ref()
            .map(|wallet| wallet.pubkey.clone())
            .unwrap_or_else(|| "Not ready yet".to_string());
        let session_blocks_mineable =
            total_session_balance_lamports / TREASURY_FEE_PER_BLOCK_LAMPORTS;

        ui.label(
            "Keep some SOL in the desktop mining wallet so each winning block can pay the 0.01 SOL treasury fee without interrupting your run.",
        );
        ui.add_space(8.0);
        ui.label(
            RichText::new(format!(
                "Estimated funding target: {} SOL for up to {} winning blocks.",
                format_sol_compact(estimated_session_preload_lamports),
                app.phantom_session_max_blocks
            ))
            .color(theme_muted()),
        );
        ui.add_space(10.0);
        egui::Frame::group(ui.style())
            .fill(theme_card_alt())
            .stroke(egui::Stroke::new(1.0, theme_border()))
            .rounding(egui::Rounding::same(14.0))
            .inner_margin(egui::Margin::same(12.0))
            .show(ui, |ui| {
                labeled_value(ui, "Desktop wallet", desktop_wallet_address);
                labeled_value(
                    ui,
                    "Balance (SOL)",
                    format!("{} SOL", format_sol_compact(total_session_balance_lamports)),
                );
                labeled_value(
                    ui,
                    "Balance (BLOC)",
                    format!(
                        "{} BLOC",
                        format_decimal_amount_trimmed(total_bloc_balance_raw, bloc_decimals)
                    ),
                );
                labeled_value(
                    ui,
                    "Blocks mineable",
                    format!("{} winning blocks", session_blocks_mineable),
                );
            });

        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.label("Session block cap");
            ui.add(
                egui::Slider::new(
                    &mut app.phantom_session_max_blocks,
                    1..=MAX_PHANTOM_SESSION_BLOCKS,
                )
                .clamp_to_range(true)
                .show_value(false),
            );
            ui.label(
                RichText::new(format!("{} blocks", app.phantom_session_max_blocks))
                    .color(theme_accent()),
            );
        });
        ui.label(
            RichText::new(
                "Use this target to decide how much SOL you want to keep ready in the desktop wallet.",
            )
            .color(theme_muted()),
        );

        ui.add_space(12.0);
        ui.horizontal_wrapped(|ui| {
            if let Some(animation) = &app.wallet_animation {
                render_animated_texture(ui, animation, app.wallet_animation_started_at, 64.0);
            }
            if ui
                .add(
                    egui::Button::new(RichText::new("Deposit").color(theme_button_text()))
                        .fill(theme_accent())
                        .min_size(egui::vec2(220.0, 44.0)),
                )
                .clicked()
            {
                app.open_deposit_modal();
            }
            if ui
                .add_enabled(
                    (total_session_balance_lamports > 0 || total_bloc_balance_raw > 0)
                        && app.session_sweep_receiver.is_none(),
                    egui::Button::new("Withdrawal").min_size(egui::vec2(160.0, 44.0)),
                )
                .clicked()
            {
                app.withdrawal_sol_amount.clear();
                app.withdrawal_bloc_amount.clear();
                app.show_withdrawal_modal = true;
            }
        });
    });
}

fn render_miner_controls_card(ui: &mut egui::Ui, app: &mut BlockMineStudioApp) {
    card_frame(ui, "Miner controls", |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(&mut app.miner_controls_mode, MinerControlsMode::Fast, "Fast");
            ui.selectable_value(
                &mut app.miner_controls_mode,
                MinerControlsMode::Advanced,
                "Advanced",
            );
        });
        ui.add_space(10.0);

        match app.miner_controls_mode {
            MinerControlsMode::Fast => {
                ui.label(
                    RichText::new(
                        "Pick the mining path you want and start immediately. GPU mode still lets you choose which cards to use.",
                    )
                    .color(theme_muted()),
                );
                ui.add_space(10.0);
                render_gpu_picker(ui, app, false);
                ui.add_space(12.0);
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .add_enabled(
                            app.mining_handle.is_none(),
                            egui::Button::new(
                                RichText::new("CPU Mine").color(theme_button_text()),
                            )
                            .fill(theme_accent())
                            .min_size(egui::vec2(180.0, 42.0)),
                        )
                        .clicked()
                    {
                        app.start_cpu_mining();
                    }
                    if ui
                        .add_enabled(
                            app.mining_handle.is_none(),
                            egui::Button::new(
                                RichText::new("GPU Mine").color(theme_button_text()),
                            )
                            .fill(theme_accent())
                            .min_size(egui::vec2(180.0, 42.0)),
                        )
                        .clicked()
                    {
                        app.start_gpu_mining();
                    }
                    if ui
                        .add_enabled(
                            app.mining_handle.is_some(),
                            egui::Button::new("Stop mining").min_size(egui::vec2(140.0, 42.0)),
                        )
                        .clicked()
                    {
                        app.stop_mining();
                    }
                });
            }
            MinerControlsMode::Advanced => {
                ui.horizontal(|ui| {
                    ui.label("RPC");
                    ui.add(TextEdit::singleline(&mut app.rpc_url).desired_width(320.0));
                });
                ui.horizontal(|ui| {
                    ui.label("Program");
                    ui.add(TextEdit::singleline(&mut app.program_id).desired_width(320.0));
                });
                ui.add_space(12.0);

                egui::ComboBox::from_label("Backend")
                    .selected_text(match app.backend {
                        BackendMode::Cpu => "CPU",
                        BackendMode::Gpu => "GPU",
                        BackendMode::Both => "GPU",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut app.backend, BackendMode::Cpu, "CPU");
                        ui.selectable_value(&mut app.backend, BackendMode::Gpu, "GPU");
                    });

                ui.add_space(8.0);
                match app.backend {
                    BackendMode::Cpu => {
                        ui.label(
                            RichText::new(
                                "Tune CPU batch size, thread count, and core pinning for rigs that need more control.",
                            )
                            .color(theme_muted()),
                        );
                        ui.add_space(10.0);
                        grid_cpu_fields(ui, &mut app.batch_size, &mut app.cpu_threads);
                        ui.add_space(10.0);
                        render_cpu_core_picker(ui, app);
                    }
                    BackendMode::Gpu | BackendMode::Both => {
                        ui.label(
                            RichText::new(
                                "Select one or more GPUs, then only touch the advanced tuning if you really need it. Auto-tune settings are saved locally.",
                            )
                            .color(theme_muted()),
                        );
                        ui.add_space(10.0);
                        grid_gpu_primary_fields(ui, &mut app.gpu_batch_size);
                        ui.add_space(10.0);
                        render_gpu_picker(ui, app, true);
                        ui.add_space(10.0);
                        grid_gpu_fields(ui, &mut app.gpu_local_work_size);
                    }
                }

                ui.add_space(12.0);
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .add_enabled(
                            app.mining_handle.is_none(),
                            egui::Button::new(
                                RichText::new("Start mining").color(theme_button_text()),
                            )
                            .fill(theme_accent())
                            .min_size(egui::vec2(180.0, 42.0)),
                        )
                        .clicked()
                    {
                        app.start_mining();
                    }
                    if ui
                        .add_enabled(
                            app.mining_handle.is_some(),
                            egui::Button::new("Stop mining").min_size(egui::vec2(140.0, 42.0)),
                        )
                        .clicked()
                    {
                        app.stop_mining();
                    }
                });
            }
        }
    });
}

fn render_live_telemetry_card(ui: &mut egui::Ui, app: &mut BlockMineStudioApp) {
    card_frame(ui, "Live miner telemetry", |ui| {
        let current_era = reward_era_for_block(app.latest_snapshot.current_block_number);
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Current era").color(theme_muted()).size(13.0));
            ui.label(
                RichText::new(decode_era_name(current_era.name))
                    .color(theme_text())
                    .size(16.0)
                    .strong(),
            );
            if ui
                .add(
                    egui::Button::new(
                        RichText::new("i")
                            .color(theme_text())
                            .size(12.0)
                            .strong(),
                    )
                    .min_size(egui::vec2(22.0, 22.0))
                    .fill(theme_card_alt()),
                )
                .on_hover_text("Open the full mining curve")
                .clicked()
            {
                app.show_era_schedule_modal = true;
            }
        });
        ui.add_space(10.0);
        ui.columns(2, |cols| {
            metric(&mut cols[0], "Current block", format!("#{}", app.latest_snapshot.current_block_number));
            metric(
                &mut cols[1],
                "Difficulty",
                format!("{} bits", app.latest_snapshot.difficulty_bits),
            );
            metric(
                &mut cols[0],
                "Reward / block",
                format!("{} BLOC", format_bloc(app.latest_snapshot.current_reward)),
            );
            metric(
                &mut cols[1],
                "Current era",
                decode_era_name(current_era.name),
            );
            metric(
                &mut cols[0],
                "Hashrate",
                format_hashrate_compact(app.display_hashrate_hps),
            );
            metric(
                &mut cols[1],
                "Session wins",
                app.latest_snapshot.session_blocks_mined.to_string(),
            );
            metric(
                &mut cols[0],
                "Session BLOC",
                format!("{} BLOC", format_bloc(app.latest_snapshot.session_tokens_mined)),
            );
            metric(
                &mut cols[1],
                "Treasury fees",
                format!("{} BLOC", format_bloc(app.latest_snapshot.protocol_treasury_fees)),
            );
        });
    });
}

fn render_hashrate_signal_card(ui: &mut egui::Ui, app: &BlockMineStudioApp) {
    card_frame(ui, "Live compute signal", |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new("Desktop mining output in real time")
                    .size(24.0)
                    .color(Color32::WHITE),
            );
        });
        ui.add_space(6.0);
        ui.label(
            RichText::new(
                "The line stays still and adds one new point every 30 seconds. Each point is the highest hashrate seen in the last 30-second window.",
            )
            .color(theme_muted()),
        );

        ui.add_space(14.0);
        ui.columns(3, |columns| {
            metric_chip(
                &mut columns[0],
                "Live rate",
                format_hashrate_compact(app.display_hashrate_hps),
            );
            metric_chip(
                &mut columns[1],
                "Attempts",
                format_count_compact(app.latest_snapshot.session_hashes),
            );
            metric_chip(
                &mut columns[2],
                "Session wins",
                app.latest_snapshot.session_blocks_mined.to_string(),
            );
        });

        ui.add_space(14.0);
        egui::Frame::group(ui.style())
            .fill(theme_bg())
            .stroke(egui::Stroke::new(1.0, theme_border()))
            .rounding(egui::Rounding::same(20.0))
            .inner_margin(egui::Margin::same(14.0))
            .show(ui, |ui| {
                let chart_height = 220.0;
                let desired_size = egui::vec2(ui.available_width(), chart_height);
                let (rect, _) = ui.allocate_exact_size(desired_size, egui::Sense::hover());
                let painter = ui.painter_at(rect);
                let grid_color = Color32::from_rgba_premultiplied(88, 88, 96, 120);

                for step in 0..=5 {
                    let y = egui::lerp(rect.top()..=rect.bottom(), step as f32 / 5.0);
                    painter.line_segment(
                        [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                        egui::Stroke::new(1.0, grid_color),
                    );
                }

                for step in 0..=6 {
                    let x = egui::lerp(rect.left()..=rect.right(), step as f32 / 6.0);
                    painter.line_segment(
                        [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                        egui::Stroke::new(1.0, grid_color),
                    );
                }

                let series = app.hashrate_chart.display_series();
                let peak = app
                    .hashrate_chart
                    .real_points()
                    .into_iter()
                    .fold(app.display_hashrate_hps.max(1.0), f64::max);
                let baseline = (peak * 1.14).max(10_000.0);

                let points: Vec<egui::Pos2> = series
                    .iter()
                    .enumerate()
                    .map(|(index, value)| {
                        let x = egui::lerp(
                            rect.left()..=rect.right(),
                            index as f32 / (series.len().saturating_sub(1).max(1)) as f32,
                        );
                        let normalized = (*value / baseline).clamp(0.0, 1.0) as f32;
                        let y = rect.bottom() - normalized * (rect.height() - 12.0) - 6.0;
                        egui::pos2(x, y)
                    })
                    .collect();

                if app.hashrate_chart.committed_peaks.len() >= 2 {
                    painter.add(egui::Shape::line(
                        points,
                        egui::Stroke::new(3.5, theme_accent()),
                    ));
                } else {
                    painter.text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "Collecting 30s peaks...",
                        egui::TextStyle::Body.resolve(ui.style()),
                        theme_muted(),
                    );
                }
            });

        ui.add_space(12.0);
        ui.columns(3, |columns| {
            metric_chip(
                &mut columns[0],
                "30s peak",
                format_hashrate_compact(
                    app.hashrate_chart
                        .last_window_peak
                        .max(app.display_hashrate_hps),
                ),
            );
            metric_chip(
                &mut columns[1],
                "Chart average",
                format_hashrate_compact(
                    app.hashrate_chart
                        .chart_average(app.display_hashrate_hps),
                ),
            );
            metric_chip(&mut columns[2], "Runtime", app.runtime_label());
        });
    });
}

fn format_hashrate_compact(rate_hps: f64) -> String {
    if !rate_hps.is_finite() || rate_hps <= 0.0 {
        return "0 H/s".to_string();
    }

    let units = [
        ("H/s", 1.0_f64),
        ("kH/s", 1_000.0_f64),
        ("MH/s", 1_000_000.0_f64),
        ("GH/s", 1_000_000_000.0_f64),
        ("TH/s", 1_000_000_000_000.0_f64),
    ];

    for window in units.windows(2).rev() {
        let current = window[0];
        if rate_hps >= current.1 {
            return format!("{:.2} {}", rate_hps / current.1, current.0);
        }
    }

    format!("{:.0} H/s", rate_hps)
}

fn format_count_compact(value: u64) -> String {
    let value_f64 = value as f64;

    if value >= 1_000_000_000_000 {
        return format!("{:.2}T", value_f64 / 1_000_000_000_000.0);
    }
    if value >= 1_000_000_000 {
        return format!("{:.2}B", value_f64 / 1_000_000_000.0);
    }
    if value >= 1_000_000 {
        return format!("{:.2}M", value_f64 / 1_000_000.0);
    }
    if value >= 1_000 {
        return format!("{:.2}K", value_f64 / 1_000.0);
    }

    value.to_string()
}

fn render_mining_action_row(ui: &mut egui::Ui, app: &mut BlockMineStudioApp) {
    let mining_live = app.mining_handle.is_some();
    let wallet_ready = app.active_wallet.is_some();

    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(
                wallet_ready && !mining_live,
                egui::Button::new(
                    RichText::new("Start mining with this wallet")
                        .color(theme_button_text()),
                )
                .fill(theme_accent())
                .min_size(egui::vec2(240.0, 42.0)),
            )
            .clicked()
        {
            app.start_mining();
        }

        if ui
            .add_enabled(
                mining_live,
                egui::Button::new("Stop").min_size(egui::vec2(96.0, 42.0)),
            )
            .clicked()
        {
            app.stop_mining();
        }
    });

    if !wallet_ready {
        ui.label(
            RichText::new("Create or import a wallet first to unlock native mining.")
                .color(theme_muted()),
        );
    }
}

fn render_cpu_core_picker(ui: &mut egui::Ui, app: &mut BlockMineStudioApp) {
    ui.label(
        RichText::new("CPU core selection")
            .size(14.0)
            .color(theme_accent()),
    );
    ui.label(
        RichText::new(
            "Leave it on auto to use all logical cores, or pin the CPU miner to specific logical processors.",
        )
        .color(theme_muted()),
    );
    ui.add_space(8.0);

    ui.horizontal_wrapped(|ui| {
        let auto_selected = app.selected_cpu_cores.is_empty();
        if ui
            .selectable_label(auto_selected, format!("Auto (all {} cores)", app.available_cpu_cores.len()))
            .clicked()
        {
            app.selected_cpu_cores.clear();
        }

        if ui.button("Select all").clicked() {
            app.selected_cpu_cores = app.available_cpu_cores.iter().copied().collect();
        }

        if ui.button("Clear").clicked() {
            app.selected_cpu_cores.clear();
        }
    });

    ui.add_space(8.0);
    egui::Frame::group(ui.style())
        .fill(theme_card_alt())
        .stroke(egui::Stroke::new(1.0, theme_border()))
        .rounding(egui::Rounding::same(14.0))
        .inner_margin(egui::Margin::same(12.0))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                for core_id in &app.available_cpu_cores {
                    let selected = app.selected_cpu_cores.contains(core_id);
                    if ui
                        .selectable_label(selected, format!("CPU {}", core_id))
                        .clicked()
                    {
                        if selected {
                            app.selected_cpu_cores.remove(core_id);
                        } else {
                            app.selected_cpu_cores.insert(*core_id);
                        }
                    }
                }
            });
        });

    let selected_summary = selected_cpu_cores(&app.selected_cpu_cores)
        .map(|cores| {
            cores.into_iter()
                .map(|core| core.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_else(|| "all logical cores".to_string());
    ui.add_space(6.0);
    ui.label(
        RichText::new(format!("Pinned CPUs: {selected_summary}"))
            .color(theme_muted()),
    );
}

fn render_gpu_picker(ui: &mut egui::Ui, app: &mut BlockMineStudioApp, show_autotune: bool) {
    ui.label(
        RichText::new("GPU selection")
            .size(14.0)
            .color(theme_accent()),
    );
    ui.label(
        RichText::new("Tick the GPUs you want to mine with. The miner launches one worker per selected device and sums the throughput.")
            .color(theme_muted()),
    );
    ui.add_space(8.0);

    ui.horizontal_wrapped(|ui| {
        if ui.button("Refresh GPU list").clicked() {
            app.refresh_gpu_devices();
        }

        if let Some(error) = &app.gpu_devices_error {
            ui.colored_label(theme_error(), error);
        } else if app.gpu_devices.is_empty() {
            ui.label(RichText::new("No OpenCL GPUs found yet.").color(theme_text()));
        }
    });

    if app.gpu_devices.is_empty() {
        return;
    }

    ui.horizontal_wrapped(|ui| {
        if ui.button("Select all GPUs").clicked() {
            app.selected_gpu_keys = app.gpu_devices.iter().map(device_selection_key).collect();
            app.sync_selected_gpu_selection();
            app.persist_ui_preferences();
        }
        if ui.button("Clear").clicked() {
            app.selected_gpu_keys.clear();
            app.persist_ui_preferences();
        }
    });

    ui.add_space(8.0);
    egui::Frame::group(ui.style())
        .fill(theme_card_alt())
        .stroke(egui::Stroke::new(1.0, theme_border()))
        .rounding(egui::Rounding::same(14.0))
        .inner_margin(egui::Margin::same(12.0))
        .show(ui, |ui| {
            for device in app.gpu_devices.clone() {
                let key = device_selection_key(&device);
                let mut selected = app.selected_gpu_keys.contains(&key);
                if ui.checkbox(&mut selected, format_gpu_device_label(&device)).changed() {
                    if selected {
                        app.selected_gpu_keys.insert(key);
                    } else {
                        app.selected_gpu_keys.remove(&key);
                    }
                    app.sync_selected_gpu_selection();
                    app.persist_ui_preferences();
                }
            }
        });

    let selected_devices = app.selected_gpu_devices();
    ui.add_space(6.0);
    ui.label(
        RichText::new(format!(
            "Selected GPUs: {} of {}",
            selected_devices.len(),
            app.gpu_devices.len()
        ))
        .color(theme_muted()),
    );

    if !selected_devices.is_empty() {
        ui.add_space(8.0);
        egui::Frame::group(ui.style())
            .fill(theme_card_alt())
            .stroke(egui::Stroke::new(1.0, theme_border()))
            .rounding(egui::Rounding::same(14.0))
            .inner_margin(egui::Margin::same(12.0))
            .show(ui, |ui| {
                for device in selected_devices.iter().copied() {
                    labeled_value(ui, "GPU", &device.device_name);
                    labeled_value(ui, "Vendor", &device.vendor);
                    labeled_value(ui, "Platform", &device.platform_name);
                    labeled_value(ui, "Memory", format_gpu_memory(device.global_memory_bytes));
                    labeled_value(ui, "Compute units", device.max_compute_units.to_string());
                    labeled_value(ui, "Max work-group", device.max_work_group_size.to_string());
                    ui.add_space(6.0);
                }
            });

        if show_autotune {
            ui.add_space(10.0);
            ui.horizontal_wrapped(|ui| {
                let autotune_live = app.gpu_autotune_receiver.is_some();
                if ui
                    .add_enabled(
                        !autotune_live && app.mining_handle.is_none(),
                        egui::Button::new(
                            RichText::new("Auto-tune selected GPUs")
                                .color(theme_button_text()),
                        )
                        .fill(theme_accent())
                        .min_size(egui::vec2(210.0, 38.0)),
                    )
                    .clicked()
                {
                    app.start_gpu_autotune();
                }

                if autotune_live {
                    ui.label(
                        RichText::new("Testing shared GPU profiles... this can take around 20-40 seconds.")
                            .color(theme_muted()),
                    );
                } else {
                    ui.label(
                        RichText::new(
                            "Auto-tune benchmarks the selected GPUs together and applies one shared profile with the best aggregate throughput.",
                        )
                        .color(theme_muted()),
                    );
                }
            });

            if let Some(status) = &app.gpu_autotune_status {
                ui.add_space(6.0);
                ui.label(RichText::new(status).color(theme_success()));
            }

            if let Some(best) = &app.gpu_autotune_best {
                ui.add_space(6.0);
                ui.label(
                    RichText::new(format!(
                        "Best aggregate profile: batch {} / local {} / {}",
                        format_count_compact(best.batch_size),
                        format_local_work_size_label(best.local_work_size),
                        format_hashrate_compact(best.hashrate_hps)
                    ))
                    .color(theme_accent()),
                );
            }
        }
    }
}

fn grid_cpu_fields(ui: &mut egui::Ui, batch_size: &mut String, cpu_threads: &mut String) {
    egui::Grid::new("mining_fields_cpu")
        .num_columns(2)
        .spacing([12.0, 10.0])
        .show(ui, |ui| {
            ui.label("CPU batch size");
            ui.add(TextEdit::singleline(batch_size).desired_width(120.0));
            ui.end_row();
            ui.label("CPU threads");
            ui.add(TextEdit::singleline(cpu_threads).desired_width(120.0));
            ui.end_row();
        });
}

fn grid_gpu_primary_fields(ui: &mut egui::Ui, gpu_batch_size: &mut String) {
    egui::Grid::new("mining_fields_gpu_primary")
        .num_columns(2)
        .spacing([12.0, 10.0])
        .show(ui, |ui| {
            ui.label("GPU batch size");
            ui.add(TextEdit::singleline(gpu_batch_size).desired_width(120.0));
            ui.end_row();
        });
}

fn detect_available_cpu_cores() -> Vec<usize> {
    let count = std::thread::available_parallelism()
        .map(|parallelism| parallelism.get())
        .unwrap_or(1);
    (0..count).collect()
}

fn selected_cpu_cores(selected: &BTreeSet<usize>) -> Option<Vec<usize>> {
    if selected.is_empty() {
        None
    } else {
        Some(selected.iter().copied().collect())
    }
}

fn format_gpu_device_label(device: &GpuDeviceInfo) -> String {
    format!(
        "{} | {} | platform {} device {}",
        device.device_name, device.vendor, device.platform_index, device.device_index
    )
}

fn device_selection_key(device: &GpuDeviceInfo) -> String {
    format!("{}:{}", device.platform_index, device.device_index)
}

fn format_gpu_memory(bytes: u64) -> String {
    if bytes >= 1_000_000_000 {
        format!("{:.2} GB", bytes as f64 / 1_000_000_000.0)
    } else if bytes >= 1_000_000 {
        format!("{:.2} MB", bytes as f64 / 1_000_000.0)
    } else {
        format!("{bytes} B")
    }
}

fn format_local_work_size_label(local_work_size: Option<usize>) -> String {
    local_work_size
        .map(|size| size.to_string())
        .unwrap_or_else(|| "auto".to_string())
}

fn grid_gpu_fields(ui: &mut egui::Ui, gpu_local_work_size: &mut String) {
    egui::Grid::new("mining_fields_b")
        .num_columns(2)
        .spacing([12.0, 10.0])
        .show(ui, |ui| {
            ui.label("GPU local work size");
            ui.add(TextEdit::singleline(gpu_local_work_size).desired_width(120.0));
            ui.end_row();
        });
}

fn parse_u64_field(raw: &str, label: &str) -> Result<u64> {
    raw.trim()
        .parse::<u64>()
        .with_context(|| format!("{label} must be a valid number"))
}

fn parse_usize_field(raw: &str, label: &str) -> Result<usize> {
    raw.trim()
        .parse::<usize>()
        .with_context(|| format!("{label} must be a valid number"))
}

fn parse_optional_usize_field(raw: &str, label: &str) -> Result<Option<usize>> {
    if raw.trim().is_empty() {
        return Ok(None);
    }

    Ok(Some(parse_usize_field(raw, label)?))
}

fn open_in_default_browser(url: &str) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", url])
            .spawn()
            .with_context(|| format!("failed to open {url}"))?;
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open")
            .arg(url)
            .spawn()
            .with_context(|| format!("failed to open {url}"))?;
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(url)
            .spawn()
            .with_context(|| format!("failed to open {url}"))?;
        return Ok(());
    }

    #[allow(unreachable_code)]
    Err(anyhow::anyhow!("opening the browser is not supported on this platform"))
}

fn build_gpu_autotune_candidates(devices: &[GpuDeviceInfo]) -> Vec<(u64, Option<usize>)> {
    if devices.is_empty() {
        return Vec::new();
    }

    let shared_max_work_group_size = devices
        .iter()
        .map(|device| device.max_work_group_size)
        .min()
        .unwrap_or(0);
    let mut local_sizes = vec![None];
    for size in [64usize, 128, 256] {
        if size <= shared_max_work_group_size && !local_sizes.contains(&Some(size)) {
            local_sizes.push(Some(size));
        }
    }

    if shared_max_work_group_size > 0
        && ![64usize, 128, 256].contains(&shared_max_work_group_size)
        && !local_sizes.contains(&Some(shared_max_work_group_size))
    {
        local_sizes.push(Some(shared_max_work_group_size));
    }

    let has_nvidia = devices.iter().any(|device| {
        let vendor = device.vendor.to_ascii_lowercase();
        let device_name = device.device_name.to_ascii_lowercase();
        vendor.contains("nvidia") || device_name.contains("nvidia")
    });
    let batch_sizes: Vec<u64> = if has_nvidia {
        vec![524_288, 1_048_576, 2_097_152, 4_194_304]
    } else {
        vec![1_048_576, 2_097_152, 4_194_304, 8_388_608]
    };

    let mut candidates = Vec::new();
    for batch_size in batch_sizes {
        for local_work_size in &local_sizes {
            candidates.push((batch_size, *local_work_size));
        }
    }
    candidates
}

fn run_gpu_autotune(
    devices: Vec<GpuDeviceInfo>,
    candidates: Vec<(u64, Option<usize>)>,
    sender: &mpsc::Sender<GpuAutotuneMessage>,
) -> Result<GpuAutotuneOutcome> {
    let mut tested = Vec::new();
    let total = candidates.len();

    for (index, (batch_size, local_work_size)) in candidates.into_iter().enumerate() {
        let _ = sender.send(GpuAutotuneMessage::Progress(format!(
            "Testing profile {}/{}: batch {} / local {}...",
            index + 1,
            total,
            format_count_compact(batch_size),
            format_local_work_size_label(local_work_size)
        )));

        let hashrate_hps = benchmark_gpu_profile(&devices, batch_size, local_work_size)
            .with_context(|| {
                format!(
                    "GPU auto-tune failed on batch {} / local {}",
                    batch_size,
                    format_local_work_size_label(local_work_size)
                )
            })?;
        tested.push(GpuAutotuneCandidate {
            batch_size,
            local_work_size,
            hashrate_hps,
        });
    }

    let raw_best = tested
        .iter()
        .cloned()
        .max_by(|left, right| {
            left.hashrate_hps
                .partial_cmp(&right.hashrate_hps)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .context("GPU auto-tune did not produce any benchmark results")?;

    // Prefer the smallest batch among profiles that stay within 97% of the
    // fastest throughput. This keeps the GPU path more responsive on desktop
    // rigs, especially on NVIDIA laptop parts where giant batches feel bursty.
    let best = tested
        .iter()
        .filter(|candidate| candidate.hashrate_hps >= raw_best.hashrate_hps * 0.97)
        .cloned()
        .min_by(|left, right| {
            left.batch_size
                .cmp(&right.batch_size)
                .then_with(|| left.local_work_size.cmp(&right.local_work_size))
        })
        .unwrap_or(raw_best);

    Ok(GpuAutotuneOutcome { best, tested })
}

fn benchmark_gpu_profile(
    devices: &[GpuDeviceInfo],
    batch_size: u64,
    local_work_size: Option<usize>,
) -> Result<f64> {
    let (sender, receiver) = mpsc::channel();

    thread::scope(|scope| {
        for device in devices.iter().cloned() {
            let sender = sender.clone();
            scope.spawn(move || {
                let miner = GpuMiner::new(device.platform_index, device.device_index, local_work_size);
                let result = miner.benchmark_with_batch_size(2, batch_size);
                let _ = sender.send(result);
            });
        }
        drop(sender);

        let mut aggregate_hashes = 0u64;
        let mut aggregate_elapsed = 0.0f64;
        for result in receiver {
            let report = result?;
            aggregate_hashes = aggregate_hashes.saturating_add(report.hashes);
            aggregate_elapsed = aggregate_elapsed.max(report.elapsed.as_secs_f64());
        }

        Ok::<f64, anyhow::Error>(
            aggregate_hashes as f64 / aggregate_elapsed.max(0.000_001),
        )
    })
}

fn start_phantom_bridge_listener(
) -> Result<(Receiver<Result<PhantomBridgeCompletion, String>>, String, String)> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .context("failed to bind the local Phantom bridge listener")?;
    let port = listener
        .local_addr()
        .context("failed to read the Phantom bridge listener address")?
        .port();
    let token = format!(
        "{:016x}{:016x}",
        rand::thread_rng().gen::<u64>(),
        rand::thread_rng().gen::<u64>()
    );
    let callback_url = format!("http://127.0.0.1:{port}/complete");
    let (sender, receiver) = mpsc::channel();
    let expected_token = token.clone();

    thread::spawn(move || {
        let result = wait_for_phantom_bridge_completion(listener, &expected_token)
            .map_err(|error| error.to_string());
        let _ = sender.send(result);
    });

    Ok((receiver, callback_url, token))
}

fn wait_for_phantom_bridge_completion(
    listener: TcpListener,
    expected_token: &str,
) -> Result<PhantomBridgeCompletion> {
    let (mut stream, _) = listener
        .accept()
        .context("Phantom bridge listener did not receive a callback")?;
    let mut buffer = [0u8; 8192];
    let read = stream
        .read(&mut buffer)
        .context("failed to read the Phantom bridge callback request")?;
    let request = String::from_utf8_lossy(&buffer[..read]);
    let request_line = request
        .lines()
        .next()
        .context("Phantom bridge callback request was empty")?;
    let path = request_line
        .split_whitespace()
        .nth(1)
        .context("failed to parse the Phantom bridge callback request line")?;

    let query = path
        .split_once('?')
        .map(|(_, query)| query)
        .unwrap_or_default();
    let params = parse_query_string(query);
    let token = params
        .get("token")
        .context("bridge callback did not include a token")?;
    if token != expected_token {
        respond_to_bridge_request(&mut stream, false)?;
        anyhow::bail!("bridge callback token mismatch")
    }

    let miner_pubkey = params
        .get("miner")
        .context("bridge callback did not include the Phantom wallet")?
        .parse::<Pubkey>()
        .context("invalid Phantom wallet received from the bridge")?;
    let delegate_pubkey = params
        .get("delegate")
        .context("bridge callback did not include the delegate wallet")?
        .parse::<Pubkey>()
        .context("invalid delegate wallet received from the bridge")?;
    let signature = params
        .get("signature")
        .cloned()
        .context("bridge callback did not include the authorization signature")?;

    respond_to_bridge_request(&mut stream, true)?;
    Ok(PhantomBridgeCompletion {
        token: token.clone(),
        miner_pubkey,
        delegate_pubkey,
        signature,
    })
}

fn respond_to_bridge_request(stream: &mut std::net::TcpStream, success: bool) -> Result<()> {
    let (status_line, body) = if success {
        (
            "HTTP/1.1 200 OK",
        "<html><body style=\"font-family:sans-serif;background:#071018;color:#fff;padding:24px;\">Wallet connected. You can close this tab and return to the desktop miner.</body></html>",
        )
    } else {
        (
            "HTTP/1.1 400 Bad Request",
            "<html><body style=\"font-family:sans-serif;background:#071018;color:#fff;padding:24px;\">The desktop miner rejected this callback.</body></html>",
        )
    };
    let response = format!(
        "{status_line}\r\nContent-Type: text/html; charset=utf-8\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(response.as_bytes())
        .context("failed to respond to the Phantom bridge callback")?;
    Ok(())
}

fn parse_query_string(query: &str) -> std::collections::HashMap<String, String> {
    let mut params = std::collections::HashMap::new();
    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        params.insert(url_decode_component(key), url_decode_component(value));
    }
    params
}

fn url_encode_component(input: &str) -> String {
    input
        .bytes()
        .flat_map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                vec![byte as char]
            }
            b' ' => vec!['%','2','0'],
            _ => format!("%{:02X}", byte).chars().collect(),
        })
        .collect()
}

fn url_decode_component(input: &str) -> String {
    let mut output = Vec::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut index = 0;

    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                output.push(b' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                let hex = &input[index + 1..index + 3];
                if let Ok(value) = u8::from_str_radix(hex, 16) {
                    output.push(value);
                    index += 3;
                } else {
                    output.push(bytes[index]);
                    index += 1;
                }
            }
            byte => {
                output.push(byte);
                index += 1;
            }
        }
    }

    String::from_utf8_lossy(&output).to_string()
}

fn derive_site_origin(raw_url: &str) -> Option<String> {
    let trimmed = raw_url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }

    let scheme_index = trimmed.find("://")?;
    let authority_start = scheme_index + 3;
    let path_start = trimmed[authority_start..]
        .find('/')
        .map(|offset| authority_start + offset);

    Some(match path_start {
        Some(index) => trimmed[..index].to_string(),
        None => trimmed.to_string(),
    })
}

fn derive_leaderboard_ingest_url(raw_url: &str) -> Option<String> {
    derive_site_origin(raw_url).map(|origin| format!("{origin}/api/leaderboard/heartbeat"))
}
