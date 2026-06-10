use anyhow::{Result, anyhow};
use k256::ecdsa::{RecoveryId, Signature, VerifyingKey};
use revm::primitives::{Address, B256, U256, keccak256};
use sha3::{Digest, Keccak256};

use crate::engine::Transaction;
use revm::primitives::Bytes;

use super::SignedTransaction;

/// Decode a standard Ethereum signed transaction (legacy or typed envelope).
pub fn decode_ethereum_tx(bytes: &[u8]) -> Result<SignedTransaction> {
    if bytes.is_empty() {
        return Err(anyhow!("empty transaction"));
    }

    match bytes[0] {
        0x01 => decode_eip2930(&bytes[1..]),
        0x02 => decode_eip1559(&bytes[1..]),
        // Mersennet shielded transaction (EIP-2718 type byte 0x7E).
        // See ADR-018 + `shielded_evm::SHIELDED_TX_TYPE`.
        0x7E => decode_shielded(&bytes[1..]),
        b if b >= 0xc0 => decode_legacy(bytes),
        _ => Err(anyhow!("unknown transaction type: 0x{:02x}", bytes[0])),
    }
}

/// Decode a Prime-Chain shielded transaction. Format:
///   `0x7E || RLP([chain_id, nonce, from, payload_bytes, y_parity, r, s])`
/// where `payload_bytes` is the bincode-serialized
/// [`crate::shielded_evm::ShieldedEnvelope`]. Gas fields are absent:
/// shielded txs are bond-sponsored.
fn decode_shielded(payload: &[u8]) -> Result<SignedTransaction> {
    let items = rlp_decode_list(payload)?;
    if items.len() != 7 {
        return Err(anyhow!(
            "shielded tx expects 7 RLP items, got {}",
            items.len()
        ));
    }

    let chain_id = rlp_to_u64(&items[0])?;
    let nonce = rlp_to_u64(&items[1])?;
    if items[2].len() != 20 {
        return Err(anyhow!(
            "shielded tx: invalid from address length: {}",
            items[2].len()
        ));
    }
    let from = Address::from_slice(&items[2]);
    let envelope_bytes = items[3].clone();
    let envelope: crate::shielded_evm::ShieldedEnvelope = bincode::deserialize(&envelope_bytes)
        .map_err(|e| anyhow!("shielded tx: invalid payload encoding: {e}"))?;
    let y_parity = rlp_to_u64(&items[4])? as u8;
    let r = rlp_to_u256(&items[5]);
    let s = rlp_to_u256(&items[6]);

    let tx = Transaction {
        from,
        to: None,
        value: U256::ZERO,
        data: Bytes::new(),
        gas_limit: 0,
        gas_price: U256::ZERO,
        nonce,
        chain_id: Some(chain_id),
        signature: Some((r, s, y_parity as u64)),
        tx_type: 0x7E,
        shielded_payload: Some(envelope),
    };

    Ok(SignedTransaction {
        tx,
        v: U256::from(y_parity as u64),
        r,
        s,
        tx_type: 0x7E,
    })
}

/// Decode a legacy (type 0) RLP-encoded signed transaction.
/// Format: RLP([nonce, gasPrice, gasLimit, to, value, data, v, r, s])
fn decode_legacy(bytes: &[u8]) -> Result<SignedTransaction> {
    let items = rlp_decode_list(bytes)?;
    if items.len() != 9 {
        return Err(anyhow!(
            "legacy tx expects 9 RLP items, got {}",
            items.len()
        ));
    }

    let nonce = rlp_to_u64(&items[0])?;
    let gas_price = rlp_to_u256(&items[1]);
    let gas_limit = rlp_to_u64(&items[2])?;
    let to = rlp_to_address(&items[3]);
    let value = rlp_to_u256(&items[4]);
    let data = Bytes::from(items[5].clone());
    let v_raw = rlp_to_u64(&items[6])?;
    let r = rlp_to_u256(&items[7]);
    let s = rlp_to_u256(&items[8]);

    // EIP-155: v = recovery_id + 35 + chain_id * 2
    let (chain_id, recovery_byte) = if v_raw >= 35 {
        let chain_id = (v_raw - 35) / 2;
        let recovery_byte = (v_raw - 35 - chain_id * 2) as u8;
        (Some(chain_id), recovery_byte)
    } else if v_raw == 27 || v_raw == 28 {
        // Pre-EIP-155
        (None, (v_raw - 27) as u8)
    } else {
        return Err(anyhow!("invalid v value: {v_raw}"));
    };

    // Compute EIP-155 signing hash: keccak256(RLP([nonce, gasPrice, gasLimit, to, value, data, chainId, 0, 0]))
    let signing_hash = if let Some(cid) = chain_id {
        let mut sign_items: Vec<Vec<u8>> = items[..6].to_vec();
        sign_items.push(u64_to_rlp_bytes(cid));
        sign_items.push(vec![]); // 0
        sign_items.push(vec![]); // 0
        keccak256(rlp_encode_list(&sign_items))
    } else {
        // Pre-EIP-155: hash just the 6 fields
        keccak256(rlp_encode_list(&items[..6]))
    };

    let from = recover_from_hash(signing_hash, r, s, recovery_byte)?;

    let tx = Transaction {
        from,
        to,
        value,
        data,
        gas_limit,
        gas_price,
        nonce,
        chain_id,
        signature: Some((r, s, v_raw)),
        tx_type: 0,
        shielded_payload: None,
    };

    Ok(SignedTransaction {
        tx,
        v: U256::from(v_raw),
        r,
        s,
        tx_type: 0,
    })
}

/// Decode an EIP-2930 (type 1) transaction.
/// Format: 0x01 || RLP([chainId, nonce, gasPrice, gasLimit, to, value, data, accessList, signatureYParity, signatureR, signatureS])
fn decode_eip2930(payload: &[u8]) -> Result<SignedTransaction> {
    let items = rlp_decode_list(payload)?;
    if items.len() != 11 {
        return Err(anyhow!(
            "EIP-2930 tx expects 11 RLP items, got {}",
            items.len()
        ));
    }

    let chain_id = rlp_to_u64(&items[0])?;
    let nonce = rlp_to_u64(&items[1])?;
    let gas_price = rlp_to_u256(&items[2]);
    let gas_limit = rlp_to_u64(&items[3])?;
    let to = rlp_to_address(&items[4]);
    let value = rlp_to_u256(&items[5]);
    let data = Bytes::from(items[6].clone());
    // items[7] = accessList (ignored for now)
    let y_parity = rlp_to_u64(&items[8])? as u8;
    let r = rlp_to_u256(&items[9]);
    let s = rlp_to_u256(&items[10]);

    // Signing hash: keccak256(0x01 || RLP([chainId, nonce, gasPrice, gasLimit, to, value, data, accessList]))
    let inner_rlp = rlp_encode_list(&items[..8]);
    let mut sign_payload = vec![0x01u8];
    sign_payload.extend_from_slice(&inner_rlp);
    let signing_hash = keccak256(sign_payload);

    let from = recover_from_hash(signing_hash, r, s, y_parity)?;

    let v_val = y_parity as u64;
    let tx = Transaction {
        from,
        to,
        value,
        data,
        gas_limit,
        gas_price,
        nonce,
        chain_id: Some(chain_id),
        signature: Some((r, s, v_val)),
        tx_type: 1,
        shielded_payload: None,
    };

    Ok(SignedTransaction {
        tx,
        v: U256::from(v_val),
        r,
        s,
        tx_type: 1,
    })
}

/// Decode an EIP-1559 (type 2) transaction.
/// Format: 0x02 || RLP([chainId, nonce, maxPriorityFeePerGas, maxFeePerGas, gasLimit, to, value, data, accessList, signatureYParity, signatureR, signatureS])
fn decode_eip1559(payload: &[u8]) -> Result<SignedTransaction> {
    let items = rlp_decode_list(payload)?;
    if items.len() != 12 {
        return Err(anyhow!(
            "EIP-1559 tx expects 12 RLP items, got {}",
            items.len()
        ));
    }

    let chain_id = rlp_to_u64(&items[0])?;
    let nonce = rlp_to_u64(&items[1])?;
    let _max_priority_fee = rlp_to_u256(&items[2]);
    let max_fee_per_gas = rlp_to_u256(&items[3]);
    let gas_limit = rlp_to_u64(&items[4])?;
    let to = rlp_to_address(&items[5]);
    let value = rlp_to_u256(&items[6]);
    let data = Bytes::from(items[7].clone());
    // items[8] = accessList (ignored for now)
    let y_parity = rlp_to_u64(&items[9])? as u8;
    let r = rlp_to_u256(&items[10]);
    let s = rlp_to_u256(&items[11]);

    // Signing hash: keccak256(0x02 || RLP([chainId, nonce, maxPriorityFeePerGas, maxFeePerGas, gasLimit, to, value, data, accessList]))
    let inner_rlp = rlp_encode_list(&items[..9]);
    let mut sign_payload = vec![0x02u8];
    sign_payload.extend_from_slice(&inner_rlp);
    let signing_hash = keccak256(sign_payload);

    let from = recover_from_hash(signing_hash, r, s, y_parity)?;

    // Map maxFeePerGas to gas_price for internal representation
    let v_val = y_parity as u64;
    let tx = Transaction {
        from,
        to,
        value,
        data,
        gas_limit,
        gas_price: max_fee_per_gas,
        nonce,
        chain_id: Some(chain_id),
        signature: Some((r, s, v_val)),
        tx_type: 2,
        shielded_payload: None,
    };

    Ok(SignedTransaction {
        tx,
        v: U256::from(v_val),
        r,
        s,
        tx_type: 2,
    })
}

fn recover_from_hash(hash: B256, r: U256, s: U256, recovery_byte: u8) -> Result<Address> {
    let recovery_id = RecoveryId::try_from(recovery_byte)
        .map_err(|e| anyhow!("invalid recovery id {recovery_byte}: {e}"))?;

    let mut sig_bytes = [0u8; 64];
    sig_bytes[..32].copy_from_slice(&r.to_be_bytes::<32>());
    sig_bytes[32..64].copy_from_slice(&s.to_be_bytes::<32>());
    let signature = Signature::from_bytes((&sig_bytes).into())
        .map_err(|e| anyhow!("invalid signature: {e}"))?;

    let verifying_key =
        VerifyingKey::recover_from_prehash(hash.as_slice(), &signature, recovery_id)
            .map_err(|e| anyhow!("ECDSA recovery failed: {e}"))?;

    let uncompressed = verifying_key.to_encoded_point(false);
    let pub_bytes = &uncompressed.as_bytes()[1..];
    let addr_hash = Keccak256::digest(pub_bytes);
    Ok(Address::from_slice(&addr_hash[12..]))
}

// ---------------------------------------------------------------------------
// Minimal RLP codec (just enough for Ethereum transactions)
// ---------------------------------------------------------------------------

/// Decode a top-level RLP list into its raw-bytes items.
/// Nested lists (like accessList) are kept as their raw RLP bytes.
fn rlp_decode_list(input: &[u8]) -> Result<Vec<Vec<u8>>> {
    if input.is_empty() {
        return Err(anyhow!("empty RLP input"));
    }

    let (list_payload, _consumed) = decode_rlp_item(input)?;

    // The first byte should indicate a list
    let first = input[0];
    if first < 0xc0 {
        return Err(anyhow!(
            "expected RLP list, got string prefix 0x{:02x}",
            first
        ));
    }

    // Now decode items within the list payload
    let mut items = Vec::new();
    let mut offset = 0;
    while offset < list_payload.len() {
        let b = list_payload[offset];
        if b < 0x80 {
            // Single byte
            items.push(vec![b]);
            offset += 1;
        } else if b <= 0xb7 {
            // Short string: 0-55 bytes
            let len = (b - 0x80) as usize;
            if offset + 1 + len > list_payload.len() {
                return Err(anyhow!("RLP short string overflow"));
            }
            items.push(list_payload[offset + 1..offset + 1 + len].to_vec());
            offset += 1 + len;
        } else if b <= 0xbf {
            // Long string
            let len_bytes = (b - 0xb7) as usize;
            if offset + 1 + len_bytes > list_payload.len() {
                return Err(anyhow!("RLP long string length overflow"));
            }
            let len = be_bytes_to_usize(&list_payload[offset + 1..offset + 1 + len_bytes]);
            if offset + 1 + len_bytes + len > list_payload.len() {
                return Err(anyhow!("RLP long string data overflow"));
            }
            items.push(list_payload[offset + 1 + len_bytes..offset + 1 + len_bytes + len].to_vec());
            offset += 1 + len_bytes + len;
        } else if b <= 0xf7 {
            // Short list (keep as raw RLP for nested structures like accessList)
            let len = (b - 0xc0) as usize;
            if offset + 1 + len > list_payload.len() {
                return Err(anyhow!("RLP short list overflow"));
            }
            // For lists, push the raw content (so accessList stays as-is)
            items.push(list_payload[offset..offset + 1 + len].to_vec());
            offset += 1 + len;
        } else {
            // Long list
            let len_bytes = (b - 0xf7) as usize;
            if offset + 1 + len_bytes > list_payload.len() {
                return Err(anyhow!("RLP long list length overflow"));
            }
            let len = be_bytes_to_usize(&list_payload[offset + 1..offset + 1 + len_bytes]);
            if offset + 1 + len_bytes + len > list_payload.len() {
                return Err(anyhow!("RLP long list data overflow"));
            }
            items.push(list_payload[offset..offset + 1 + len_bytes + len].to_vec());
            offset += 1 + len_bytes + len;
        }
    }

    Ok(items)
}

/// Decode one RLP item, returning (payload_bytes, total_consumed_bytes).
fn decode_rlp_item(input: &[u8]) -> Result<(Vec<u8>, usize)> {
    if input.is_empty() {
        return Err(anyhow!("empty RLP item"));
    }
    let b = input[0];
    if b < 0x80 {
        Ok((vec![b], 1))
    } else if b <= 0xb7 {
        let len = (b - 0x80) as usize;
        if input.len() < 1 + len {
            return Err(anyhow!("RLP short string truncated"));
        }
        Ok((input[1..1 + len].to_vec(), 1 + len))
    } else if b <= 0xbf {
        let len_bytes = (b - 0xb7) as usize;
        if input.len() < 1 + len_bytes {
            return Err(anyhow!("RLP long string length truncated"));
        }
        let len = be_bytes_to_usize(&input[1..1 + len_bytes]);
        if input.len() < 1 + len_bytes + len {
            return Err(anyhow!("RLP long string data truncated"));
        }
        Ok((
            input[1 + len_bytes..1 + len_bytes + len].to_vec(),
            1 + len_bytes + len,
        ))
    } else if b <= 0xf7 {
        let len = (b - 0xc0) as usize;
        if input.len() < 1 + len {
            return Err(anyhow!("RLP short list truncated"));
        }
        Ok((input[1..1 + len].to_vec(), 1 + len))
    } else {
        let len_bytes = (b - 0xf7) as usize;
        if input.len() < 1 + len_bytes {
            return Err(anyhow!("RLP long list length truncated"));
        }
        let len = be_bytes_to_usize(&input[1..1 + len_bytes]);
        if input.len() < 1 + len_bytes + len {
            return Err(anyhow!("RLP long list data truncated"));
        }
        Ok((
            input[1 + len_bytes..1 + len_bytes + len].to_vec(),
            1 + len_bytes + len,
        ))
    }
}

/// Encode a list of raw-bytes items into an RLP list.
fn rlp_encode_list(items: &[Vec<u8>]) -> Vec<u8> {
    let mut payload = Vec::new();
    for item in items {
        // Check if item is already a raw RLP list (starts with 0xc0+)
        if !item.is_empty() && item[0] >= 0xc0 {
            // Already RLP-encoded list, include as-is
            payload.extend_from_slice(item);
        } else {
            payload.extend_from_slice(&rlp_encode_bytes(item));
        }
    }
    let mut out = Vec::new();
    encode_length(payload.len(), 0xc0, &mut out);
    out.extend_from_slice(&payload);
    out
}

fn rlp_encode_bytes(data: &[u8]) -> Vec<u8> {
    if data.len() == 1 && data[0] < 0x80 {
        return vec![data[0]];
    }
    let mut out = Vec::new();
    encode_length(data.len(), 0x80, &mut out);
    out.extend_from_slice(data);
    out
}

fn encode_length(len: usize, offset: u8, out: &mut Vec<u8>) {
    if len < 56 {
        out.push(offset + len as u8);
    } else {
        let len_bytes = usize_to_min_be_bytes(len);
        out.push(offset + 55 + len_bytes.len() as u8);
        out.extend_from_slice(&len_bytes);
    }
}

fn u64_to_rlp_bytes(val: u64) -> Vec<u8> {
    if val == 0 {
        return vec![];
    }
    let bytes = val.to_be_bytes();
    let start = bytes.iter().position(|&b| b != 0).unwrap_or(8);
    bytes[start..].to_vec()
}

fn be_bytes_to_usize(bytes: &[u8]) -> usize {
    let mut val = 0usize;
    for &b in bytes {
        val = (val << 8) | b as usize;
    }
    val
}

fn usize_to_min_be_bytes(val: usize) -> Vec<u8> {
    let bytes = val.to_be_bytes();
    let start = bytes
        .iter()
        .position(|&b| b != 0)
        .unwrap_or(bytes.len() - 1);
    bytes[start..].to_vec()
}

fn rlp_to_u64(bytes: &[u8]) -> Result<u64> {
    if bytes.is_empty() {
        return Ok(0);
    }
    if bytes.len() > 8 {
        return Err(anyhow!(
            "RLP value too large for u64: {} bytes",
            bytes.len()
        ));
    }
    let mut padded = [0u8; 8];
    padded[8 - bytes.len()..].copy_from_slice(bytes);
    Ok(u64::from_be_bytes(padded))
}

fn rlp_to_u256(bytes: &[u8]) -> U256 {
    if bytes.is_empty() {
        return U256::ZERO;
    }
    U256::from_be_slice(bytes)
}

fn rlp_to_address(bytes: &[u8]) -> Option<Address> {
    if bytes.is_empty() {
        return None;
    }
    if bytes.len() == 20 {
        Some(Address::from_slice(bytes))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rlp_encode_decode_roundtrip() {
        let items: Vec<Vec<u8>> = vec![
            vec![],           // nonce=0
            vec![0x01],       // gasPrice=1
            vec![0x52, 0x08], // gasLimit=21000
            vec![0xBB; 20],   // to
            vec![0x03, 0xe8], // value=1000
            vec![],           // data
            vec![0x1e, 0xef], // chainId=131071
            vec![],           // 0
            vec![],           // 0
        ];
        let encoded = rlp_encode_list(&items);
        let decoded = rlp_decode_list(&encoded).unwrap();
        assert_eq!(decoded.len(), 9);
        assert_eq!(decoded[0], vec![] as Vec<u8>);
        assert_eq!(decoded[3], vec![0xBB; 20]);
    }

    #[test]
    fn test_rlp_to_u64_empty() {
        assert_eq!(rlp_to_u64(&[]).unwrap(), 0);
    }

    #[test]
    fn test_rlp_to_u64_value() {
        assert_eq!(rlp_to_u64(&[0x52, 0x08]).unwrap(), 21000);
    }
}
