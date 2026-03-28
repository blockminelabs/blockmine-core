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
    ImportedSecret,
    ImportedSeedPhrase,
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
const IMPORTED_SECRET_PREFIX: &str = "imported-key";
const IMPORTED_SEED_PREFIX: &str = "imported-seed";

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

pub fn import_wallet_from_seed_phrase(phrase: &str, label: Option<&str>) -> Result<ManagedWallet> {
    let normalized = phrase.trim();
    let mnemonic = Mnemonic::parse_in_normalized(Language::English, normalized)
        .context("invalid recovery phrase")?;
    let canonical_phrase = mnemonic.to_string();
    let keypair =
        keypair_from_seed_phrase_and_passphrase(&canonical_phrase, "").map_err(|error| {
            anyhow::anyhow!("failed to derive a Solana keypair from the recovery phrase: {error}")
        })?;

    persist_managed_wallet(
        &keypair,
        label,
        Some(&canonical_phrase),
        WalletSource::ImportedSeedPhrase,
        IMPORTED_SEED_PREFIX,
    )
}

pub fn import_wallet_from_private_key(
    raw_secret: &str,
    label: Option<&str>,
) -> Result<ManagedWallet> {
    let keypair = parse_private_key_input(raw_secret)?;
    persist_managed_wallet(
        &keypair,
        label,
        None,
        WalletSource::ImportedSecret,
        IMPORTED_SECRET_PREFIX,
    )
}

pub fn list_managed_wallets() -> Result<Vec<ManagedWallet>> {
    let wallet_dir = app_storage_dir()?.join("wallets");
    fs::create_dir_all(&wallet_dir)
        .with_context(|| format!("failed to create wallet directory {}", wallet_dir.display()))?;

    let mut wallets = Vec::new();
    for entry in fs::read_dir(&wallet_dir)
        .with_context(|| format!("failed to read {}", wallet_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if file_name.ends_with(".recovery.json") {
            continue;
        }

        let keypair = match read_keypair_file(&path) {
            Ok(keypair) => keypair,
            Err(_) => continue,
        };
        let pubkey = keypair.pubkey().to_string();
        let recovery_path = wallet_dir.join(file_name.replace(".json", ".recovery.json"));
        let legacy_seed_path = if file_name == DESKTOP_SESSION_WALLET_FILENAME {
            Some(wallet_dir.join(DESKTOP_SESSION_WALLET_SEED_FILENAME))
        } else {
            None
        };
        let source = wallet_source_from_filename(file_name);

        wallets.push(ManagedWallet {
            pubkey,
            keypair_path: path,
            seed_phrase_path: if recovery_path.exists() {
                Some(recovery_path)
            } else {
                legacy_seed_path.filter(|candidate| candidate.exists())
            },
            seed_phrase: None,
            source,
        });
    }

    wallets.sort_by(|left, right| {
        source_rank(left.source)
            .cmp(&source_rank(right.source))
            .then_with(|| left.pubkey.cmp(&right.pubkey))
    });
    Ok(wallets)
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

fn persist_managed_wallet(
    keypair: &Keypair,
    label: Option<&str>,
    recovery_phrase: Option<&str>,
    source: WalletSource,
    default_prefix: &str,
) -> Result<ManagedWallet> {
    let wallet_dir = app_storage_dir()?.join("wallets");
    fs::create_dir_all(&wallet_dir)
        .with_context(|| format!("failed to create wallet directory {}", wallet_dir.display()))?;

    let pubkey = keypair.pubkey().to_string();
    let prefix = sanitize_label(label.unwrap_or(default_prefix));
    let basename = format!("{prefix}-{pubkey}");
    let keypair_path = wallet_dir.join(format!("{basename}.json"));
    let recovery_phrase_path = recovery_phrase.map(|_| wallet_dir.join(format!("{basename}.recovery.json")));

    write_keypair_file(keypair, &keypair_path)
        .map_err(|error| anyhow::anyhow!("failed to write {}: {error}", keypair_path.display()))?;
    if let (Some(path), Some(phrase)) = (&recovery_phrase_path, recovery_phrase) {
        write_recovery_phrase_file(path, phrase)?;
    }

    Ok(ManagedWallet {
        pubkey,
        keypair_path,
        seed_phrase_path: recovery_phrase_path,
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

fn wallet_source_from_filename(file_name: &str) -> WalletSource {
    if file_name == DESKTOP_SESSION_WALLET_FILENAME {
        WalletSource::SessionDelegate
    } else if file_name.starts_with(IMPORTED_SECRET_PREFIX) {
        WalletSource::ImportedSecret
    } else if file_name.starts_with(IMPORTED_SEED_PREFIX) {
        WalletSource::ImportedSeedPhrase
    } else {
        WalletSource::DedicatedGenerated
    }
}

fn source_rank(source: WalletSource) -> u8 {
    match source {
        WalletSource::SessionDelegate => 0,
        WalletSource::ImportedSeedPhrase => 1,
        WalletSource::ImportedSecret => 2,
        WalletSource::DedicatedGenerated => 3,
        WalletSource::ImportedFile => 4,
    }
}
