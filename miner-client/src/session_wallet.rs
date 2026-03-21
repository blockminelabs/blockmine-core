use anyhow::{Context, Result};
use anchor_lang::AccountDeserialize;
use blockmine_program::{constants::CONFIG_SEED, state::ProtocolConfig};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    instruction::Instruction,
    message::Message,
    program_pack::Pack,
    pubkey::Pubkey,
    signature::{read_keypair_file, Signature, Signer},
    system_instruction,
    transaction::Transaction,
};
use spl_associated_token_account::{
    get_associated_token_address,
    instruction::create_associated_token_account_idempotent,
};
use spl_token::instruction::transfer_checked;

use crate::wallet_store::{load_session_delegate_wallet, ManagedWallet};

#[derive(Debug, Clone)]
pub struct SessionSweepResult {
    pub wallet_pubkey: Pubkey,
    pub balance_before: u64,
    pub bloc_balance_before: u64,
    pub sent_lamports: u64,
    pub sent_bloc_raw: u64,
    pub signature: Option<Signature>,
    pub skipped_reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SessionSweepSummary {
    pub attempted: usize,
    pub swept: usize,
    pub total_sent_lamports: u64,
    pub total_sent_bloc_raw: u64,
    pub results: Vec<SessionSweepResult>,
}

#[derive(Debug, Clone)]
pub struct SessionWalletBalance {
    pub wallet_pubkey: Pubkey,
    pub balance_lamports: u64,
    pub bloc_token_account: Pubkey,
    pub bloc_balance_raw: u64,
}

#[derive(Debug, Clone)]
pub struct SessionBalanceSummary {
    pub wallet_count: usize,
    pub funded_wallet_count: usize,
    pub total_balance_lamports: u64,
    pub total_bloc_balance_raw: u64,
    pub bloc_mint: Pubkey,
    pub bloc_decimals: u8,
    pub balances: Vec<SessionWalletBalance>,
}

pub fn list_session_delegate_wallets() -> Result<Vec<ManagedWallet>> {
    Ok(load_session_delegate_wallet()?.into_iter().collect())
}

pub fn sweep_single_session_delegate_wallet(
    rpc_url: &str,
    program_id: Pubkey,
    wallet: &ManagedWallet,
    recipient: Pubkey,
    requested_sol_lamports: u64,
    requested_bloc_raw: u64,
) -> Result<SessionSweepSummary> {
    let client = RpcClient::new_with_commitment(rpc_url.to_string(), CommitmentConfig::confirmed());
    sweep_wallets_with_client(
        &client,
        program_id,
        &[wallet.clone()],
        recipient,
        requested_sol_lamports,
        requested_bloc_raw,
    )
}

pub fn sweep_all_session_delegate_wallets(
    rpc_url: &str,
    program_id: Pubkey,
    recipient: Pubkey,
) -> Result<SessionSweepSummary> {
    let wallets = list_session_delegate_wallets()?;
    let client = RpcClient::new_with_commitment(rpc_url.to_string(), CommitmentConfig::confirmed());
    sweep_wallets_with_client(&client, program_id, &wallets, recipient, u64::MAX, 0)
}

pub fn load_session_delegate_balances(rpc_url: &str, program_id: Pubkey) -> Result<SessionBalanceSummary> {
    let wallets = list_session_delegate_wallets()?;
    let client = RpcClient::new_with_commitment(rpc_url.to_string(), CommitmentConfig::confirmed());
    let protocol_config = fetch_protocol_config(&client, program_id)?;
    let mut balances = Vec::with_capacity(wallets.len());
    let mut total_balance_lamports = 0u64;
    let mut total_bloc_balance_raw = 0u64;
    let mut funded_wallet_count = 0usize;

    for wallet in &wallets {
        let wallet_pubkey = wallet
            .pubkey
            .parse::<Pubkey>()
            .with_context(|| format!("invalid wallet pubkey {}", wallet.pubkey))?;
        let balance_lamports = client
            .get_balance(&wallet_pubkey)
            .with_context(|| format!("failed to fetch balance for {wallet_pubkey}"))?;
        let bloc_token_account = get_associated_token_address(&wallet_pubkey, &protocol_config.bloc_mint);
        let bloc_balance_raw = client
            .get_token_account_balance(&bloc_token_account)
            .ok()
            .and_then(|amount| amount.amount.parse::<u64>().ok())
            .unwrap_or(0);

        if balance_lamports > 0 || bloc_balance_raw > 0 {
            funded_wallet_count += 1;
            total_balance_lamports = total_balance_lamports.saturating_add(balance_lamports);
            total_bloc_balance_raw = total_bloc_balance_raw.saturating_add(bloc_balance_raw);
        }
        balances.push(SessionWalletBalance {
            wallet_pubkey,
            balance_lamports,
            bloc_token_account,
            bloc_balance_raw,
        });
    }

    Ok(SessionBalanceSummary {
        wallet_count: wallets.len(),
        funded_wallet_count,
        total_balance_lamports,
        total_bloc_balance_raw,
        bloc_mint: protocol_config.bloc_mint,
        bloc_decimals: protocol_config.token_decimals,
        balances,
    })
}

fn sweep_wallets_with_client(
    client: &RpcClient,
    program_id: Pubkey,
    wallets: &[ManagedWallet],
    recipient: Pubkey,
    requested_sol_lamports: u64,
    requested_bloc_raw: u64,
) -> Result<SessionSweepSummary> {
    let protocol_config = fetch_protocol_config(client, program_id)?;
    let mut summary = SessionSweepSummary {
        attempted: wallets.len(),
        swept: 0,
        total_sent_lamports: 0,
        total_sent_bloc_raw: 0,
        results: Vec::with_capacity(wallets.len()),
    };

    for wallet in wallets {
        let result = sweep_wallet_with_client(
            client,
            wallet,
            recipient,
            requested_sol_lamports,
            requested_bloc_raw,
            &protocol_config,
        )?;
        if result.signature.is_some() && (result.sent_lamports > 0 || result.sent_bloc_raw > 0) {
            summary.swept += 1;
            summary.total_sent_lamports = summary
                .total_sent_lamports
                .saturating_add(result.sent_lamports);
            summary.total_sent_bloc_raw = summary
                .total_sent_bloc_raw
                .saturating_add(result.sent_bloc_raw);
        }
        summary.results.push(result);
    }

    Ok(summary)
}

fn sweep_wallet_with_client(
    client: &RpcClient,
    wallet: &ManagedWallet,
    recipient: Pubkey,
    requested_sol_lamports: u64,
    requested_bloc_raw: u64,
    protocol_config: &ProtocolConfig,
) -> Result<SessionSweepResult> {
    let fallback_pubkey = wallet.pubkey.parse::<Pubkey>().unwrap_or_default();
    let keypair = match read_keypair_file(&wallet.keypair_path)
        .map_err(|error| anyhow::anyhow!("failed to read {}: {error}", wallet.keypair_path.display()))
    {
        Ok(keypair) => keypair,
        Err(error) => {
            return Ok(SessionSweepResult {
                wallet_pubkey: fallback_pubkey,
                balance_before: 0,
                bloc_balance_before: 0,
                sent_lamports: 0,
                sent_bloc_raw: 0,
                signature: None,
                skipped_reason: Some(error.to_string()),
            });
        }
    };
    let wallet_pubkey = keypair.pubkey();

    if wallet_pubkey == recipient {
        return Ok(SessionSweepResult {
            wallet_pubkey,
            balance_before: 0,
            bloc_balance_before: 0,
            sent_lamports: 0,
            sent_bloc_raw: 0,
            signature: None,
            skipped_reason: Some("wallet already matches the recipient".to_string()),
        });
    }

    let balance = match client
        .get_balance(&wallet_pubkey)
        .with_context(|| format!("failed to fetch balance for {wallet_pubkey}"))
    {
        Ok(balance) => balance,
        Err(error) => {
            return Ok(SessionSweepResult {
                wallet_pubkey,
                balance_before: 0,
                bloc_balance_before: 0,
                sent_lamports: 0,
                sent_bloc_raw: 0,
                signature: None,
                skipped_reason: Some(error.to_string()),
            });
        }
    };
    let sender_bloc_ata = get_associated_token_address(&wallet_pubkey, &protocol_config.bloc_mint);
    let bloc_balance_before = client
        .get_token_account_balance(&sender_bloc_ata)
        .ok()
        .and_then(|amount| amount.amount.parse::<u64>().ok())
        .unwrap_or(0);

    if balance == 0 && bloc_balance_before == 0 {
        return Ok(SessionSweepResult {
            wallet_pubkey,
            balance_before: 0,
            bloc_balance_before: 0,
            sent_lamports: 0,
            sent_bloc_raw: 0,
            signature: None,
            skipped_reason: Some("wallet balance is already zero".to_string()),
        });
    }

    let recent_blockhash = match client
        .get_latest_blockhash()
        .context("failed to fetch the latest blockhash for the sweep")
    {
        Ok(recent_blockhash) => recent_blockhash,
        Err(error) => {
            return Ok(SessionSweepResult {
                wallet_pubkey,
                balance_before: balance,
                bloc_balance_before,
                sent_lamports: 0,
                sent_bloc_raw: 0,
                signature: None,
                skipped_reason: Some(error.to_string()),
            });
        }
    };

    let recipient_bloc_ata = get_associated_token_address(&recipient, &protocol_config.bloc_mint);
    let recipient_needs_bloc_ata = requested_bloc_raw > 0 && client.get_account(&recipient_bloc_ata).is_err();
    let ata_rent_lamports = if recipient_needs_bloc_ata {
        client
            .get_minimum_balance_for_rent_exemption(spl_token::state::Account::LEN)
            .unwrap_or(0)
    } else {
        0
    };

    let wants_sol_max = requested_sol_lamports >= balance;
    let wants_sol_transfer = requested_sol_lamports > 0;
    let wants_bloc_transfer = requested_bloc_raw > 0;

    if !wants_sol_transfer && !wants_bloc_transfer {
        return Ok(SessionSweepResult {
            wallet_pubkey,
            balance_before: balance,
            bloc_balance_before,
            sent_lamports: 0,
            sent_bloc_raw: 0,
            signature: None,
            skipped_reason: Some("choose a SOL or BLOC amount to withdraw".to_string()),
        });
    }

    if requested_bloc_raw > bloc_balance_before {
        return Ok(SessionSweepResult {
            wallet_pubkey,
            balance_before: balance,
            bloc_balance_before,
            sent_lamports: 0,
            sent_bloc_raw: 0,
            signature: None,
            skipped_reason: Some("requested BLOC amount exceeds the desktop wallet balance".to_string()),
        });
    }

    let mut fee_preview_instructions = Vec::new();
    if recipient_needs_bloc_ata {
        fee_preview_instructions.push(create_associated_token_account_idempotent(
            &wallet_pubkey,
            &recipient,
            &protocol_config.bloc_mint,
            &spl_token::ID,
        ));
    }
    if wants_sol_transfer {
        fee_preview_instructions.push(system_instruction::transfer(&wallet_pubkey, &recipient, 1));
    }
    if wants_bloc_transfer {
        fee_preview_instructions.push(
            transfer_checked(
                &spl_token::ID,
                &sender_bloc_ata,
                &protocol_config.bloc_mint,
                &recipient_bloc_ata,
                &wallet_pubkey,
                &[],
                requested_bloc_raw.max(1),
                protocol_config.token_decimals,
            )
            .context("failed to build the BLOC withdrawal instruction")?,
        );
    }

    let message = Message::new_with_blockhash(
        &fee_preview_instructions,
        Some(&wallet_pubkey),
        &recent_blockhash,
    );
    let fee = client
        .get_fee_for_message(&message)
        .ok()
        .filter(|fee| *fee > 0)
        .unwrap_or(10_000);

    let required_sol_reserve = fee.saturating_add(ata_rent_lamports);
    if balance <= required_sol_reserve && wants_sol_transfer {
        return Ok(SessionSweepResult {
            wallet_pubkey,
            balance_before: balance,
            bloc_balance_before,
            sent_lamports: 0,
            sent_bloc_raw: 0,
            signature: None,
            skipped_reason: Some("balance is too small to cover the network fee".to_string()),
        });
    }

    let max_spendable_sol = balance.saturating_sub(required_sol_reserve);
    let sol_lamports = if wants_sol_transfer {
        if wants_sol_max {
            max_spendable_sol
        } else if requested_sol_lamports > max_spendable_sol {
            return Ok(SessionSweepResult {
                wallet_pubkey,
                balance_before: balance,
                bloc_balance_before,
                sent_lamports: 0,
                sent_bloc_raw: 0,
                signature: None,
                skipped_reason: Some(
                    "requested SOL amount exceeds the spendable balance after fees".to_string(),
                ),
            });
        } else {
            requested_sol_lamports
        }
    } else {
        0
    };

    if required_sol_reserve > balance {
        return Ok(SessionSweepResult {
            wallet_pubkey,
            balance_before: balance,
            bloc_balance_before,
            sent_lamports: 0,
            sent_bloc_raw: 0,
            signature: None,
            skipped_reason: Some("not enough SOL to cover network fees for this withdrawal".to_string()),
        });
    }

    let mut instructions: Vec<Instruction> = Vec::new();
    if recipient_needs_bloc_ata {
        instructions.push(create_associated_token_account_idempotent(
            &wallet_pubkey,
            &recipient,
            &protocol_config.bloc_mint,
            &spl_token::ID,
        ));
    }
    if sol_lamports > 0 {
        instructions.push(system_instruction::transfer(
            &wallet_pubkey,
            &recipient,
            sol_lamports,
        ));
    }
    if wants_bloc_transfer {
        instructions.push(
            transfer_checked(
                &spl_token::ID,
                &sender_bloc_ata,
                &protocol_config.bloc_mint,
                &recipient_bloc_ata,
                &wallet_pubkey,
                &[],
                requested_bloc_raw,
                protocol_config.token_decimals,
            )
            .context("failed to build the BLOC withdrawal instruction")?,
        );
    }

    if instructions.is_empty() {
        return Ok(SessionSweepResult {
            wallet_pubkey,
            balance_before: balance,
            bloc_balance_before,
            sent_lamports: 0,
            sent_bloc_raw: 0,
            signature: None,
            skipped_reason: Some("nothing to withdraw after fee checks".to_string()),
        });
    }

    let transaction = Transaction::new_signed_with_payer(
        &instructions,
        Some(&wallet_pubkey),
        &[&keypair],
        recent_blockhash,
    );
    let signature = match client
        .send_and_confirm_transaction(&transaction)
        .with_context(|| format!("failed to sweep session wallet {wallet_pubkey}"))
    {
        Ok(signature) => signature,
        Err(error) => {
            return Ok(SessionSweepResult {
                wallet_pubkey,
                balance_before: balance,
                bloc_balance_before,
                sent_lamports: 0,
                sent_bloc_raw: 0,
                signature: None,
                skipped_reason: Some(error.to_string()),
            });
        }
    };

    Ok(SessionSweepResult {
        wallet_pubkey,
        balance_before: balance,
        bloc_balance_before,
        sent_lamports: sol_lamports,
        sent_bloc_raw: requested_bloc_raw,
        signature: Some(signature),
        skipped_reason: None,
    })
}

fn fetch_protocol_config(client: &RpcClient, program_id: Pubkey) -> Result<ProtocolConfig> {
    let (config_pda, _) = Pubkey::find_program_address(&[CONFIG_SEED], &program_id);
    let account = client
        .get_account(&config_pda)
        .with_context(|| format!("protocol config {} not found", config_pda))?;
    let mut data = account.data.as_slice();
    ProtocolConfig::try_deserialize(&mut data).context("failed to deserialize protocol config")
}
