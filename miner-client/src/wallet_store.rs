use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use bip39::{Language, Mnemonic};
use serde::{Deserialize, Serialize};
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
const DESKTOP_SESSION_WALLET_RECOVERY_FILENAME: &str = "desktop-session-wallet.recovery.json";
const DESKTOP_SESSION_WALLET_SEED_FILENAME: &str = "desktop-session-wallet.seed.txt";

#[derive(Debug, Serialize, Deserialize)]
struct RecoveryPhraseRecord {
    phrase: String,
}

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
    let recovery_phrase_path = wallet_dir.join(format!("{basename}.recovery.json"));

    write_keypair_file(&keypair, &keypair_path)
        .map_err(|error| anyhow::anyhow!("failed to write {}: {error}", keypair_path.display()))?;
    write_recovery_phrase_file(&recovery_phrase_path, &phrase)?;

    Ok(ManagedWallet {
        pubkey,
        keypair_path,
        seed_phrase_path: Some(recovery_phrase_path),
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
    let recovery_phrase_path = wallet_dir.join(DESKTOP_SESSION_WALLET_RECOVERY_FILENAME);

    if let Some(existing) = load_session_delegate_wallet()? {
        return Ok(existing);
    }

    let mnemonic = Mnemonic::generate_in(Language::English, 12)
        .context("failed to generate a recovery phrase for the desktop mining wallet")?;
    let phrase = mnemonic.to_string();
    let keypair = keypair_from_seed_phrase_and_passphrase(&phrase, "").map_err(|error| {
        anyhow::anyhow!("failed to derive a Solana keypair from the recovery phrase: {error}")
    })?;

    let pubkey = keypair.pubkey().to_string();
    let _ = label;

    write_keypair_file(&keypair, &keypair_path)
        .map_err(|error| anyhow::anyhow!("failed to write {}: {error}", keypair_path.display()))?;
    write_recovery_phrase_file(&recovery_phrase_path, &phrase)?;

    Ok(ManagedWallet {
        pubkey,
        keypair_path,
        seed_phrase_path: Some(recovery_phrase_path),
        seed_phrase: Some(phrase),
        source: WalletSource::SessionDelegate,
    })
}

pub fn load_session_delegate_wallet() -> Result<Option<ManagedWallet>> {
    let wallet_dir = app_storage_dir()?.join("wallets");
    let keypair_path = wallet_dir.join(DESKTOP_SESSION_WALLET_FILENAME);
    let recovery_phrase_path = wallet_dir.join(DESKTOP_SESSION_WALLET_RECOVERY_FILENAME);
    let legacy_seed_phrase_path = wallet_dir.join(DESKTOP_SESSION_WALLET_SEED_FILENAME);
    if !keypair_path.exists() {
        return Ok(None);
    }

    let keypair = read_keypair_file(&keypair_path)
        .map_err(|error| anyhow::anyhow!("failed to read {}: {error}", keypair_path.display()))?;

    Ok(Some(ManagedWallet {
        pubkey: keypair.pubkey().to_string(),
        keypair_path,
        seed_phrase_path: recovery_phrase_path
            .exists()
            .then_some(recovery_phrase_path)
            .or_else(|| legacy_seed_phrase_path.exists().then_some(legacy_seed_phrase_path)),
        seed_phrase: None,
        source: WalletSource::SessionDelegate,
    }))
}

pub fn load_managed_keypair(wallet: &ManagedWallet) -> Result<Keypair> {
    read_keypair_file(&wallet.keypair_path).map_err(|error| {
        anyhow::anyhow!("failed to read {}: {error}", wallet.keypair_path.display())
    })
}

pub fn load_wallet_seed_phrase(wallet: &ManagedWallet) -> Result<Option<String>> {
    if let Some(phrase) = &wallet.seed_phrase {
        return Ok(Some(phrase.trim().to_string()));
    }

    let Some(path) = &wallet.seed_phrase_path else {
        return Ok(None);
    };
    if !path.exists() {
        return Ok(None);
    }

    let trimmed = read_recovery_phrase_file(path)?;
    if trimmed.is_empty() {
        Ok(None)
    } else {
        Ok(Some(trimmed))
    }
}

fn write_recovery_phrase_file(path: &Path, phrase: &str) -> Result<()> {
    let raw = serde_json::to_string_pretty(&RecoveryPhraseRecord {
        phrase: phrase.to_string(),
    })
    .context("failed to serialize the recovery phrase payload")?;

    fs::write(path, raw).with_context(|| {
        format!(
            "failed to write the recovery phrase to {}",
            path.display()
        )
    })?;

    Ok(())
}

fn read_recovery_phrase_file(path: &Path) -> Result<String> {
    let raw = fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;

    if path.extension().and_then(|value| value.to_str()) == Some("json") {
        let payload = serde_json::from_str::<RecoveryPhraseRecord>(&raw).with_context(|| {
            format!("failed to decode recovery phrase payload from {}", path.display())
        })?;
        return Ok(payload.phrase.trim().to_string());
    }

    Ok(raw.trim().to_string())
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
