use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use bip39::{Language, Mnemonic};
use serde::{Deserialize, Serialize};
use solana_sdk::signature::{
    keypair_from_seed_phrase_and_passphrase, read_keypair_file, write_keypair_file, Keypair, Signer,
};

use crate::wallet_store::{app_storage_dir, load_wallet_seed_phrase, ManagedWallet, WalletSource};

pub const VAST_WALLET_SEED_ENV: &str = "BLOCKMINE_VAST_WALLET_SEED";
pub const VAST_WALLET_PRIVATE_KEY_ENV: &str = "BLOCKMINE_VAST_WALLET_PRIVATE_KEY";

const VAST_WALLET_KEYPAIR_FILENAME: &str = "worker-wallet.json";
const VAST_WALLET_RECOVERY_FILENAME: &str = "worker-wallet.recovery.json";
const VAST_WALLET_BACKUP_ACK_FILENAME: &str = "worker-wallet.backup-confirmed";

#[derive(Debug)]
pub struct EnsureWorkerWalletResult {
    pub wallet: ManagedWallet,
    pub created: bool,
    pub imported_from_env: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct RecoveryPhraseRecord {
    phrase: String,
}

pub fn vast_storage_dir() -> Result<PathBuf> {
    let path = app_storage_dir()?.join("vast");
    fs::create_dir_all(&path)
        .with_context(|| format!("failed to create vast wallet directory {}", path.display()))?;
    Ok(path)
}

pub fn worker_wallet_keypair_path() -> Result<PathBuf> {
    Ok(vast_storage_dir()?.join(VAST_WALLET_KEYPAIR_FILENAME))
}

pub fn worker_wallet_recovery_path() -> Result<PathBuf> {
    Ok(vast_storage_dir()?.join(VAST_WALLET_RECOVERY_FILENAME))
}

pub fn worker_wallet_backup_ack_path() -> Result<PathBuf> {
    Ok(vast_storage_dir()?.join(VAST_WALLET_BACKUP_ACK_FILENAME))
}

pub fn worker_wallet_backup_acknowledged() -> Result<bool> {
    Ok(worker_wallet_backup_ack_path()?.exists())
}

pub fn acknowledge_worker_wallet_backup() -> Result<()> {
    let path = worker_wallet_backup_ack_path()?;
    fs::write(&path, b"confirmed\n")
        .with_context(|| format!("failed to write wallet backup marker {}", path.display()))?;
    Ok(())
}

pub fn load_vast_worker_wallet() -> Result<Option<ManagedWallet>> {
    let keypair_path = worker_wallet_keypair_path()?;
    let recovery_phrase_path = worker_wallet_recovery_path()?;
    if !keypair_path.exists() {
        return Ok(None);
    }

    let keypair = read_keypair_file(&keypair_path)
        .map_err(|error| anyhow::anyhow!("failed to read {}: {error}", keypair_path.display()))?;

    Ok(Some(ManagedWallet {
        pubkey: keypair.pubkey().to_string(),
        keypair_path,
        seed_phrase_path: recovery_phrase_path.exists().then_some(recovery_phrase_path),
        seed_phrase: None,
        source: WalletSource::DedicatedGenerated,
    }))
}

pub fn ensure_vast_worker_wallet() -> Result<EnsureWorkerWalletResult> {
    if let Some(wallet) = load_vast_worker_wallet()? {
        return Ok(EnsureWorkerWalletResult {
            wallet,
            created: false,
            imported_from_env: false,
        });
    }

    let seed_from_env = std::env::var(VAST_WALLET_SEED_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if let Some(phrase) = seed_from_env {
        let wallet = import_worker_wallet_from_seed_phrase(&phrase)?;
        return Ok(EnsureWorkerWalletResult {
            wallet,
            created: true,
            imported_from_env: true,
        });
    }

    let private_key_from_env = std::env::var(VAST_WALLET_PRIVATE_KEY_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if let Some(secret) = private_key_from_env {
        let wallet = import_worker_wallet_from_private_key(&secret)?;
        return Ok(EnsureWorkerWalletResult {
            wallet,
            created: true,
            imported_from_env: true,
        });
    }

    let wallet = create_generated_worker_wallet()?;
    Ok(EnsureWorkerWalletResult {
        wallet,
        created: true,
        imported_from_env: false,
    })
}

pub fn load_vast_worker_seed_phrase() -> Result<Option<String>> {
    let Some(wallet) = load_vast_worker_wallet()? else {
        return Ok(None);
    };
    load_wallet_seed_phrase(&wallet)
}

fn create_generated_worker_wallet() -> Result<ManagedWallet> {
    let mnemonic = Mnemonic::generate_in(Language::English, 12)
        .context("failed to generate a recovery phrase")?;
    let phrase = mnemonic.to_string();
    let keypair = keypair_from_seed_phrase_and_passphrase(&phrase, "").map_err(|error| {
        anyhow::anyhow!("failed to derive a Solana keypair from the recovery phrase: {error}")
    })?;

    persist_worker_wallet(
        &keypair,
        Some(&phrase),
        WalletSource::DedicatedGenerated,
    )
}

fn import_worker_wallet_from_seed_phrase(phrase: &str) -> Result<ManagedWallet> {
    let normalized = phrase.trim();
    let mnemonic = Mnemonic::parse_in_normalized(Language::English, normalized)
        .context("invalid recovery phrase")?;
    let canonical_phrase = mnemonic.to_string();
    let keypair =
        keypair_from_seed_phrase_and_passphrase(&canonical_phrase, "").map_err(|error| {
            anyhow::anyhow!("failed to derive a Solana keypair from the recovery phrase: {error}")
        })?;

    persist_worker_wallet(
        &keypair,
        Some(&canonical_phrase),
        WalletSource::ImportedSeedPhrase,
    )
}

fn import_worker_wallet_from_private_key(raw_secret: &str) -> Result<ManagedWallet> {
    let keypair = parse_private_key_input(raw_secret)?;
    persist_worker_wallet(&keypair, None, WalletSource::ImportedSecret)
}

fn persist_worker_wallet(
    keypair: &Keypair,
    recovery_phrase: Option<&str>,
    source: WalletSource,
) -> Result<ManagedWallet> {
    let keypair_path = worker_wallet_keypair_path()?;
    let recovery_phrase_path = worker_wallet_recovery_path()?;

    write_keypair_file(keypair, &keypair_path)
        .map_err(|error| anyhow::anyhow!("failed to write {}: {error}", keypair_path.display()))?;
    if let Some(phrase) = recovery_phrase {
        write_recovery_phrase_file(&recovery_phrase_path, phrase)?;
    } else if recovery_phrase_path.exists() {
        fs::remove_file(&recovery_phrase_path).with_context(|| {
            format!(
                "failed to remove stale recovery phrase {}",
                recovery_phrase_path.display()
            )
        })?;
    }

    Ok(ManagedWallet {
        pubkey: keypair.pubkey().to_string(),
        keypair_path,
        seed_phrase_path: recovery_phrase.map(|_| recovery_phrase_path),
        seed_phrase: recovery_phrase.map(|value| value.to_string()),
        source,
    })
}

fn parse_private_key_input(raw_secret: &str) -> Result<Keypair> {
    let trimmed = raw_secret.trim();
    if trimmed.is_empty() {
        anyhow::bail!("private key is empty");
    }

    if trimmed.starts_with('[') {
        let bytes = serde_json::from_str::<Vec<u8>>(trimmed)
            .context("invalid private key JSON array")?;
        return keypair_from_bytes(bytes);
    }

    let compact = trimmed.lines().map(str::trim).collect::<String>();
    let bytes = bs58::decode(compact)
        .into_vec()
        .context("invalid base58 private key")?;
    keypair_from_bytes(bytes)
}

fn keypair_from_bytes(bytes: Vec<u8>) -> Result<Keypair> {
    if bytes.len() != 64 {
        anyhow::bail!(
            "private key must decode to 64 bytes, got {} bytes",
            bytes.len()
        );
    }
    Keypair::from_bytes(&bytes).context("failed to decode the Solana keypair bytes")
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
