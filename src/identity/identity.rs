use anyhow::{Context, Result};
use k256::ecdsa::SigningKey;
use rand::rngs::OsRng;
use revm::primitives::{keccak256, Address};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct NodeIdentity {
    #[allow(dead_code)]
    pub signing_key: SigningKey,
    pub address: Address,
}

#[derive(Debug, Serialize, Deserialize)]
struct NodeKeyRecord {
    private_key: String,
}

pub fn load_or_create_identity(path: impl AsRef<Path>) -> Result<NodeIdentity> {
    let path = path.as_ref();
    if let Ok(data) = fs::read_to_string(path) {
        let record: NodeKeyRecord = serde_json::from_str(&data)
            .with_context(|| format!("failed to parse node key file {}", path.display()))?;
        let key_bytes = hex::decode(record.private_key.trim_start_matches("0x"))
            .context("invalid node key hex")?;
        let signing_key = SigningKey::from_slice(&key_bytes).context("invalid node key bytes")?;
        let address = signing_key_to_address(&signing_key);
        return Ok(NodeIdentity { signing_key, address });
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).ok();
    }
    let signing_key = SigningKey::random(&mut OsRng);
    let address = signing_key_to_address(&signing_key);
    let record = NodeKeyRecord {
        private_key: format!("0x{}", hex::encode(signing_key.to_bytes())),
    };
    let data = serde_json::to_string_pretty(&record)?;
    fs::write(path, data)?;
    Ok(NodeIdentity { signing_key, address })
}

fn signing_key_to_address(signing_key: &SigningKey) -> Address {
    let verifying_key = signing_key.verifying_key();
    let encoded = verifying_key.to_encoded_point(false);
    let public_key = encoded.as_bytes();
    let hash = keccak256(&public_key[1..]);
    Address::from_slice(&hash[12..])
}
