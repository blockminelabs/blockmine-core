use sha2::{Digest, Sha256};
use solana_sdk::pubkey::Pubkey;

pub fn build_solution_payload(challenge: &[u8; 32], miner: &Pubkey) -> [u8; 72] {
    let mut payload = [0u8; 72];
    payload[..32].copy_from_slice(challenge);
    payload[32..64].copy_from_slice(miner.as_ref());
    payload
}

pub fn compute_solution_hash(challenge: &[u8; 32], miner: &Pubkey, nonce: u64) -> [u8; 32] {
    let mut payload = build_solution_payload(challenge, miner);
    compute_solution_hash_from_payload(&mut payload, nonce)
}

pub fn compute_solution_hash_from_payload(payload: &mut [u8; 72], nonce: u64) -> [u8; 32] {
    payload[64..72].copy_from_slice(&nonce.to_le_bytes());
    Sha256::digest(payload).into()
}

pub fn hash_meets_target(hash: &[u8; 32], target: &[u8; 32]) -> bool {
    hash < target
}
