use std::collections::HashMap;

use revm::primitives::{Address, B256, KECCAK_EMPTY};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PublishedCodeAttestation {
    pub deployer: Address,
    pub code_hash: B256,
    pub metadata_uri: Option<String>,
    pub published_at_block: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CodePublicationRegistrySnapshot {
    #[serde(default)]
    pub deployments: Vec<([u8; 20], [u8; 20])>,
    #[serde(default)]
    pub published: Vec<PublishedCodeAttestationSnapshot>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PublishedCodeAttestationSnapshot {
    pub contract: [u8; 20],
    pub deployer: [u8; 20],
    pub code_hash: [u8; 32],
    #[serde(default)]
    pub metadata_uri: Option<String>,
    pub published_at_block: u64,
}

#[derive(Clone, Debug, Default)]
pub struct CodePublicationRegistry {
    deployments: HashMap<Address, Address>,
    published: HashMap<Address, PublishedCodeAttestation>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CodePublicationError {
    #[error("contract deployment not recorded")]
    UnknownContract,
    #[error("only the recorded deployer can publish this contract")]
    UnauthorizedPublisher,
    #[error("contract has no runtime bytecode")]
    NoRuntimeCode,
}

impl CodePublicationRegistry {
    pub fn record_deployment(&mut self, contract: Address, deployer: Address) {
        self.deployments.insert(contract, deployer);
        self.published.remove(&contract);
    }

    pub fn publish(
        &mut self,
        caller: Address,
        contract: Address,
        code_hash: B256,
        metadata_uri: Option<String>,
        published_at_block: u64,
    ) -> Result<(), CodePublicationError> {
        let deployer = self
            .deployments
            .get(&contract)
            .copied()
            .ok_or(CodePublicationError::UnknownContract)?;
        if deployer != caller {
            return Err(CodePublicationError::UnauthorizedPublisher);
        }
        if code_hash == KECCAK_EMPTY {
            return Err(CodePublicationError::NoRuntimeCode);
        }
        self.published.insert(
            contract,
            PublishedCodeAttestation {
                deployer,
                code_hash,
                metadata_uri,
                published_at_block,
            },
        );
        Ok(())
    }

    pub fn revoke(
        &mut self,
        caller: Address,
        contract: Address,
    ) -> Result<(), CodePublicationError> {
        let deployer = self
            .deployments
            .get(&contract)
            .copied()
            .ok_or(CodePublicationError::UnknownContract)?;
        if deployer != caller {
            return Err(CodePublicationError::UnauthorizedPublisher);
        }
        self.published.remove(&contract);
        Ok(())
    }

    pub fn attestation(&self, contract: Address) -> Option<PublishedCodeAttestation> {
        self.published.get(&contract).cloned()
    }

    pub fn snapshot(&self) -> CodePublicationRegistrySnapshot {
        let mut deployments: Vec<_> = self
            .deployments
            .iter()
            .map(|(contract, deployer)| (*contract, *deployer))
            .collect();
        deployments.sort_by_key(|(contract, _)| contract.as_slice().to_vec());

        let mut published: Vec<_> = self
            .published
            .iter()
            .map(|(contract, attestation)| {
                let mut contract_bytes = [0u8; 20];
                contract_bytes.copy_from_slice(contract.as_slice());
                let mut deployer_bytes = [0u8; 20];
                deployer_bytes.copy_from_slice(attestation.deployer.as_slice());
                let mut code_hash_bytes = [0u8; 32];
                code_hash_bytes.copy_from_slice(attestation.code_hash.as_slice());
                PublishedCodeAttestationSnapshot {
                    contract: contract_bytes,
                    deployer: deployer_bytes,
                    code_hash: code_hash_bytes,
                    metadata_uri: attestation.metadata_uri.clone(),
                    published_at_block: attestation.published_at_block,
                }
            })
            .collect();
        published.sort_by_key(|entry| entry.contract);

        CodePublicationRegistrySnapshot {
            deployments: deployments
                .into_iter()
                .map(|(contract, deployer)| {
                    let mut contract_bytes = [0u8; 20];
                    contract_bytes.copy_from_slice(contract.as_slice());
                    let mut deployer_bytes = [0u8; 20];
                    deployer_bytes.copy_from_slice(deployer.as_slice());
                    (contract_bytes, deployer_bytes)
                })
                .collect(),
            published,
        }
    }

    pub fn restore(snapshot: CodePublicationRegistrySnapshot) -> Self {
        let deployments = snapshot
            .deployments
            .into_iter()
            .map(|(contract, deployer)| {
                (
                    Address::from_slice(&contract),
                    Address::from_slice(&deployer),
                )
            })
            .collect();

        let published = snapshot
            .published
            .into_iter()
            .map(|entry| {
                let contract = Address::from_slice(&entry.contract);
                let attestation = PublishedCodeAttestation {
                    deployer: Address::from_slice(&entry.deployer),
                    code_hash: B256::from(entry.code_hash),
                    metadata_uri: entry.metadata_uri,
                    published_at_block: entry.published_at_block,
                };
                (contract, attestation)
            })
            .collect();

        Self {
            deployments,
            published,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(byte: u8) -> Address {
        Address::from_slice(&[byte; 20])
    }

    #[test]
    fn only_deployer_can_publish_or_revoke() {
        let contract = addr(0x11);
        let deployer = addr(0x22);
        let stranger = addr(0x33);
        let mut registry = CodePublicationRegistry::default();
        registry.record_deployment(contract, deployer);

        let err = registry
            .publish(stranger, contract, B256::from([0x44; 32]), None, 7)
            .expect_err("stranger should be rejected");
        assert_eq!(err, CodePublicationError::UnauthorizedPublisher);

        registry
            .publish(
                deployer,
                contract,
                B256::from([0x55; 32]),
                Some("ipfs://artifact".to_string()),
                9,
            )
            .expect("deployer publish succeeds");

        let attestation = registry.attestation(contract).expect("published");
        assert_eq!(attestation.deployer, deployer);
        assert_eq!(attestation.published_at_block, 9);

        let err = registry
            .revoke(stranger, contract)
            .expect_err("stranger revoke rejected");
        assert_eq!(err, CodePublicationError::UnauthorizedPublisher);

        registry
            .revoke(deployer, contract)
            .expect("deployer revoke works");
        assert!(registry.attestation(contract).is_none());
    }
}
