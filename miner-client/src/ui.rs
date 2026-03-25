use std::io::{stdout, IsTerminal, Stdout, Write};
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::style::{Attribute, Print, SetAttribute};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::{execute, queue};

use crate::engine::BackendMode;

pub struct MineUiSnapshot {
    pub status: String,
    pub backend: BackendMode,
    pub wallet: String,
    pub bloc_ata: String,
    pub current_block_number: u64,
    pub current_reward: u64,
    pub difficulty_bits: u8,
    pub challenge: [u8; 32],
    pub target: [u8; 32],
    pub session_blocks_mined: u64,
    pub session_tokens_mined: u64,
    pub session_hashes: u64,
    pub wallet_blocks_mined: u64,
    pub wallet_tokens_mined: u64,
    pub protocol_blocks_mined: u64,
    pub protocol_treasury_fees: u64,
    pub last_hashrate: String,
    pub last_nonce: Option<u64>,
    pub last_hash: Option<[u8; 32]>,
    pub last_signature: Option<String>,
    pub last_event: String,
    pub recent_reports: Vec<String>,
}

pub struct MineUi {
    stdout: Stdout,
    started_at: Instant,
    interactive: bool,
    closed: bool,
}

impl MineUi {
    pub fn new() -> Result<Self> {
        let interactive = stdout().is_terminal();
        let mut ui = Self {
            stdout: stdout(),
            started_at: Instant::now(),
            interactive,
            closed: false,
        };

        if ui.interactive {
            enable_raw_mode()?;
            execute!(ui.stdout, EnterAlternateScreen, Hide)?;
        }

        Ok(ui)
    }

    pub fn render(&mut self, snapshot: &MineUiSnapshot) -> Result<()> {
        if !self.interactive {
            return Ok(());
        }

        let runtime = format_duration(self.started_at.elapsed());
        queue!(self.stdout, MoveTo(0, 0), Clear(ClearType::All))?;
        queue!(
            self.stdout,
            SetAttribute(Attribute::Bold),
            Print("BlockMine Desktop Miner\n"),
            SetAttribute(Attribute::Reset),
            Print("Ctrl+C per fermare il miner\n\n"),
            Print(format!("Status            : {}\n", snapshot.status)),
            Print(format!("Backend           : {:?}\n", snapshot.backend)),
            Print(format!("Runtime           : {}\n", runtime)),
            Print(format!("Wallet            : {}\n", snapshot.wallet)),
            Print(format!("BLOC ATA          : {}\n", snapshot.bloc_ata)),
            Print(format!(
                "Current block     : {}\n",
                snapshot.current_block_number
            )),
            Print(format!(
                "Reward            : {} BLOC\n",
                format_bloc(snapshot.current_reward)
            )),
            Print(format!(
                "Difficulty bits   : {}\n",
                snapshot.difficulty_bits
            )),
            Print(format!(
                "Challenge         : 0x{}\n",
                shorten_hex(&snapshot.challenge)
            )),
            Print(format!(
                "Target            : 0x{}\n\n",
                shorten_hex(&snapshot.target)
            )),
            Print(format!(
                "Session blocks    : {}\n",
                snapshot.session_blocks_mined
            )),
            Print(format!(
                "Session mined     : {} BLOC\n",
                format_bloc(snapshot.session_tokens_mined)
            )),
            Print(format!(
                "Session hashes    : {}\n",
                format_u64(snapshot.session_hashes)
            )),
            Print(format!(
                "Last hashrate     : {}\n\n",
                snapshot.last_hashrate
            )),
            Print(format!(
                "Wallet blocks     : {}\n",
                snapshot.wallet_blocks_mined
            )),
            Print(format!(
                "Wallet mined      : {} BLOC\n",
                format_bloc(snapshot.wallet_tokens_mined)
            )),
            Print(format!(
                "Protocol blocks   : {}\n",
                snapshot.protocol_blocks_mined
            )),
            Print(format!(
                "Treasury fees     : {} BLOC\n\n",
                format_bloc(snapshot.protocol_treasury_fees)
            )),
            Print(format!("Last event        : {}\n", snapshot.last_event)),
        )?;

        if let Some(nonce) = snapshot.last_nonce {
            queue!(
                self.stdout,
                Print(format!("Last nonce        : {}\n", nonce))
            )?;
        }
        if let Some(hash) = snapshot.last_hash {
            queue!(
                self.stdout,
                Print(format!("Last hash         : 0x{}\n", hex::encode(hash)))
            )?;
        }
        if let Some(signature) = &snapshot.last_signature {
            queue!(
                self.stdout,
                Print(format!("Last tx           : {}\n", signature))
            )?;
        }

        if !snapshot.recent_reports.is_empty() {
            queue!(self.stdout, Print("\nRecent reports\n"))?;
            for report in &snapshot.recent_reports {
                queue!(self.stdout, Print(format!("  {}\n", report)))?;
            }
        }

        self.stdout.flush()?;
        Ok(())
    }

    pub fn shutdown(&mut self) -> Result<()> {
        if self.closed {
            return Ok(());
        }

        if self.interactive {
            execute!(self.stdout, Show, LeaveAlternateScreen)?;
            disable_raw_mode()?;
        }

        self.closed = true;
        Ok(())
    }
}

impl Drop for MineUi {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

pub fn format_bloc(amount: u64) -> String {
    let whole = amount / 1_000_000_000;
    let fractional = amount % 1_000_000_000;
    format!("{whole}.{fractional:09}")
}

pub fn format_u64(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, ch) in digits.chars().rev().enumerate() {
        if index != 0 && index % 3 == 0 {
            out.push('_');
        }
        out.push(ch);
    }
    out.chars().rev().collect()
}

fn shorten_hex(bytes: &[u8; 32]) -> String {
    let full = hex::encode(bytes);
    format!("{}...{}", &full[..16], &full[full.len() - 16..])
}

fn format_duration(duration: Duration) -> String {
    let total_seconds = duration.as_secs();
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}")
}
