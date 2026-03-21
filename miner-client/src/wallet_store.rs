use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use bip39::{Language, Mnemonic};
use solana_sdk::signature::{
    keypair_from_seed_phrase_and_passphrase, read_keypair_file, write_keypair_file, Keypair, Signer,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalletSource {
    DedicatedGenerated,
    SessionDelegate,
    ImportedFile,
}

#[derive(Debug, Clone)]
pub struct ManagedWallet {
    pub pubkey: String,
    pub keypair_path: PathBuf,
    pub seed_phrase_path: Option<PathBuf>,
    pub seed_phrase: Option<String>,
    pub source: WalletSource,
}

const DESKTOP_SESSION_WALLET_FILENAME: &str = "desktop-session-wallet.json";

pub fn app_storage_dir() -> Result<PathBuf> {
    if let Some(data_dir) = dirs::data_local_dir() {
        return Ok(data_dir.join("BlockMine"));
    }

    let home = dirs::home_dir().context("unable to resolve the home directory")?;
    Ok(home.join(".blockmine"))
}

pub fn create_dedicated_wallet(label: Option<&str>) -> Result<ManagedWallet> {
    let mnemonic = Mnemonic::generate_in(Language::English, 12)
        .context("failed to generate a recovery phrase")?;
    let phrase = mnemonic.to_string();
    let keypair = keypair_from_seed_phrase_and_passphrase(&phrase, "").map_err(|error| {
        anyhow::anyhow!("failed to derive a Solana keypair from the recovery phrase: {error}")
    })?;

    let wallet_dir = app_storage_dir()?.join("wallets");
    fs::create_dir_all(&wallet_dir)
        .with_context(|| format!("failed to create wallet directory {}", wallet_dir.display()))?;

    let pubkey = keypair.pubkey().to_string();
    let prefix = sanitize_label(label.unwrap_or("wallet"));
    let basename = format!("{prefix}-{pubkey}");
    let keypair_path = wallet_dir.join(format!("{basename}.json"));
    let seed_phrase_path = wallet_dir.join(format!("{basename}.seed.txt"));

    write_keypair_file(&keypair, &keypair_path)
        .map_err(|error| anyhow::anyhow!("failed to write {}: {error}", keypair_path.display()))?;
    fs::write(&seed_phrase_path, format!("{phrase}\n")).with_context(|| {
        format!(
            "failed to write the recovery phrase to {}",
            seed_phrase_path.display()
        )
    })?;

    Ok(ManagedWallet {
        pubkey,
        keypair_path,
        seed_phrase_path: Some(seed_phrase_path),
        seed_phrase: Some(phrase),
        source: WalletSource::DedicatedGenerated,
    })
}

pub fn import_wallet_file(path: &Path) -> Result<ManagedWallet> {
    let keypair = read_keypair_file(path)
        .map_err(|error| anyhow::anyhow!("failed to read {}: {error}", path.display()))?;

    Ok(ManagedWallet {
        pubkey: keypair.pubkey().to_string(),
        keypair_path: path.to_path_buf(),
        seed_phrase_path: None,
        seed_phrase: None,
        source: WalletSource::ImportedFile,
    })
}

pub fn create_session_delegate_wallet(label: Option<&str>) -> Result<ManagedWallet> {
    let wallet_dir = app_storage_dir()?.join("wallets");
    fs::create_dir_all(&wallet_dir)
        .with_context(|| format!("failed to create wallet directory {}", wallet_dir.display()))?;
    let keypair_path = wallet_dir.join(DESKTOP_SESSION_WALLET_FILENAME);

    if let Some(existing) = load_session_delegate_wallet()? {
        return Ok(existing);
    }

    let keypair = Keypair::new();

    let pubkey = keypair.pubkey().to_string();
    let _ = label;

    write_keypair_file(&keypair, &keypair_path)
        .map_err(|error| anyhow::anyhow!("failed to write {}: {error}", keypair_path.display()))?;

    Ok(ManagedWallet {
        pubkey,
        keypair_path,
        seed_phrase_path: None,
        seed_phrase: None,
        source: WalletSource::SessionDelegate,
    })
}

pub fn load_session_delegate_wallet() -> Result<Option<ManagedWallet>> {
    let wallet_dir = app_storage_dir()?.join("wallets");
    let keypair_path = wallet_dir.join(DESKTOP_SESSION_WALLET_FILENAME);
    if !keypair_path.exists() {
        return Ok(None);
    }

    let keypair = read_keypair_file(&keypair_path)
        .map_err(|error| anyhow::anyhow!("failed to read {}: {error}", keypair_path.display()))?;

    Ok(Some(ManagedWallet {
        pubkey: keypair.pubkey().to_string(),
        keypair_path,
        seed_phrase_path: None,
        seed_phrase: None,
        source: WalletSource::SessionDelegate,
    }))
}

pub fn load_managed_keypair(wallet: &ManagedWallet) -> Result<Keypair> {
    read_keypair_file(&wallet.keypair_path)
        .map_err(|error| anyhow::anyhow!("failed to read {}: {error}", wallet.keypair_path.display()))
}

fn sanitize_label(input: &str) -> String {
    let cleaned = input
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();

    let trimmed = cleaned.trim_matches('-');
    if trimmed.is_empty() {
        "wallet".to_string()
    } else {
        trimmed.to_string()
    }
}
