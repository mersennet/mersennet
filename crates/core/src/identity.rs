use anyhow::{Context, Result};
use k256::ecdsa::{SigningKey, signature::hazmat::PrehashSigner};
use rand::rngs::OsRng;
use revm::primitives::{Address, keccak256};
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
        make_owner_only(path);
        let record: NodeKeyRecord = serde_json::from_str(&data)
            .with_context(|| format!("failed to parse node key file {}", path.display()))?;
        let key_bytes = hex::decode(record.private_key.trim_start_matches("0x"))
            .context("invalid node key hex")?;
        let signing_key = SigningKey::from_slice(&key_bytes).context("invalid node key bytes")?;
        let address = signing_key_to_address(&signing_key);
        return Ok(NodeIdentity {
            signing_key,
            address,
        });
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
    write_owner_only(path, data.as_bytes())
        .with_context(|| format!("failed to create node key file {}", path.display()))?;
    Ok(NodeIdentity {
        signing_key,
        address,
    })
}

/// The node key signs blocks for a validator: owner-only from the moment it
/// exists. `create_new`, so a key file that exists but could not be read is
/// never replaced by a fresh identity.
fn write_owner_only(path: &Path, data: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path)?.write_all(data)
}

/// Keys written before owner-only creation (0644) are tightened on load.
#[cfg(unix)]
fn make_owner_only(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let loose = fs::metadata(path).is_ok_and(|m| m.permissions().mode() & 0o077 != 0);
    if !loose {
        return;
    }
    if let Err(e) = fs::set_permissions(path, fs::Permissions::from_mode(0o600)) {
        tracing::warn!(path = %path.display(), error = %e, "node key file is readable by others and could not be made owner-only");
    }
}
#[cfg(not(unix))]
fn make_owner_only(_: &Path) {}

fn signing_key_to_address(signing_key: &SigningKey) -> Address {
    let verifying_key = signing_key.verifying_key();
    let encoded = verifying_key.to_encoded_point(false);
    let public_key = encoded.as_bytes();
    let hash = keccak256(&public_key[1..]);
    Address::from_slice(&hash[12..])
}

/// Signed "who am I" answer a node gives over its P2P TCP port so an operator
/// can prove they run a reachable node (see `whoami` in the network crate).
///
/// The message is EIP-191 personal-sign formatted, so any Ethereum tooling
/// (`ethers.verifyMessage`) recovers the node identity from the signature.
/// The operator address is whatever the node's config says
/// (`p2p.operator_address`); binding it into the signed message means only the
/// person who configured the node can claim it.
#[derive(Debug, Clone)]
pub struct NodeAttestor {
    pub signing_key: SigningKey,
    pub identity: Address,
    pub operator: Option<Address>,
    pub version: String,
}

pub fn node_attestation_message(
    nonce_hex: &str,
    identity: Address,
    operator: Option<Address>,
    height: u64,
) -> String {
    format!(
        "Mersennet node attestation v1\nnonce: {}\nidentity: 0x{}\noperator: {}\nheight: {}",
        nonce_hex,
        hex::encode(identity.as_slice()),
        operator
            .map(|o| format!("0x{}", hex::encode(o.as_slice())))
            .unwrap_or_else(|| "none".to_string()),
        height
    )
}

impl NodeAttestor {
    /// Build the JSON answer for a `whoami` request carrying `nonce`.
    pub fn attest(&self, nonce: &[u8], height: u64) -> serde_json::Value {
        let nonce_hex = hex::encode(nonce);
        let message = node_attestation_message(&nonce_hex, self.identity, self.operator, height);
        // EIP-191: keccak256("\x19Ethereum Signed Message:\n" || len || message)
        let mut prefixed = format!("\x19Ethereum Signed Message:\n{}", message.len()).into_bytes();
        prefixed.extend_from_slice(message.as_bytes());
        let digest = keccak256(&prefixed);
        let (sig, recid) = self
            .signing_key
            .sign_prehash(digest.as_slice())
            .map(|(s, r): (k256::ecdsa::Signature, k256::ecdsa::RecoveryId)| (s, r))
            .expect("attestation signing failed");
        let mut sig_bytes = sig.to_bytes().to_vec();
        sig_bytes.push(27 + recid.to_byte());
        serde_json::json!({
            "identity": format!("0x{}", hex::encode(self.identity.as_slice())),
            "operator": self.operator.map(|o| format!("0x{}", hex::encode(o.as_slice()))),
            "height": height,
            "version": self.version,
            "message": message,
            "signature": format!("0x{}", hex::encode(sig_bytes)),
            // Lets the operator register this node as a validator with one
            // click: the staking precompile checks this exact signature.
            "registrationProof": self.registration_proof().map(|p| format!("0x{}", hex::encode(p))),
        })
    }

    /// Node-key signature binding this identity to the configured operator
    /// (`registerValidator` proof). None when no operator is configured.
    pub fn registration_proof(&self) -> Option<Vec<u8>> {
        self.operator.map(|op| {
            crate::crypto::sign_validator_registration(op, self.identity, &self.signing_key)
        })
    }

    pub fn identity_json(&self) -> serde_json::Value {
        serde_json::json!({
            "identity": format!("0x{}", hex::encode(self.identity.as_slice())),
            "operator": self.operator.map(|o| format!("0x{}", hex::encode(o.as_slice()))),
            "registrationProof": self.registration_proof().map(|p| format!("0x{}", hex::encode(p))),
            "version": self.version,
        })
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn key_path(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("mersennet-identity-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir.join("keys").join("node_key.json")
    }
    fn mode(path: &Path) -> u32 {
        fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn a_new_node_key_is_owner_only_and_loads_back_as_the_same_identity() {
        let path = key_path("new");
        let first = load_or_create_identity(&path).unwrap();
        assert_eq!(mode(&path), 0o600);
        assert_eq!(
            load_or_create_identity(&path).unwrap().address,
            first.address
        );
    }

    #[test]
    fn a_key_readable_by_others_is_made_owner_only_when_loaded() {
        let path = key_path("loose");
        let first = load_or_create_identity(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(
            load_or_create_identity(&path).unwrap().address,
            first.address
        );
        assert_eq!(mode(&path), 0o600);
    }

    #[test]
    fn a_key_file_that_cannot_be_read_is_never_replaced() {
        let path = key_path("unreadable");
        load_or_create_identity(&path).unwrap();
        let before = fs::read(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).unwrap();
        if fs::read(&path).is_ok() {
            return; // running as root: the file stays readable, nothing to check
        }
        assert!(load_or_create_identity(&path).is_err());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(fs::read(&path).unwrap(), before);
    }
}
