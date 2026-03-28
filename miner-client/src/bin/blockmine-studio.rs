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
use blockmine_miner::config::CliConfig;
use blockmine_miner::engine::gpu::{list_devices as list_gpu_devices, GpuDeviceInfo, GpuMiner};
use blockmine_miner::engine::BackendMode;
use blockmine_miner::miner_loop::GpuDeviceSelection;
use blockmine_miner::mining_service::{
    MiningHandle, MiningRuntimeOptions, MiningSnapshot, MiningUpdate,
};
use blockmine_miner::rpc::RpcFacade;
use blockmine_miner::session_wallet::{
    load_managed_wallet_balances, sweep_single_session_delegate_wallet, SessionBalanceSummary,
    SessionSweepSummary,
};
use blockmine_miner::ui::format_bloc;
use blockmine_miner::wallet_store::{
    app_storage_dir, create_dedicated_wallet, create_session_delegate_wallet,
    import_wallet_from_private_key, import_wallet_from_seed_phrase, list_managed_wallets, load_managed_keypair,
    load_session_delegate_wallet, load_wallet_seed_phrase, ManagedWallet, WalletSource,
};
use blockmine_program::math::rewards::{reward_era_for_block, ERA_NAME_LEN};
use eframe::egui::{
    self, Align, Color32, IconData, RichText, TextEdit, TextureHandle, TextureOptions,
};
use eframe::{App, Frame, NativeOptions};
use image::codecs::gif::GifDecoder;
use image::{AnimationDecoder, ImageReader};
use qrcode::{types::Color as QrColor, QrCode};
use rand::Rng;
use serde::{Deserialize, Serialize};
use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey};
use sysinfo::System;

const DEFAULT_RPC_URL: &str = "https://api.mainnet-beta.solana.com";
const DEFAULT_PROGRAM_ID: &str = "FgRe73gAkZPhxpiCFHMYMfLR4dabDaB1FDVFazVTcCtv";
const DEFAULT_BROWSER_MINE_URL: &str = "https://blockmine.dev/desktop-bridge";
const TREASURY_FEE_PER_BLOCK_LAMPORTS: u64 = 10_000_000;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DepositModalStep {
    Picker,
    Details,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FastMiningChoice {
    Cpu,
    Gpu,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AddWalletMode {
    SeedPhrase,
    PrivateKey,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DesktopUiPreferences {
    #[serde(default)]
    selected_wallet_pubkey: Option<String>,
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
        let count = area.round().clamp(
            BACKGROUND_MIN_PARTICLES as f32,
            BACKGROUND_MAX_PARTICLES as f32,
        ) as usize;

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
        .with_inner_size([1500.0, 980.0])
        .with_min_inner_size([1180.0, 820.0])
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
    web3_deposit_sol: f32,
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
    available_wallets: Vec<ManagedWallet>,
    active_wallet: Option<ManagedWallet>,
    phantom_session: Option<PhantomSessionLink>,
    pending_phantom_bridge: Option<PendingPhantomBridge>,
    phantom_bridge_receiver: Option<Receiver<Result<PhantomBridgeCompletion, String>>>,
    session_balance_receiver: Option<Receiver<Result<SessionBalanceSummary, String>>>,
    session_sweep_receiver: Option<Receiver<Result<SessionSweepSummary, String>>>,
    current_block_receiver: Option<Receiver<Result<(u64, u8, u64), String>>>,
    gpu_autotune_receiver: Option<Receiver<GpuAutotuneMessage>>,
    gpu_autotune_status: Option<String>,
    gpu_autotune_best: Option<GpuAutotuneCandidate>,
    active_runtime_wallet: Option<ManagedWallet>,
    active_runtime_miner: Option<Pubkey>,
    session_balance_summary: Option<SessionBalanceSummary>,
    last_session_balance_refresh_at: Instant,
    last_current_block_refresh_at: Instant,
    show_deposit_modal: bool,
    show_withdrawal_modal: bool,
    show_era_schedule_modal: bool,
    show_seed_phrase_modal: bool,
    show_seed_phrase_warning_modal: bool,
    show_add_wallet_modal: bool,
    deposit_method: DepositMethod,
    deposit_modal_step: DepositModalStep,
    add_wallet_mode: AddWalletMode,
    add_wallet_label: String,
    add_wallet_secret: String,
    seed_phrase_words: Vec<String>,
    seed_phrase_requires_ack: bool,
    seed_phrase_acknowledged: bool,
    withdrawal_target_wallet: String,
    withdrawal_sol_amount: String,
    withdrawal_bloc_amount: String,
    miner_controls_mode: MinerControlsMode,
    fast_mining_choice: FastMiningChoice,
    cpu_model_label: String,
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
        let existing_wallet = load_session_delegate_wallet().ok().flatten();
        let desktop_wallet = existing_wallet
            .clone()
            .or_else(|| create_session_delegate_wallet(Some("desktop-session")).ok());
        let initial_seed_phrase = if existing_wallet.is_none() {
            desktop_wallet
                .as_ref()
                .and_then(|wallet| wallet.seed_phrase.clone())
                .unwrap_or_default()
        } else {
            String::new()
        };
        let preferences = load_desktop_ui_preferences().ok().flatten();
        let selected_wallet_pubkey = preferences
            .as_ref()
            .and_then(|prefs| prefs.selected_wallet_pubkey.clone());
        let selected_gpu_keys = preferences
            .as_ref()
            .map(|prefs| prefs.selected_gpu_keys.iter().cloned().collect())
            .unwrap_or_default();
        let mut available_wallets = list_managed_wallets().unwrap_or_default();
        if let Some(wallet) = desktop_wallet.as_ref() {
            if !available_wallets.iter().any(|candidate| candidate.pubkey == wallet.pubkey) {
                available_wallets.insert(0, wallet.clone());
            }
        }
        let active_wallet = selected_wallet_pubkey
            .as_ref()
            .and_then(|pubkey| {
                available_wallets
                    .iter()
                    .find(|wallet| wallet.pubkey == *pubkey)
                    .cloned()
            })
            .or_else(|| desktop_wallet.clone())
            .or_else(|| available_wallets.first().cloned());

        let mut app = Self {
            rpc_url: DEFAULT_RPC_URL.to_string(),
            program_id: DEFAULT_PROGRAM_ID.to_string(),
            browser_mine_url: DEFAULT_BROWSER_MINE_URL.to_string(),
            web3_deposit_sol: 0.6,
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
            available_wallets,
            active_wallet,
            phantom_session: None,
            pending_phantom_bridge: None,
            phantom_bridge_receiver: None,
            session_balance_receiver: None,
            session_sweep_receiver: None,
            current_block_receiver: None,
            gpu_autotune_receiver: None,
            gpu_autotune_status: None,
            gpu_autotune_best: None,
            active_runtime_wallet: None,
            active_runtime_miner: None,
            session_balance_summary: None,
            last_session_balance_refresh_at: Instant::now(),
            last_current_block_refresh_at: Instant::now() - Duration::from_secs(10),
            show_deposit_modal: false,
            show_withdrawal_modal: false,
            show_era_schedule_modal: false,
            show_seed_phrase_modal: false,
            show_seed_phrase_warning_modal: false,
            show_add_wallet_modal: false,
            deposit_method: DepositMethod::Web3Wallet,
            deposit_modal_step: DepositModalStep::Picker,
            add_wallet_mode: AddWalletMode::SeedPhrase,
            add_wallet_label: String::new(),
            add_wallet_secret: String::new(),
            seed_phrase_words: split_seed_phrase_words(&initial_seed_phrase),
            seed_phrase_requires_ack: !initial_seed_phrase.is_empty(),
            seed_phrase_acknowledged: false,
            withdrawal_target_wallet: String::new(),
            withdrawal_sol_amount: String::new(),
            withdrawal_bloc_amount: String::new(),
            miner_controls_mode: MinerControlsMode::Fast,
            fast_mining_choice: FastMiningChoice::Cpu,
            cpu_model_label: detect_cpu_model(),
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
            status: if initial_seed_phrase.is_empty() {
                "Desktop mining wallet ready. Deposit SOL, then start mining.".to_string()
            } else {
                "Before you start mining, save your recovery phrase.".to_string()
            },
            error: None,
        };
        app.refresh_gpu_devices();
        app.start_session_balance_refresh();
        app
    }

    fn open_deposit_modal(&mut self) {
        if !self.ensure_active_wallet() {
            return;
        }
        self.show_deposit_modal = true;
        self.deposit_modal_step = DepositModalStep::Picker;
        self.error = None;
    }

    fn ensure_active_wallet(&mut self) -> bool {
        if self.active_wallet.is_some() {
            return true;
        }

        match create_session_delegate_wallet(Some("desktop-session")) {
            Ok(wallet) => {
                if !self
                    .available_wallets
                    .iter()
                    .any(|candidate| candidate.pubkey == wallet.pubkey)
                {
                    self.available_wallets.insert(0, wallet.clone());
                }
                self.active_wallet = Some(wallet.clone());
                if let Some(phrase) = wallet.seed_phrase.clone() {
                    self.seed_phrase_words = split_seed_phrase_words(&phrase);
                    self.seed_phrase_requires_ack = true;
                    self.seed_phrase_acknowledged = false;
                    self.status = "Before you start mining, save your recovery phrase.".to_string();
                }
                true
            }
            Err(error) => {
                self.error = Some(format!(
                    "Failed to prepare the desktop mining wallet: {error}"
                ));
                false
            }
        }
    }

    fn refresh_available_wallets(&mut self) {
        let selected_pubkey = self
            .active_wallet
            .as_ref()
            .map(|wallet| wallet.pubkey.clone());

        let mut wallets = match list_managed_wallets() {
            Ok(wallets) => wallets,
            Err(error) => {
                self.error = Some(format!("Failed to load local wallets: {error}"));
                return;
            }
        };

        if let Ok(Some(session_wallet)) = load_session_delegate_wallet() {
            if !wallets
                .iter()
                .any(|candidate| candidate.pubkey == session_wallet.pubkey)
            {
                wallets.insert(0, session_wallet);
            }
        }

        self.available_wallets = wallets;
        self.active_wallet = selected_pubkey
            .as_ref()
            .and_then(|pubkey| {
                self.available_wallets
                    .iter()
                    .find(|wallet| wallet.pubkey == *pubkey)
                    .cloned()
            })
            .or_else(|| self.available_wallets.first().cloned());
    }

    fn select_active_wallet(&mut self, wallet: ManagedWallet) {
        self.active_wallet = Some(wallet.clone());
        self.seed_phrase_requires_ack = false;
        self.seed_phrase_acknowledged = false;
        self.persist_ui_preferences();
        self.session_balance_receiver = None;
        self.session_balance_summary = None;
        self.last_session_balance_refresh_at = Instant::now() - Duration::from_secs(3);
        self.start_session_balance_refresh();
        self.status = if self.mining_handle.is_some() {
            format!(
                "Wallet {} selected. Stop and restart mining to switch the live runtime wallet.",
                shorten_pubkey(&wallet.pubkey)
            )
        } else {
            format!("Active wallet set to {}.", shorten_pubkey(&wallet.pubkey))
        };
        self.error = None;
    }

    fn reset_wallet_manager_form(&mut self) {
        self.add_wallet_label.clear();
        self.add_wallet_secret.clear();
        self.add_wallet_mode = AddWalletMode::SeedPhrase;
    }

    fn create_managed_wallet(&mut self) {
        if self.mining_handle.is_some() {
            self.error = Some("Stop mining before creating a new wallet.".to_string());
            return;
        }

        let label = self.add_wallet_label.trim();
        match create_dedicated_wallet((!label.is_empty()).then_some(label)) {
            Ok(wallet) => {
                self.refresh_available_wallets();
                self.select_active_wallet(wallet.clone());
                if let Ok(Some(phrase)) = load_wallet_seed_phrase(&wallet) {
                    self.seed_phrase_words = split_seed_phrase_words(&phrase);
                    self.seed_phrase_requires_ack = true;
                    self.seed_phrase_acknowledged = false;
                    self.show_seed_phrase_modal = true;
                    self.status = "New wallet created. Save the recovery phrase before mining."
                        .to_string();
                }
                self.show_add_wallet_modal = false;
                self.reset_wallet_manager_form();
            }
            Err(error) => {
                self.error = Some(format!("Failed to create a new wallet: {error}"));
            }
        }
    }

    fn import_managed_wallet(&mut self) {
        if self.mining_handle.is_some() {
            self.error = Some("Stop mining before importing a wallet.".to_string());
            return;
        }

        let label = self.add_wallet_label.trim();
        let secret = self.add_wallet_secret.trim();
        if secret.is_empty() {
            self.error = Some("Paste a recovery phrase or private key first.".to_string());
            return;
        }

        let result = match self.add_wallet_mode {
            AddWalletMode::SeedPhrase => {
                import_wallet_from_seed_phrase(secret, (!label.is_empty()).then_some(label))
            }
            AddWalletMode::PrivateKey => {
                import_wallet_from_private_key(secret, (!label.is_empty()).then_some(label))
            }
        };

        match result {
            Ok(wallet) => {
                self.refresh_available_wallets();
                self.select_active_wallet(wallet.clone());
                self.show_add_wallet_modal = false;
                self.reset_wallet_manager_form();
                self.status = format!(
                    "Wallet {} imported and selected.",
                    shorten_pubkey(&wallet.pubkey)
                );
                self.error = None;
            }
            Err(error) => {
                self.error = Some(format!("Failed to import wallet: {error}"));
            }
        }
    }

    fn open_seed_phrase_for_wallet(&mut self, require_ack: bool) {
        let Some(wallet) = self.active_wallet.as_ref() else {
            self.error = Some("The desktop mining wallet is not ready yet.".to_string());
            return;
        };

        match load_wallet_seed_phrase(wallet) {
            Ok(Some(phrase)) => {
                self.seed_phrase_words = split_seed_phrase_words(&phrase);
                self.seed_phrase_requires_ack = require_ack;
                self.seed_phrase_acknowledged = false;
                self.show_seed_phrase_modal = true;
                self.error = None;
            }
            Ok(None) => {
                self.error = Some(
                    "This wallet was created before recovery phrases were enabled. Move the funds to a fresh wallet before mainnet."
                        .to_string(),
                );
            }
            Err(error) => {
                self.error = Some(format!("Failed to load the recovery phrase: {error}"));
            }
        }
    }

    fn open_web3_deposit_flow(&mut self) {
        let Some(wallet) = &self.active_wallet else {
            self.error = Some("The desktop mining wallet is not ready yet.".to_string());
            return;
        };

        let deposit_lamports = sol_to_lamports(self.web3_deposit_sol.max(0.1));
        let deposit_block_cap = (deposit_lamports / TREASURY_FEE_PER_BLOCK_LAMPORTS).max(1);
        let desktop_bridge_url = format!(
            "{}?desktop_deposit=1&desktop_wallet={}&desktop_deposit_lamports={}&desktop_max_submissions={}",
            self.browser_mine_url.trim_end_matches('/'),
            wallet.pubkey,
            deposit_lamports,
            deposit_block_cap
        );

        match open_in_default_browser(&desktop_bridge_url) {
            Ok(()) => {
                self.show_deposit_modal = false;
                self.status = format!(
                    "Web3 deposit opened. Approve about {} SOL in the browser wallet to fund the miner.",
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

        let Some(wallet) = self.active_wallet.clone() else {
            return;
        };

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
            let result = load_managed_wallet_balances(&rpc_url, program_id, &wallet)
                .map_err(|error| error.to_string());
            let _ = sender.send(result);
        });
    }

    fn start_current_block_refresh(&mut self) {
        if self.current_block_receiver.is_some() {
            return;
        }

        let config = match self.build_cli_config(None) {
            Ok(config) => config,
            Err(error) => {
                self.error = Some(error.to_string());
                return;
            }
        };

        let (sender, receiver) = mpsc::channel();
        self.current_block_receiver = Some(receiver);
        self.last_current_block_refresh_at = Instant::now();

        thread::spawn(move || {
            let rpc = RpcFacade::new(&config);
            let result = rpc
                .fetch_current_block()
                .map(|block| {
                    (
                        block.block_number,
                        block.difficulty_bits,
                        block.block_reward,
                    )
                })
                .map_err(|error| error.to_string());
            let _ = sender.send(result);
        });
    }

    fn persist_ui_preferences(&self) {
        if let Err(error) = save_desktop_ui_preferences(&DesktopUiPreferences {
            selected_wallet_pubkey: self.active_wallet.as_ref().map(|wallet| wallet.pubkey.clone()),
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

        let valid_keys: BTreeSet<String> =
            self.gpu_devices.iter().map(device_selection_key).collect();
        self.selected_gpu_keys
            .retain(|key| valid_keys.contains(key));
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
            .filter(|device| {
                self.selected_gpu_keys
                    .contains(&device_selection_key(device))
            })
            .collect()
    }

    fn start_mining(&mut self) {
        if self.mining_handle.is_some() {
            self.error = Some("Mining is already running.".to_string());
            return;
        }

        if !self.ensure_active_wallet() {
            return;
        }

        let Some(wallet) = self.active_wallet.clone() else {
            self.error = Some("The desktop mining wallet is not ready yet.".to_string());
            return;
        };

        if self.seed_phrase_requires_ack && !self.seed_phrase_acknowledged {
            self.status = "Before you start mining, save your recovery phrase.".to_string();
            self.open_seed_phrase_for_wallet(true);
            return;
        }

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
            self.error =
                Some("Connect a wallet first to arm a desktop mining session.".to_string());
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
            self.error = Some(
                "No valid GPU tuning profiles were generated for the selected GPUs.".to_string(),
            );
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
            Some(miner) => format!(
                "Native miner started for connected wallet {} via session delegate.",
                miner
            ),
            None => format!("Native miner started for wallet {}.", wallet.pubkey),
        };
        self.error = None;
        Ok(())
    }

    fn build_runtime_options(
        &self,
        miner_override: Option<Pubkey>,
    ) -> Result<MiningRuntimeOptions> {
        let selected_gpu_devices: Vec<GpuDeviceSelection> = self
            .selected_gpu_devices()
            .into_iter()
            .map(|device| GpuDeviceSelection {
                platform_index: device.platform_index,
                device_index: device.device_index,
            })
            .collect();
        if matches!(self.backend, BackendMode::Gpu | BackendMode::Both)
            && selected_gpu_devices.is_empty()
        {
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
            gpu_platform: fallback_device
                .map(|device| device.platform_index)
                .unwrap_or(0),
            gpu_device: fallback_device
                .map(|device| device.device_index)
                .unwrap_or(0),
            gpu_local_work_size: parse_optional_usize_field(
                &self.gpu_local_work_size,
                "GPU local work size",
            )?,
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
                        && completion.delegate_pubkey.to_string()
                            == pending.delegate_wallet.pubkey =>
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
                    self.error = Some(
                        "Wallet bridge response did not match the pending session.".to_string(),
                    );
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

        let current_block_result = self
            .current_block_receiver
            .as_ref()
            .and_then(|receiver| receiver.try_recv().ok());
        if let Some(result) = current_block_result {
            self.current_block_receiver = None;
            match result {
                Ok((block_number, difficulty_bits, current_reward)) => {
                    self.latest_snapshot.current_block_number = block_number;
                    self.latest_snapshot.difficulty_bits = difficulty_bits;
                    self.latest_snapshot.current_reward = current_reward;
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
                            self.status =
                                "GPU auto-tune completed. Best settings applied.".to_string();
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
        self.mouse_particle_field
            .tick(screen_rect, pointer, stable_dt);
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
        if self.current_block_receiver.is_none()
            && self.last_current_block_refresh_at.elapsed() >= Duration::from_secs(6)
        {
            self.start_current_block_refresh();
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
                    render_brand_header(
                        ui,
                        self.logo_circle_texture.as_ref(),
                        self.logo_wordmark_texture.as_ref(),
                    );
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
                        render_wallet_card(&mut columns[0], self);
                        columns[0].add_space(14.0);
                        render_miner_controls_card(&mut columns[0], self);

                        render_hashrate_signal_card(&mut columns[1], self);
                        columns[1].add_space(14.0);
                        render_live_telemetry_card(&mut columns[1], self);
                    });
                });
        });

        if self.show_deposit_modal {
            let mut open = true;
            let modal_size = match (self.deposit_modal_step, self.deposit_method) {
                (DepositModalStep::Picker, _) => egui::vec2(620.0, 240.0),
                (DepositModalStep::Details, DepositMethod::Web3Wallet) => egui::vec2(620.0, 290.0),
                (DepositModalStep::Details, DepositMethod::ManualSend) => egui::vec2(760.0, 360.0),
            };
            egui::Window::new("Fund desktop wallet")
                .collapsible(false)
                .resizable(false)
                .fixed_size(modal_size)
                .open(&mut open)
                .show(ctx, |ui| {
                    let wallet_address = self
                        .active_wallet
                        .as_ref()
                        .map(|wallet| wallet.pubkey.clone())
                        .unwrap_or_else(|| "Not ready yet".to_string());
                    let suggested_lamports = sol_to_lamports(self.web3_deposit_sol.max(0.1));
                    let mined_blocks_target = (suggested_lamports / TREASURY_FEE_PER_BLOCK_LAMPORTS).max(1);
                    match self.deposit_modal_step {
                        DepositModalStep::Picker => {
                            ui.label(
                                RichText::new("Choose how you want to fund the desktop miner.")
                                    .color(theme_muted()),
                            );
                            ui.add_space(14.0);
                            ui.columns(2, |columns| {
                                if action_choice_card(
                                    &mut columns[0],
                                    "Web3 Wallet",
                                    "Open the browser bridge and approve the transfer from your wallet.",
                                    true,
                                ) {
                                    self.deposit_method = DepositMethod::Web3Wallet;
                                    self.deposit_modal_step = DepositModalStep::Details;
                                }
                                if action_choice_card(
                                    &mut columns[1],
                                    "Manual Deposit",
                                    "Copy the desktop wallet address and send SOL from any external wallet.",
                                    false,
                                ) {
                                    self.deposit_method = DepositMethod::ManualSend;
                                    self.deposit_modal_step = DepositModalStep::Details;
                                }
                            });
                        }
                        DepositModalStep::Details => {
                            egui::Frame::group(ui.style())
                                .fill(theme_card_alt())
                                .stroke(egui::Stroke::new(1.0, theme_border()))
                                .rounding(egui::Rounding::same(18.0))
                                .inner_margin(egui::Margin::same(18.0))
                                .show(ui, |ui| {
                                    if matches!(self.deposit_method, DepositMethod::Web3Wallet) {
                                        ui.label(
                                            RichText::new(
                                                "Open the browser bridge and approve the transfer from your wallet.",
                                            )
                                            .color(theme_muted()),
                                        );
                                        ui.add_space(10.0);
                                        ui.label(RichText::new("Select amount").color(theme_accent()));
                                        ui.add(
                                            egui::Slider::new(&mut self.web3_deposit_sol, 0.1..=100.0)
                                                .suffix(" SOL")
                                                .logarithmic(false)
                                                .clamp_to_range(true),
                                        );
                                        ui.label(
                                            RichText::new(format!(
                                                "{} SOL keeps roughly {} blocks funded.",
                                                format_sol_compact(suggested_lamports),
                                                mined_blocks_target
                                            ))
                                            .color(theme_muted()),
                                        );
                                        ui.add_space(10.0);
                                        ui.horizontal(|ui| {
                                            if ui
                                                .add(
                                                    egui::Button::new(
                                                        RichText::new("Open Web3 Wallet")
                                                            .color(theme_button_text()),
                                                    )
                                                    .fill(theme_accent())
                                                    .min_size(egui::vec2(240.0, 42.0)),
                                                )
                                                .clicked()
                                            {
                                                self.open_web3_deposit_flow();
                                            }
                                            if ui
                                                .add(
                                                    egui::Button::new(
                                                        RichText::new("Back")
                                                            .color(theme_text())
                                                            .size(14.0),
                                                    )
                                                    .fill(theme_card())
                                                    .min_size(egui::vec2(240.0, 42.0)),
                                                )
                                                .clicked()
                                            {
                                                self.deposit_modal_step = DepositModalStep::Picker;
                                            }
                                        });
                                    } else {
                                        let qr_payload = format!("solana:{wallet_address}");
                                        ui.horizontal_top(|ui| {
                                            ui.vertical(|ui| {
                                                ui.label(
                                                    RichText::new(
                                                        "Copy the desktop wallet address or scan the QR code to send SOL from your phone.",
                                                    )
                                                    .color(theme_muted()),
                                                );
                                                ui.add_space(10.0);
                                                ui.label(
                                                    RichText::new(wallet_address.clone())
                                                        .monospace()
                                                        .color(theme_text()),
                                                );
                                                ui.add_space(8.0);
                                                ui.label(
                                                    RichText::new(format!(
                                                        "Suggested funding: {} SOL for about {} blocks.",
                                                        format_sol_compact(suggested_lamports),
                                                        mined_blocks_target
                                                    ))
                                                    .color(theme_muted()),
                                                );
                                                ui.add_space(12.0);
                                                ui.horizontal_wrapped(|ui| {
                                                    if ui
                                                        .add(
                                                            egui::Button::new(
                                                                RichText::new("Copy wallet address")
                                                                    .color(theme_button_text()),
                                                            )
                                                            .fill(theme_accent())
                                                            .min_size(egui::vec2(240.0, 42.0)),
                                                        )
                                                        .clicked()
                                                    {
                                                        ui.ctx().copy_text(wallet_address.clone());
                                                        self.status = "Desktop wallet address copied. Send SOL to this address, then come back to the miner.".to_string();
                                                        self.error = None;
                                                    }
                                                    if ui
                                                        .add(
                                                            egui::Button::new(
                                                                RichText::new("Back")
                                                                    .color(theme_text())
                                                                    .size(14.0),
                                                            )
                                                            .fill(theme_card())
                                                            .min_size(egui::vec2(180.0, 42.0)),
                                                        )
                                                        .clicked()
                                                    {
                                                        self.deposit_modal_step = DepositModalStep::Picker;
                                                    }
                                                });
                                            });
                                            ui.add_space(18.0);
                                            egui::Frame::group(ui.style())
                                                .fill(Color32::WHITE)
                                                .stroke(egui::Stroke::new(1.0, theme_border()))
                                                .rounding(egui::Rounding::same(20.0))
                                                .inner_margin(egui::Margin::same(14.0))
                                                .show(ui, |ui| {
                                                    render_qr_code(ui, &qr_payload, 190.0);
                                                });
                                        });
                                    }
                                });
                        }
                    }
                });
            self.show_deposit_modal = self.show_deposit_modal && open;
        }

        if self.show_seed_phrase_warning_modal {
            let mut open = true;
            egui::Window::new("Recovery phrase")
                .collapsible(false)
                .resizable(false)
                .default_width(480.0)
                .open(&mut open)
                .show(ctx, |ui| {
                    ui.label(
                        RichText::new(
                            "Do not share these words. Anyone with the recovery phrase controls the funds in this wallet. Make sure you are not on a screen share or recording before continuing."
                        )
                        .color(theme_muted()),
                    );
                    ui.add_space(14.0);
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new("I understand, reveal!")
                                    .color(theme_button_text()),
                            )
                            .fill(theme_accent())
                            .min_size(egui::vec2(220.0, 38.0)),
                        )
                        .clicked()
                    {
                        self.show_seed_phrase_warning_modal = false;
                        self.open_seed_phrase_for_wallet(false);
                    }
                });
            self.show_seed_phrase_warning_modal = self.show_seed_phrase_warning_modal && open;
        }

        if self.show_seed_phrase_modal {
            let mut open = true;
            egui::Window::new("Recovery phrase")
                .collapsible(false)
                .resizable(false)
                .default_width(620.0)
                .open(&mut open)
                .show(ctx, |ui| {
                    ui.label(
                        RichText::new(
                            "These 12 words recover the desktop mining wallet. Store them offline and keep them private."
                        )
                        .color(theme_muted()),
                    );
                    ui.add_space(12.0);
                    egui::Frame::group(ui.style())
                        .fill(theme_card_alt())
                        .stroke(egui::Stroke::new(1.0, theme_border()))
                        .rounding(egui::Rounding::same(18.0))
                        .inner_margin(egui::Margin::same(16.0))
                        .show(ui, |ui| {
                            egui::Grid::new("seed_phrase_grid")
                                .num_columns(2)
                                .spacing(egui::vec2(14.0, 12.0))
                                .show(ui, |ui| {
                                    for (index, word) in self.seed_phrase_words.iter().enumerate() {
                                        ui.label(
                                            RichText::new(format!("{:02}. {}", index + 1, word))
                                                .monospace()
                                                .color(theme_text())
                                                .size(18.0),
                                        );
                                        if index % 2 == 1 {
                                            ui.end_row();
                                        }
                                    }
                                });
                        });
                    ui.add_space(12.0);
                    ui.horizontal_wrapped(|ui| {
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("Copy phrase").color(theme_button_text()),
                                )
                                .fill(theme_accent())
                                .min_size(egui::vec2(160.0, 38.0)),
                            )
                            .clicked()
                        {
                            ui.ctx().copy_text(self.seed_phrase_words.join(" "));
                            self.status =
                                "Recovery phrase copied. Store it somewhere safe and offline."
                                    .to_string();
                            self.error = None;
                        }
                        if self.seed_phrase_requires_ack {
                            if ui
                                .add(
                                    egui::Button::new("I saved the recovery phrase")
                                        .min_size(egui::vec2(220.0, 38.0)),
                                )
                                .clicked()
                            {
                                self.seed_phrase_acknowledged = true;
                                self.show_seed_phrase_modal = false;
                            }
                        } else if ui.button("Close").clicked() {
                            self.show_seed_phrase_modal = false;
                        }
                    });
                });
            if self.seed_phrase_requires_ack && !self.seed_phrase_acknowledged && !open {
                open = true;
            }
            self.show_seed_phrase_modal = self.show_seed_phrase_modal
                && open
                && !(self.seed_phrase_requires_ack && self.seed_phrase_acknowledged);
        }

        if self.show_add_wallet_modal {
            let mut open = true;
            let mut wallet_to_select: Option<ManagedWallet> = None;
            let mut create_wallet = false;
            let mut import_wallet = false;

            egui::Window::new("Manage wallets")
                .collapsible(false)
                .resizable(false)
                .default_width(760.0)
                .open(&mut open)
                .show(ctx, |ui| {
                    ui.label(
                        RichText::new(
                            "Choose which local wallet the desktop miner should use, create a fresh one, or import an existing wallet.",
                        )
                        .color(theme_muted()),
                    );
                    if self.mining_handle.is_some() {
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new(
                                "Stop mining before switching, creating, or importing wallets.",
                            )
                            .color(theme_accent()),
                        );
                    }

                    ui.add_space(14.0);
                    egui::Frame::group(ui.style())
                        .fill(theme_card_alt())
                        .stroke(egui::Stroke::new(1.0, theme_border()))
                        .rounding(egui::Rounding::same(18.0))
                        .inner_margin(egui::Margin::same(16.0))
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new("Available wallets")
                                    .size(13.0)
                                    .color(theme_accent()),
                            );
                            ui.add_space(10.0);

                            if self.available_wallets.is_empty() {
                                ui.label(
                                    RichText::new("No local wallets found yet. Create one below.")
                                        .color(theme_muted()),
                                );
                            }

                            for wallet in self.available_wallets.clone() {
                                let selected = self
                                    .active_wallet
                                    .as_ref()
                                    .map(|active| active.pubkey == wallet.pubkey)
                                    .unwrap_or(false);

                                egui::Frame::group(ui.style())
                                    .fill(theme_card())
                                    .stroke(egui::Stroke::new(
                                        1.0,
                                        if selected {
                                            theme_accent_soft()
                                        } else {
                                            theme_border()
                                        },
                                    ))
                                    .rounding(egui::Rounding::same(16.0))
                                    .inner_margin(egui::Margin::same(14.0))
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            ui.vertical(|ui| {
                                                ui.label(
                                                    RichText::new(format_wallet_source_label(&wallet))
                                                        .strong()
                                                        .color(theme_text()),
                                                );
                                                ui.add_space(4.0);
                                                ui.label(
                                                    RichText::new(wallet.pubkey.clone())
                                                        .monospace()
                                                        .color(theme_muted()),
                                                );
                                            });
                                            ui.with_layout(
                                                egui::Layout::right_to_left(Align::Center),
                                                |ui| {
                                                    if selected {
                                                        ui.label(
                                                            RichText::new("Selected")
                                                                .color(theme_accent())
                                                                .strong(),
                                                        );
                                                    } else if ui
                                                        .add_enabled(
                                                            self.mining_handle.is_none(),
                                                            egui::Button::new("Use wallet")
                                                                .min_size(egui::vec2(108.0, 34.0)),
                                                        )
                                                        .clicked()
                                                    {
                                                        wallet_to_select = Some(wallet.clone());
                                                    }
                                                },
                                            );
                                        });
                                    });
                                ui.add_space(8.0);
                            }
                        });

                    ui.add_space(14.0);
                    ui.columns(2, |columns| {
                        egui::Frame::group(columns[0].style())
                            .fill(theme_card_alt())
                            .stroke(egui::Stroke::new(1.0, theme_border()))
                            .rounding(egui::Rounding::same(18.0))
                            .inner_margin(egui::Margin::same(16.0))
                            .show(&mut columns[0], |ui| {
                                ui.label(
                                    RichText::new("Create new")
                                        .size(13.0)
                                        .color(theme_accent()),
                                );
                                ui.add_space(8.0);
                                ui.label(
                                    RichText::new(
                                        "Generate a fresh local wallet for this machine and store its recovery phrase.",
                                    )
                                    .color(theme_muted()),
                                );
                                ui.add_space(10.0);
                                ui.label(RichText::new("Wallet label").color(theme_muted()));
                                ui.add(
                                    TextEdit::singleline(&mut self.add_wallet_label)
                                        .desired_width(f32::INFINITY)
                                        .hint_text("mining-rig-a"),
                                );
                                ui.add_space(14.0);
                                if ui
                                    .add_enabled(
                                        self.mining_handle.is_none(),
                                        egui::Button::new(
                                            RichText::new("+  Create new")
                                                .color(theme_button_text()),
                                        )
                                        .fill(theme_accent())
                                        .min_size(egui::vec2(180.0, 40.0)),
                                    )
                                    .clicked()
                                {
                                    create_wallet = true;
                                }
                            });

                        egui::Frame::group(columns[1].style())
                            .fill(theme_card_alt())
                            .stroke(egui::Stroke::new(1.0, theme_border()))
                            .rounding(egui::Rounding::same(18.0))
                            .inner_margin(egui::Margin::same(16.0))
                            .show(&mut columns[1], |ui| {
                                ui.label(
                                    RichText::new("Import existing")
                                        .size(13.0)
                                        .color(theme_accent()),
                                );
                                ui.add_space(8.0);
                                ui.horizontal_wrapped(|ui| {
                                    ui.selectable_value(
                                        &mut self.add_wallet_mode,
                                        AddWalletMode::SeedPhrase,
                                        "Seed phrase",
                                    );
                                    ui.selectable_value(
                                        &mut self.add_wallet_mode,
                                        AddWalletMode::PrivateKey,
                                        "Private key",
                                    );
                                });
                                ui.add_space(10.0);
                                ui.label(RichText::new("Wallet label").color(theme_muted()));
                                ui.add(
                                    TextEdit::singleline(&mut self.add_wallet_label)
                                        .desired_width(f32::INFINITY)
                                        .hint_text("my-wallet"),
                                );
                                ui.add_space(8.0);
                                ui.label(
                                    RichText::new(match self.add_wallet_mode {
                                        AddWalletMode::SeedPhrase => {
                                            "Seed phrase (12 or 24 words)"
                                        }
                                        AddWalletMode::PrivateKey => {
                                            "Private key (base58 or JSON array)"
                                        }
                                    })
                                    .color(theme_muted()),
                                );
                                ui.add(
                                    TextEdit::multiline(&mut self.add_wallet_secret)
                                        .desired_rows(4)
                                        .desired_width(f32::INFINITY)
                                        .hint_text(match self.add_wallet_mode {
                                            AddWalletMode::SeedPhrase => {
                                                "word1 word2 word3 ..."
                                            }
                                            AddWalletMode::PrivateKey => {
                                                "[12,34,...] or base58 secret"
                                            }
                                        }),
                                );
                                ui.add_space(12.0);
                                if ui
                                    .add_enabled(
                                        self.mining_handle.is_none(),
                                        egui::Button::new(
                                            RichText::new("Import existing")
                                                .color(theme_button_text()),
                                        )
                                        .fill(theme_accent())
                                        .min_size(egui::vec2(180.0, 40.0)),
                                    )
                                    .clicked()
                                {
                                    import_wallet = true;
                                }
                            });
                    });
                });

            self.show_add_wallet_modal = self.show_add_wallet_modal && open;

            if let Some(wallet) = wallet_to_select {
                self.select_active_wallet(wallet);
            }
            if create_wallet {
                self.create_managed_wallet();
            }
            if import_wallet {
                self.import_managed_wallet();
            }
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
            let modal_size = egui::vec2(
                (screen_rect.width() - 100.0).clamp(1080.0, 1460.0),
                (screen_rect.height() - 220.0).clamp(560.0, 700.0),
            );
            let current_block_number = displayed_current_block_number(&self.latest_snapshot);
            let current_era = reward_era_for_block(current_block_number);
            let mut modal_rect = None;
            if let Some(window) = egui::Window::new("Mining Curve")
                .collapsible(false)
                .resizable(false)
                .fixed_size(modal_size)
                .open(&mut open)
                .show(ctx, |ui| {
                    ui.label(
                        RichText::new(
                            "Blockmine follows a fixed era schedule. Read the curve below to see where emissions are now and how the supply decays over time.",
                        )
                        .color(theme_muted()),
                    );
                    ui.add_space(10.0);
                    egui::Frame::group(ui.style())
                        .fill(theme_card_alt())
                        .stroke(egui::Stroke::new(1.0, theme_border()))
                        .rounding(egui::Rounding::same(16.0))
                        .inner_margin(egui::Margin::same(14.0))
                        .show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(RichText::new("Current era").color(theme_muted()));
                                ui.label(
                                    RichText::new(decode_era_name(current_era.name))
                                        .strong()
                                        .color(theme_accent()),
                                );
                                ui.separator();
                                ui.label(RichText::new("Current block").color(theme_muted()));
                                ui.label(
                                    RichText::new(format!("#{}", current_block_number))
                                        .strong()
                                        .color(theme_text()),
                                );
                            });
                        });
                    ui.add_space(10.0);
                    egui::Frame::group(ui.style())
                        .fill(theme_card_alt())
                        .stroke(egui::Stroke::new(1.0, theme_border()))
                        .show(ui, |ui| {
                            ui.label(RichText::new("Mining Curve").strong().color(theme_accent()));
                            ui.add_space(10.0);
                            egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
                                egui::Grid::new("era_schedule_grid")
                                    .striped(false)
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
                                            let is_current = row.era == current_era.index;
                                            let mined_progress = format_era_progress(row, current_block_number);
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
                })
            {
                modal_rect = Some(window.response.rect);
            }
            if let (Some(rect), Some(pointer)) =
                (modal_rect, ctx.input(|input| input.pointer.interact_pos()))
            {
                if ctx.input(|input| input.pointer.any_pressed()) && !rect.contains(pointer) {
                    open = false;
                }
            }
            self.show_era_schedule_modal = self.show_era_schedule_modal && open;
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {}
}

fn split_seed_phrase_words(phrase: &str) -> Vec<String> {
    phrase
        .split_whitespace()
        .map(|word| word.trim().to_string())
        .filter(|word| !word.is_empty())
        .collect()
}

fn detect_cpu_model() -> String {
    let mut system = System::new_all();
    system.refresh_cpu_all();
    system
        .cpus()
        .first()
        .map(|cpu| cpu.brand().trim().to_string())
        .filter(|label| !label.is_empty())
        .unwrap_or_else(|| "Desktop CPU".to_string())
}

fn sol_to_lamports(sol: f32) -> u64 {
    (sol.max(0.0) as f64 * 1_000_000_000.0).round() as u64
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

    match (
        summary.total_sent_lamports > 0,
        summary.total_sent_bloc_raw > 0,
    ) {
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
    let mut text = RichText::new(value.into()).color(if highlight {
        theme_accent()
    } else {
        theme_text()
    });
    if monospace {
        text = text.family(egui::FontFamily::Monospace);
    }
    if highlight {
        text = text.strong();
    }
    ui.label(text);
}

fn load_desktop_ui_preferences() -> Result<Option<DesktopUiPreferences>> {
    let path = desktop_ui_preferences_path()?;
    if !path.exists() {
        return Ok(None);
    }

    let raw =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
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
    raw.trim_end_matches('0').trim_end_matches('.').to_string()
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
                    egui::Stroke::new(1.0, Color32::from_rgba_premultiplied(232, 137, 48, alpha)),
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
    let widget_rounding = egui::Rounding::same(12.0);
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
    visuals.widgets.noninteractive.rounding = widget_rounding;
    visuals.widgets.inactive.bg_fill = theme_card_alt();
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, theme_border());
    visuals.widgets.inactive.rounding = widget_rounding;
    visuals.widgets.hovered.bg_fill = Color32::from_rgba_premultiplied(44, 44, 52, 228);
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, theme_accent_soft());
    visuals.widgets.hovered.rounding = widget_rounding;
    visuals.widgets.active.bg_fill = Color32::from_rgba_premultiplied(54, 54, 64, 236);
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, theme_accent());
    visuals.widgets.active.rounding = widget_rounding;
    visuals.window_rounding = egui::Rounding::same(18.0);
    ctx.set_visuals(visuals);
}

fn load_embedded_texture(ctx: &egui::Context, name: &str, bytes: &[u8]) -> Result<TextureHandle> {
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

fn load_embedded_gif(ctx: &egui::Context, name: &str, bytes: &[u8]) -> Result<AnimatedTexture> {
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
            ui.add(egui::Image::new(texture).fit_to_exact_size(egui::vec2(52.0, 52.0)));
        }

        if let Some(texture) = wordmark_texture {
            let size = texture.size_vec2();
            let target_height = 44.0;
            let target_width = (size.x / size.y.max(1.0)) * target_height;
            ui.add(
                egui::Image::new(texture)
                    .fit_to_exact_size(egui::vec2(target_width, target_height)),
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
        egui::Image::new(&frame.texture).fit_to_exact_size(egui::vec2(target_width, target_height)),
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
    let end = name
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(name.len());
    String::from_utf8_lossy(&name[..end]).into_owned()
}

fn render_wallet_card(ui: &mut egui::Ui, app: &mut BlockMineStudioApp) {
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
    let session_blocks_mineable = total_session_balance_lamports / TREASURY_FEE_PER_BLOCK_LAMPORTS;

    let rounding = egui::Rounding::same(18.0);
    let response = egui::Frame::group(ui.style())
        .fill(theme_card())
        .stroke(egui::Stroke::new(1.0, theme_border()))
        .rounding(rounding)
        .inner_margin(egui::Margin::same(16.0))
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.heading(RichText::new("Wallet").color(theme_text()));
                ui.with_layout(egui::Layout::right_to_left(Align::Min), |ui| {
                    if ui
                        .add(
                            egui::Button::new(RichText::new("Recovery").size(12.0))
                                .min_size(egui::vec2(92.0, 26.0)),
                        )
                        .clicked()
                    {
                        app.show_seed_phrase_warning_modal = true;
                    }
                    if ui
                        .add(
                            egui::Button::new(RichText::new("Manage wallets").size(12.0))
                                .min_size(egui::vec2(128.0, 26.0)),
                        )
                        .clicked()
                    {
                        app.show_add_wallet_modal = true;
                    }
                });
            });
            ui.add_space(10.0);
            ui.label(
                RichText::new(
                    "Use one local wallet at a time for the desktop miner. Manage wallets here, keep a little SOL inside, and the rig can keep mined blocks flowing without interruption.",
                )
                .color(theme_muted()),
            );
        ui.add_space(8.0);
        if app.seed_phrase_requires_ack && !app.seed_phrase_acknowledged {
            egui::Frame::group(ui.style())
                .fill(theme_card_alt())
                .stroke(egui::Stroke::new(1.0, theme_accent_soft()))
                .rounding(egui::Rounding::same(18.0))
                .inner_margin(egui::Margin::same(14.0))
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new("Before you start mining, save your recovery phrase.")
                                .color(theme_text())
                                .strong(),
                        );
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("Reveal").color(theme_button_text()),
                                )
                                .fill(theme_accent())
                                .min_size(egui::vec2(110.0, 34.0)),
                            )
                            .clicked()
                        {
                            app.open_seed_phrase_for_wallet(true);
                        }
                    });
                });
            ui.add_space(10.0);
        }
        egui::Frame::group(ui.style())
            .fill(theme_card_alt())
            .stroke(egui::Stroke::new(1.0, theme_border()))
            .rounding(egui::Rounding::same(18.0))
            .inner_margin(egui::Margin::same(16.0))
            .show(ui, |ui| {
                labeled_value(ui, "Desktop wallet", desktop_wallet_address);
                if let Some(wallet) = app.active_wallet.as_ref() {
                    labeled_value(ui, "Wallet type", format_wallet_source_label(wallet));
                }
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
                    format!("{session_blocks_mineable} blocks"),
                );
            });

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
    paint_frame_glow(ui, response.response.rect, rounding);
}

fn render_miner_controls_card(ui: &mut egui::Ui, app: &mut BlockMineStudioApp) {
    card_frame(ui, "Miner controls", |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(
                &mut app.miner_controls_mode,
                MinerControlsMode::Fast,
                "Fast",
            );
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
                    RichText::new("Pick one engine, check the hardware label, then start mining.")
                        .color(theme_muted()),
                );
                ui.add_space(10.0);
                ui.columns(2, |columns| {
                    if render_fast_mode_choice(
                        &mut columns[0],
                        "CPU",
                        &app.cpu_model_label,
                        app.fast_mining_choice == FastMiningChoice::Cpu,
                    ) {
                        app.fast_mining_choice = FastMiningChoice::Cpu;
                        app.backend = BackendMode::Cpu;
                    }
                    if render_fast_mode_choice(
                        &mut columns[1],
                        "GPU",
                        &selected_gpu_summary(app),
                        app.fast_mining_choice == FastMiningChoice::Gpu,
                    ) {
                        app.fast_mining_choice = FastMiningChoice::Gpu;
                        app.backend = BackendMode::Gpu;
                    }
                });
                ui.add_space(12.0);
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .add_enabled(
                            app.mining_handle.is_none(),
                            egui::Button::new(
                                RichText::new("Start mining").color(theme_button_text()),
                            )
                            .fill(theme_accent())
                            .min_size(egui::vec2(220.0, 46.0)),
                        )
                        .clicked()
                    {
                        match app.fast_mining_choice {
                            FastMiningChoice::Cpu => app.start_cpu_mining(),
                            FastMiningChoice::Gpu => app.start_gpu_mining(),
                        }
                    }
                    if ui
                        .add_enabled(
                            app.mining_handle.is_some(),
                            egui::Button::new("Stop mining").min_size(egui::vec2(160.0, 46.0)),
                        )
                        .clicked()
                    {
                        app.stop_mining();
                    }
                });
            }
            MinerControlsMode::Advanced => {
                egui::Frame::group(ui.style())
                    .fill(theme_card_alt())
                    .stroke(egui::Stroke::new(1.0, theme_border()))
                    .rounding(egui::Rounding::same(16.0))
                    .inner_margin(egui::Margin::same(14.0))
                    .show(ui, |ui| {
                        ui.label(RichText::new("RPC endpoint").color(theme_accent()));
                        ui.add_space(6.0);
                        ui.add(
                            TextEdit::singleline(&mut app.rpc_url)
                                .desired_width(ui.available_width()),
                        );
                    });
                ui.add_space(12.0);

                ui.horizontal_wrapped(|ui| {
                    ui.selectable_value(&mut app.backend, BackendMode::Cpu, "CPU tuning");
                    ui.selectable_value(&mut app.backend, BackendMode::Gpu, "GPU tuning");
                });

                ui.add_space(8.0);
                match app.backend {
                    BackendMode::Cpu => {
                        ui.label(
                            RichText::new(
                                "Tune batch size, worker count and pinned cores for CPU-only rigs.",
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
                                "Pick the cards you want, then fine-tune batch and work size only if you really need to.",
                            )
                            .color(theme_muted()),
                        );
                        ui.add_space(10.0);
                        grid_gpu_primary_fields(
                            ui,
                            &mut app.gpu_batch_size,
                            &mut app.gpu_local_work_size,
                        );
                        ui.add_space(10.0);
                        render_gpu_picker(ui, app, true);
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
    card_frame(ui, "Protocol telemetry", |ui| {
        let current_block_number = displayed_current_block_number(&app.latest_snapshot);
        let current_era = reward_era_for_block(current_block_number);
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
                    egui::Button::new(RichText::new("i").color(theme_text()).size(12.0).strong())
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
            metric(
                &mut cols[0],
                "Current block",
                format!("#{}", current_block_number),
            );
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
                "Blocks mined",
                app.latest_snapshot.session_blocks_mined.to_string(),
            );
        });
    });
}

fn render_hashrate_signal_card(ui: &mut egui::Ui, app: &BlockMineStudioApp) {
    card_frame(ui, "Mining stats", |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new("Live desktop miner output:")
                    .size(24.0)
                    .color(Color32::WHITE),
            );
        });
        ui.add_space(6.0);
        ui.label(
            RichText::new(
                "Real-time output from your rig. Hover the curve to inspect each 30-second peak.",
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
                "Blocks mined",
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
                let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::hover());
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
                        points.clone(),
                        egui::Stroke::new(3.5, theme_accent()),
                    ));
                    if response.hovered() {
                        if let Some(pointer) = response.hover_pos() {
                            if let Some((index, point)) =
                                points.iter().enumerate().min_by(|(_, left), (_, right)| {
                                    (left.x - pointer.x)
                                        .abs()
                                        .partial_cmp(&(right.x - pointer.x).abs())
                                        .unwrap_or(std::cmp::Ordering::Equal)
                                })
                            {
                                painter.line_segment(
                                    [
                                        egui::pos2(point.x, rect.top()),
                                        egui::pos2(point.x, rect.bottom()),
                                    ],
                                    egui::Stroke::new(1.0, theme_accent_soft()),
                                );
                                painter.circle_filled(*point, 5.0, theme_accent());
                                egui::show_tooltip_at_pointer(
                                    ui.ctx(),
                                    egui::Id::new("hashrate_chart_point_tooltip"),
                                    |ui| {
                                        ui.label(
                                            RichText::new(format_hashrate_compact(series[index]))
                                                .strong()
                                                .color(theme_text()),
                                        );
                                        ui.label(
                                            RichText::new("30-second peak").color(theme_muted()),
                                        );
                                    },
                                );
                            }
                        }
                    }
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
                format_hashrate_compact(app.hashrate_chart.chart_average(app.display_hashrate_hps)),
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
                    RichText::new("Start mining with this wallet").color(theme_button_text()),
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
            .selectable_label(
                auto_selected,
                format!("Auto (all {} cores)", app.available_cpu_cores.len()),
            )
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
            cores
                .into_iter()
                .map(|core| core.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_else(|| "all logical cores".to_string());
    ui.add_space(6.0);
    ui.label(RichText::new(format!("Pinned CPUs: {selected_summary}")).color(theme_muted()));
}

fn render_gpu_picker(ui: &mut egui::Ui, app: &mut BlockMineStudioApp, show_autotune: bool) {
    ui.label(
        RichText::new("GPU selection")
            .size(14.0)
            .color(theme_accent()),
    );
    ui.label(RichText::new("Pick the cards you want to use for mining.").color(theme_muted()));
    ui.add_space(8.0);

    ui.horizontal_wrapped(|ui| {
        if ui.button("Refresh GPU list").clicked() {
            app.refresh_gpu_devices();
        }
        if ui.button("Select all GPUs").clicked() {
            app.selected_gpu_keys = app.gpu_devices.iter().map(device_selection_key).collect();
            app.sync_selected_gpu_selection();
            app.persist_ui_preferences();
        }
        if ui.button("Clear").clicked() {
            app.selected_gpu_keys.clear();
            app.persist_ui_preferences();
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
                if ui
                    .checkbox(&mut selected, format_gpu_device_label(&device))
                    .changed()
                {
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
    if show_autotune {
        ui.add_space(10.0);
        let autotune_live = app.gpu_autotune_receiver.is_some();
        if ui
            .add_enabled(
                !autotune_live && app.mining_handle.is_none(),
                egui::Button::new(
                    RichText::new("Auto-tune selected GPUs").color(theme_button_text()),
                )
                .fill(theme_accent())
                .min_size(egui::vec2(210.0, 38.0)),
            )
            .clicked()
        {
            app.start_gpu_autotune();
        }
        ui.add_space(6.0);
        if autotune_live {
            ui.label(
                RichText::new("Testing shared GPU profiles. This usually takes 20 to 40 seconds.")
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

fn grid_gpu_primary_fields(
    ui: &mut egui::Ui,
    gpu_batch_size: &mut String,
    gpu_local_work_size: &mut String,
) {
    egui::Grid::new("mining_fields_gpu_primary")
        .num_columns(2)
        .spacing([12.0, 10.0])
        .show(ui, |ui| {
            ui.label("GPU batch size");
            ui.add(TextEdit::singleline(gpu_batch_size).desired_width(120.0));
            ui.end_row();
            ui.label("GPU local work size");
            ui.add(
                TextEdit::singleline(gpu_local_work_size)
                    .desired_width(120.0)
                    .hint_text("Optional"),
            );
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
    device.device_name.clone()
}

fn selected_gpu_summary(app: &BlockMineStudioApp) -> String {
    let selected = app.selected_gpu_devices();
    if selected.is_empty() {
        "No GPU selected yet".to_string()
    } else if selected.len() == 1 {
        selected[0].device_name.clone()
    } else {
        format!("{} GPUs selected", selected.len())
    }
}

fn action_choice_card(ui: &mut egui::Ui, title: &str, caption: &str, primary: bool) -> bool {
    let rounding = egui::Rounding::same(20.0);
    let fill = if primary {
        Color32::from_rgba_premultiplied(45, 33, 22, 248)
    } else {
        theme_card_alt()
    };
    let stroke = if primary {
        egui::Stroke::new(1.2, theme_accent())
    } else {
        egui::Stroke::new(1.0, theme_border())
    };
    let response = egui::Frame::group(ui.style())
        .fill(fill)
        .stroke(stroke)
        .rounding(rounding)
        .inner_margin(egui::Margin::same(18.0))
        .show(ui, |ui| {
            ui.set_min_height(126.0);
            ui.label(
                RichText::new(title)
                    .color(if primary {
                        theme_accent()
                    } else {
                        theme_text()
                    })
                    .strong()
                    .size(20.0),
            );
            ui.add_space(10.0);
            ui.label(RichText::new(caption).color(theme_muted()).size(15.0));
        });
    let click_response = ui.interact(
        response.response.rect,
        ui.id().with(("action_choice_card", title)),
        egui::Sense::click(),
    );
    paint_frame_glow(ui, response.response.rect, rounding);
    click_response.clicked()
}

fn render_fast_mode_choice(ui: &mut egui::Ui, label: &str, hardware: &str, selected: bool) -> bool {
    let rounding = egui::Rounding::same(18.0);
    let stroke = if selected {
        egui::Stroke::new(1.2, theme_accent())
    } else {
        egui::Stroke::new(1.0, theme_border())
    };
    let fill = if selected {
        Color32::from_rgba_premultiplied(41, 31, 24, 248)
    } else {
        theme_card_alt()
    };
    let response = egui::Frame::group(ui.style())
        .fill(fill)
        .stroke(stroke)
        .rounding(rounding)
        .inner_margin(egui::Margin::same(16.0))
        .show(ui, |ui| {
            ui.set_min_height(162.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(label)
                        .color(theme_accent())
                        .strong()
                        .size(16.0),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let pill_fill = if label.eq_ignore_ascii_case("gpu") {
                        theme_card()
                    } else {
                        theme_accent_soft()
                    };
                    let pill_text = if label.eq_ignore_ascii_case("gpu") {
                        theme_text()
                    } else {
                        theme_accent()
                    };
                    egui::Frame::none()
                        .fill(pill_fill)
                        .stroke(egui::Stroke::new(1.0, theme_border()))
                        .rounding(egui::Rounding::same(999.0))
                        .inner_margin(egui::Margin::symmetric(10.0, 4.0))
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new(label)
                                    .size(10.0)
                                    .strong()
                                    .color(pill_text),
                            );
                        });
                });
            });
            ui.add_space(10.0);
            render_hardware_architecture(ui, label);
            ui.add_space(10.0);
            ui.label(RichText::new(hardware).color(theme_text()).size(20.0));
        });
    let click_response = ui.interact(
        response.response.rect,
        ui.id().with(("fast_mode_choice", label)),
        egui::Sense::click(),
    );
    paint_frame_glow(ui, response.response.rect, rounding);
    click_response.clicked()
}

fn render_hardware_architecture(ui: &mut egui::Ui, label: &str) {
    let desired_height = 74.0;
    let desired_size = egui::vec2(ui.available_width(), desired_height);
    let (rect, _) = ui.allocate_exact_size(desired_size, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let cpu_mode = label.eq_ignore_ascii_case("cpu");
    let accent = if cpu_mode {
        theme_accent()
    } else {
        theme_text()
    };
    let accent_soft = if cpu_mode {
        theme_accent_soft()
    } else {
        Color32::from_rgba_premultiplied(211, 208, 202, 70)
    };
    let secondary = if cpu_mode {
        Color32::from_rgb(211, 208, 202)
    } else {
        theme_accent()
    };
    let stroke = egui::Stroke::new(1.0, Color32::from_rgba_premultiplied(255, 255, 255, 54));
    let to_screen = |x: f32, y: f32| {
        egui::pos2(
            egui::lerp(rect.left()..=rect.right(), x / 200.0),
            egui::lerp(rect.top()..=rect.bottom(), y / 100.0),
        )
    };
    let time = ui.input(|input| input.time) as f32;
    let tracks: [Vec<egui::Pos2>; 8] = [
        vec![to_screen(10.0, 20.0), to_screen(89.5, 20.0), to_screen(94.5, 25.0), to_screen(94.5, 55.0)],
        vec![to_screen(180.0, 10.0), to_screen(110.3, 10.0), to_screen(105.3, 15.0), to_screen(105.3, 45.0)],
        vec![to_screen(130.0, 20.0), to_screen(130.0, 41.8), to_screen(125.0, 46.8), to_screen(115.0, 46.8)],
        vec![to_screen(170.0, 80.0), to_screen(170.0, 58.2), to_screen(165.0, 53.2), to_screen(115.0, 53.2)],
        vec![to_screen(135.0, 65.0), to_screen(150.0, 65.0), to_screen(155.0, 70.0), to_screen(155.0, 80.0), to_screen(150.0, 85.0), to_screen(110.2, 85.0), to_screen(105.2, 80.0), to_screen(105.2, 60.0)],
        vec![to_screen(94.8, 95.0), to_screen(94.8, 59.0)],
        vec![to_screen(88.0, 88.0), to_screen(88.0, 73.0), to_screen(83.0, 68.0), to_screen(73.0, 68.0), to_screen(68.0, 63.0), to_screen(68.0, 58.0), to_screen(73.0, 53.0), to_screen(87.0, 53.0)],
        vec![to_screen(30.0, 30.0), to_screen(55.0, 30.0), to_screen(60.0, 35.0), to_screen(60.0, 41.5), to_screen(65.0, 46.5), to_screen(85.0, 46.5)],
    ];

    for track in &tracks {
        painter.add(egui::Shape::line(track.clone(), stroke));
    }

    let chip_rect = egui::Rect::from_min_size(to_screen(85.0, 40.0), egui::vec2(rect.width() * 0.15, rect.height() * 0.20));
    painter.rect_filled(chip_rect, egui::Rounding::same(4.0), Color32::from_rgb(17, 19, 24));
    painter.rect_stroke(
        chip_rect,
        egui::Rounding::same(4.0),
        egui::Stroke::new(1.0, Color32::from_rgba_premultiplied(255, 255, 255, 22)),
    );

    let pin_specs = [
        (93.0, 37.0, 2.5, 5.0),
        (104.0, 37.0, 2.5, 5.0),
        (104.0, 16.0, 2.5, 5.0),
        (114.5, 16.0, 2.5, 5.0),
    ];
    for (x, y, w, h) in pin_specs {
        painter.rect_filled(
            egui::Rect::from_min_size(to_screen(x, y), egui::vec2(rect.width() * (w / 200.0), rect.height() * (h / 100.0))),
            egui::Rounding::same(1.5),
            Color32::from_rgb(79, 79, 79),
        );
    }

    painter.text(
        chip_rect.center(),
        egui::Align2::CENTER_CENTER,
        label.to_uppercase(),
        egui::FontId::proportional(13.0),
        accent,
    );

    for (index, track) in tracks.iter().enumerate() {
        if track.len() < 2 {
            continue;
        }
        let pulse = ((time * (0.7 + index as f32 * 0.08)).fract()).clamp(0.0, 0.999);
        let pulse_pos = point_along_polyline(track, pulse);
        let radius = if cpu_mode { 5.5 } else { 5.0 };
        painter.circle_filled(pulse_pos, radius, accent.gamma_multiply(0.16));
        painter.circle_filled(pulse_pos, radius * 0.62, if index % 2 == 0 { accent } else { secondary });
    }

    let start_nodes = [
        to_screen(10.0, 20.0),
        to_screen(180.0, 10.0),
        to_screen(130.0, 20.0),
        to_screen(170.0, 80.0),
        to_screen(135.0, 65.0),
        to_screen(94.8, 95.0),
        to_screen(88.0, 88.0),
        to_screen(30.0, 30.0),
    ];
    for point in start_nodes {
        painter.circle_filled(point, 3.4, Color32::from_rgb(12, 13, 16));
        painter.circle_stroke(point, 3.4, egui::Stroke::new(1.0, accent_soft));
    }
}

fn point_along_polyline(points: &[egui::Pos2], t: f32) -> egui::Pos2 {
    if points.is_empty() {
        return egui::Pos2::ZERO;
    }
    if points.len() == 1 {
        return points[0];
    }

    let mut total = 0.0;
    let mut lengths = Vec::with_capacity(points.len().saturating_sub(1));
    for window in points.windows(2) {
        let length = window[0].distance(window[1]);
        lengths.push(length);
        total += length;
    }

    if total <= f32::EPSILON {
        return *points.last().unwrap_or(&points[0]);
    }

    let mut target = total * t.clamp(0.0, 1.0);
    for (index, length) in lengths.iter().enumerate() {
        if target <= *length {
            let start = points[index];
            let end = points[index + 1];
            let local_t = if *length <= f32::EPSILON {
                0.0
            } else {
                target / *length
            };
            return egui::pos2(
                egui::lerp(start.x..=end.x, local_t),
                egui::lerp(start.y..=end.y, local_t),
            );
        }
        target -= *length;
    }

    *points.last().unwrap_or(&points[0])
}

fn render_qr_code(ui: &mut egui::Ui, payload: &str, size: f32) {
    let Ok(code) = QrCode::new(payload.as_bytes()) else {
        ui.label(RichText::new("QR unavailable").color(theme_muted()));
        return;
    };

    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, egui::Rounding::same(16.0), Color32::WHITE);

    let width = code.width() as f32;
    let quiet_zone = 12.0;
    let drawable = (size - quiet_zone * 2.0).max(10.0);
    let module_size = (drawable / width).floor().max(1.0);
    let drawn_size = module_size * width;
    let offset_x = rect.left() + (size - drawn_size) * 0.5;
    let offset_y = rect.top() + (size - drawn_size) * 0.5;

    for y in 0..code.width() {
        for x in 0..code.width() {
            if code[(x, y)] != QrColor::Dark {
                continue;
            }

            let module_rect = egui::Rect::from_min_max(
                egui::pos2(
                    offset_x + x as f32 * module_size,
                    offset_y + y as f32 * module_size,
                ),
                egui::pos2(
                    offset_x + (x as f32 + 1.0) * module_size,
                    offset_y + (y as f32 + 1.0) * module_size,
                ),
            );
            painter.rect_filled(module_rect, egui::Rounding::ZERO, Color32::BLACK);
        }
    }
}

fn shorten_pubkey(value: &str) -> String {
    if value.len() <= 12 {
        value.to_string()
    } else {
        format!("{}...{}", &value[..4], &value[value.len().saturating_sub(4)..])
    }
}

fn format_wallet_source_label(wallet: &ManagedWallet) -> &'static str {
    match wallet.source {
        WalletSource::DedicatedGenerated => "Dedicated wallet",
        WalletSource::SessionDelegate => "Desktop wallet",
        WalletSource::ImportedFile => "Imported keypair",
        WalletSource::ImportedSecret => "Imported private key",
        WalletSource::ImportedSeedPhrase => "Imported seed phrase",
    }
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

fn displayed_current_block_number(snapshot: &MiningSnapshot) -> u64 {
    snapshot.current_block_number
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
    Err(anyhow::anyhow!(
        "opening the browser is not supported on this platform"
    ))
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
                let miner =
                    GpuMiner::new(device.platform_index, device.device_index, local_work_size);
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

        Ok::<f64, anyhow::Error>(aggregate_hashes as f64 / aggregate_elapsed.max(0.000_001))
    })
}

fn start_phantom_bridge_listener() -> Result<(
    Receiver<Result<PhantomBridgeCompletion, String>>,
    String,
    String,
)> {
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
            b' ' => vec!['%', '2', '0'],
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
