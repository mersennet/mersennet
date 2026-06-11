//! Noir circuit proof envelope and verifier trait.
//!
//! The chain's hot path is verification (every block verifies many
//! proofs); proving happens client-side. This module gives both sides
//! a typed envelope and a stable error surface.
//!
//! When the `prover` feature is enabled at the workspace level, the
//! verifier dispatches to a real Barretenberg verifier via FFI. With
//! the feature off (the default workspace build), the verifier uses a
//! deterministic mock that accepts proofs whose `proof_bytes` is the
//! Poseidon hash of `public_inputs`. The mock matches the prover in
//! lockstep so unit tests cover the integration surfaces of the
//! shielded engine without depending on Barretenberg in CI.

use crate::field::Fr;
use crate::poseidon::Poseidon;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

#[cfg(feature = "prover")]
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

/// Identifier for which Noir circuit produced a given proof.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Circuit {
    Spend,
    Output,
    JoinSplit,
    OrderPlace,
    LiquidateClaim,
    LiquidateExecute,
    /// Reserved for tests.
    #[doc(hidden)]
    Test,
}

impl Circuit {
    pub fn slug(self) -> &'static str {
        match self {
            Circuit::Spend => "spend",
            Circuit::Output => "output",
            Circuit::JoinSplit => "join_split",
            Circuit::OrderPlace => "order_place",
            Circuit::LiquidateClaim => "liquidate_claim",
            Circuit::LiquidateExecute => "liquidate_execute",
            Circuit::Test => "test",
        }
    }

    #[cfg(feature = "prover")]
    fn package_name(self) -> String {
        format!("mersennet_{}_circuit", self.slug())
    }

    #[cfg(feature = "prover")]
    fn all() -> &'static [Circuit] {
        &[
            Circuit::Spend,
            Circuit::Output,
            Circuit::JoinSplit,
            Circuit::OrderPlace,
            Circuit::LiquidateClaim,
            Circuit::LiquidateExecute,
        ]
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CircuitProof {
    pub circuit: Circuit,
    /// Canonical public-input field elements that the verifier must
    /// re-derive and compare.
    pub public_inputs: Vec<Fr>,
    /// Opaque proof bytes (Barretenberg UltraPlonk proof in production;
    /// Poseidon hash in the mock).
    pub proof_bytes: Vec<u8>,
    /// Verifying-key hash. The verifier rejects proofs whose vk_hash
    /// doesn't match the expected vk for `circuit`.
    pub vk_hash: [u8; 32],
}

#[derive(Debug, Error)]
pub enum VerifyError {
    #[error("circuit mismatch: expected {expected:?}, got {actual:?}")]
    CircuitMismatch { expected: Circuit, actual: Circuit },
    #[error("verifying-key mismatch")]
    VerifyingKeyMismatch,
    #[error("public-input length mismatch: expected {expected}, got {actual}")]
    PublicInputLength { expected: usize, actual: usize },
    #[error("public input #{index} mismatch")]
    PublicInputMismatch { index: usize },
    #[error("proof bytes failed cryptographic verification")]
    InvalidProof,
    #[error("proof backend is unavailable: {0}")]
    BackendUnavailable(String),
    #[error("proof backend failed: {0}")]
    BackendFailure(String),
}

pub trait Verifier: Send + Sync + std::fmt::Debug {
    /// Verify `proof` against `expected_public_inputs`. The caller is
    /// responsible for assembling `expected_public_inputs` from the
    /// chain's state (e.g. the current Merkle root, the order's
    /// market id, the oracle price).
    fn verify(
        &self,
        proof: &CircuitProof,
        expected_circuit: Circuit,
        expected_public_inputs: &[Fr],
    ) -> Result<(), VerifyError>;
}

#[derive(Debug, Error)]
pub enum ProveError {
    #[error("missing witness input `{0}`")]
    MissingWitnessInput(String),
    #[error("proof backend is unavailable: {0}")]
    BackendUnavailable(String),
    #[error("proof backend failed: {0}")]
    BackendFailure(String),
}

pub trait Prover: Send + Sync + std::fmt::Debug {
    /// Produce a proof for `circuit`, using `witness` as the full Noir
    /// input map and `public_inputs` as the canonical verifier contract.
    fn prove(
        &self,
        circuit: Circuit,
        public_inputs: Vec<Fr>,
        witness: &WitnessInputs,
    ) -> Result<CircuitProof, ProveError>;
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WitnessInputs {
    fields: BTreeMap<String, WitnessValue>,
}

impl WitnessInputs {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert<K, V>(&mut self, key: K, value: V)
    where
        K: Into<String>,
        V: Into<WitnessValue>,
    {
        self.fields.insert(key.into(), value.into());
    }

    #[cfg(feature = "prover")]
    fn to_toml(&self) -> String {
        let mut out = String::new();
        for (key, value) in &self.fields {
            out.push_str(key);
            out.push_str(" = ");
            out.push_str(&value.to_toml());
            out.push('\n');
        }
        out
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WitnessValue {
    Field(Fr),
    Array(Vec<WitnessValue>),
    Struct(BTreeMap<String, WitnessValue>),
}

impl WitnessValue {
    pub fn array(values: Vec<WitnessValue>) -> Self {
        Self::Array(values)
    }

    pub fn structure<K, V, I>(entries: I) -> Self
    where
        K: Into<String>,
        V: Into<WitnessValue>,
        I: IntoIterator<Item = (K, V)>,
    {
        let mut fields = BTreeMap::new();
        for (key, value) in entries {
            fields.insert(key.into(), value.into());
        }
        Self::Struct(fields)
    }

    #[cfg(feature = "prover")]
    fn to_toml(&self) -> String {
        match self {
            WitnessValue::Field(value) => format!("\"{}\"", encode_field_for_noir(value)),
            WitnessValue::Array(values) => {
                let encoded = values
                    .iter()
                    .map(WitnessValue::to_toml)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("[{encoded}]")
            }
            WitnessValue::Struct(fields) => {
                let encoded = fields
                    .iter()
                    .map(|(key, value)| format!("{key} = {}", value.to_toml()))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{{ {encoded} }}")
            }
        }
    }
}

impl From<Fr> for WitnessValue {
    fn from(value: Fr) -> Self {
        WitnessValue::Field(value)
    }
}

impl From<u64> for WitnessValue {
    fn from(value: u64) -> Self {
        WitnessValue::Field(Fr::from_u64(value))
    }
}

impl From<[u64; 32]> for WitnessValue {
    fn from(values: [u64; 32]) -> Self {
        WitnessValue::Array(values.into_iter().map(WitnessValue::from).collect())
    }
}

impl From<Vec<Fr>> for WitnessValue {
    fn from(values: Vec<Fr>) -> Self {
        WitnessValue::Array(values.into_iter().map(WitnessValue::from).collect())
    }
}

pub fn default_prover() -> Box<dyn Prover> {
    #[cfg(feature = "prover")]
    {
        if let Ok(prover) = BarretenbergProver::from_env() {
            return Box::new(prover);
        }
    }

    Box::new(MockProver::new())
}

pub fn default_verifier() -> Box<dyn Verifier> {
    #[cfg(feature = "prover")]
    {
        if let Ok(verifier) = BarretenbergVerifier::from_env() {
            return Box::new(verifier);
        }
    }

    Box::new(MockVerifier::new())
}

/// Mock prover used when the `prover` feature is off. It mirrors the
/// mock verifier so tests can exercise the proof boundary without an
/// external Noir / Barretenberg toolchain.
#[derive(Clone, Debug, Default)]
pub struct MockProver {
    poseidon: Poseidon,
}

impl MockProver {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Prover for MockProver {
    fn prove(
        &self,
        circuit: Circuit,
        public_inputs: Vec<Fr>,
        _witness: &WitnessInputs,
    ) -> Result<CircuitProof, ProveError> {
        let h = self.poseidon.hash_many(&public_inputs);
        Ok(CircuitProof {
            circuit,
            public_inputs,
            proof_bytes: h.to_bytes().to_vec(),
            vk_hash: MockVerifier::vk_hash_for(circuit),
        })
    }
}

/// Mock verifier used when the `prover` feature is off. Accepts proofs
/// whose `proof_bytes` is `Poseidon.hash_many(public_inputs).to_bytes()`
/// and whose `vk_hash` matches a per-circuit constant.
#[derive(Clone, Debug, Default)]
pub struct MockVerifier {
    poseidon: Poseidon,
}

impl MockVerifier {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn vk_hash_for(circuit: Circuit) -> [u8; 32] {
        use sha3::{Digest, Keccak256};
        let label = match circuit {
            Circuit::Spend => "MersennetChain-MockVK-Spend",
            Circuit::Output => "MersennetChain-MockVK-Output",
            Circuit::JoinSplit => "MersennetChain-MockVK-JoinSplit",
            Circuit::OrderPlace => "MersennetChain-MockVK-OrderPlace",
            Circuit::LiquidateClaim => "MersennetChain-MockVK-LiquidateClaim",
            Circuit::LiquidateExecute => "MersennetChain-MockVK-LiquidateExecute",
            Circuit::Test => "MersennetChain-MockVK-Test",
        };
        let mut h = Keccak256::new();
        h.update(label.as_bytes());
        let out = h.finalize();
        let mut buf = [0u8; 32];
        buf.copy_from_slice(&out);
        buf
    }

    /// Build a mock proof. Test/SDK callers use this until the real
    /// prover is available.
    pub fn prove(&self, circuit: Circuit, public_inputs: Vec<Fr>) -> CircuitProof {
        MockProver::new()
            .prove(circuit, public_inputs, &WitnessInputs::default())
            .expect("mock prover must not fail")
    }
}

impl Verifier for MockVerifier {
    fn verify(
        &self,
        proof: &CircuitProof,
        expected_circuit: Circuit,
        expected_public_inputs: &[Fr],
    ) -> Result<(), VerifyError> {
        if proof.circuit != expected_circuit {
            return Err(VerifyError::CircuitMismatch {
                expected: expected_circuit,
                actual: proof.circuit,
            });
        }
        if proof.vk_hash != Self::vk_hash_for(expected_circuit) {
            return Err(VerifyError::VerifyingKeyMismatch);
        }
        if proof.public_inputs.len() != expected_public_inputs.len() {
            return Err(VerifyError::PublicInputLength {
                expected: expected_public_inputs.len(),
                actual: proof.public_inputs.len(),
            });
        }
        for (i, (a, b)) in proof
            .public_inputs
            .iter()
            .zip(expected_public_inputs.iter())
            .enumerate()
        {
            if a != b {
                return Err(VerifyError::PublicInputMismatch { index: i });
            }
        }
        let expected = self.poseidon.hash_many(expected_public_inputs).to_bytes();
        if proof.proof_bytes != expected {
            return Err(VerifyError::InvalidProof);
        }
        Ok(())
    }
}

#[cfg(feature = "prover")]
#[derive(Debug, Error)]
pub enum NoirToolchainError {
    #[error("missing required environment variable {0}")]
    MissingEnv(&'static str),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("tool `{tool}` failed: {message}")]
    CommandFailed { tool: String, message: String },
    #[error("artifact is missing: {0}")]
    MissingArtifact(PathBuf),
    #[error("invalid vk hash contents in {path}: {message}")]
    InvalidVkHash { path: PathBuf, message: String },
}

#[cfg(feature = "prover")]
#[derive(Clone, Debug)]
pub struct CircuitArtifacts {
    pub circuit: Circuit,
    pub package_dir: PathBuf,
    pub artifacts_dir: PathBuf,
    pub vk_hash: [u8; 32],
}

#[cfg(feature = "prover")]
#[derive(Clone, Debug)]
pub struct NoirToolchain {
    circuits_dir: PathBuf,
    artifacts_dir: PathBuf,
    nargo_bin: String,
}

#[cfg(feature = "prover")]
impl NoirToolchain {
    pub fn from_env() -> Result<Self, NoirToolchainError> {
        let circuits_dir = env::var_os("MERSENNET_NOIR_CIRCUITS_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(default_circuits_dir);
        let artifacts_dir = env::var_os("MERSENNET_NOIR_ARTIFACTS_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| default_artifacts_dir(&circuits_dir));
        let nargo_bin = env::var("MERSENNET_NARGO_BIN").unwrap_or_else(|_| "nargo".to_string());
        Ok(Self {
            circuits_dir,
            artifacts_dir,
            nargo_bin,
        })
    }

    pub fn compile_all(&self) -> Result<Vec<CircuitArtifacts>, NoirToolchainError> {
        let mut out = Vec::with_capacity(Circuit::all().len());
        for &circuit in Circuit::all() {
            out.push(self.compile_circuit(circuit)?);
        }
        Ok(out)
    }

    pub fn compile_circuit(
        &self,
        circuit: Circuit,
    ) -> Result<CircuitArtifacts, NoirToolchainError> {
        let package_dir = self.materialize_temp_package(circuit)?;
        let status = Command::new(&self.nargo_bin)
            .arg("compile")
            .current_dir(&package_dir)
            .output()?;
        if !status.status.success() {
            return Err(NoirToolchainError::CommandFailed {
                tool: self.nargo_bin.clone(),
                message: String::from_utf8_lossy(&status.stderr).trim().to_string(),
            });
        }

        let target_dir = package_dir.join("target");
        if !target_dir.exists() {
            return Err(NoirToolchainError::MissingArtifact(target_dir));
        }

        let destination = self.artifacts_dir.join(circuit.slug());
        if destination.exists() {
            fs::remove_dir_all(&destination)?;
        }
        fs::create_dir_all(&destination)?;
        copy_dir_recursive(&target_dir, &destination)?;

        let vk_hash = hash_directory(&destination)?;
        fs::write(destination.join("vk.hash"), hex::encode(vk_hash))?;

        Ok(CircuitArtifacts {
            circuit,
            package_dir,
            artifacts_dir: destination,
            vk_hash,
        })
    }

    fn materialize_temp_package(&self, circuit: Circuit) -> Result<PathBuf, NoirToolchainError> {
        let package_dir = unique_temp_dir(circuit.slug());
        let src_dir = package_dir.join("src");
        fs::create_dir_all(&src_dir)?;

        for entry in fs::read_dir(self.circuits_dir.join("src"))? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("nr") {
                let destination = src_dir.join(entry.file_name());
                fs::copy(path, destination)?;
            }
        }

        fs::write(
            package_dir.join("Nargo.toml"),
            format!(
                "[package]\nname = \"{}\"\ntype = \"bin\"\nauthors = [\"MersennetNumbers Labs\"]\ncompiler_version = \">=0.30.0\"\n\n[dependencies]\n",
                circuit.package_name()
            ),
        )?;
        fs::write(src_dir.join("main.nr"), wrapper_source(circuit))?;
        Ok(package_dir)
    }
}

#[cfg(feature = "prover")]
#[derive(Clone, Debug)]
pub struct BarretenbergVerifier {
    artifacts_dir: PathBuf,
    verify_adapter: String,
}

#[cfg(feature = "prover")]
#[derive(Clone, Debug)]
pub struct BarretenbergProver {
    toolchain: NoirToolchain,
    prove_adapter: String,
}

#[cfg(feature = "prover")]
impl BarretenbergVerifier {
    pub fn from_env() -> Result<Self, NoirToolchainError> {
        let artifacts_dir = env::var_os("MERSENNET_NOIR_ARTIFACTS_DIR")
            .map(PathBuf::from)
            .ok_or(NoirToolchainError::MissingEnv(
                "MERSENNET_NOIR_ARTIFACTS_DIR",
            ))?;
        let verify_adapter = env::var("MERSENNET_BB_VERIFY_ADAPTER")
            .map_err(|_| NoirToolchainError::MissingEnv("MERSENNET_BB_VERIFY_ADAPTER"))?;
        Ok(Self {
            artifacts_dir,
            verify_adapter,
        })
    }

    fn expected_vk_hash(&self, circuit: Circuit) -> Result<[u8; 32], VerifyError> {
        let path = self.artifacts_dir.join(circuit.slug()).join("vk.hash");
        let raw = fs::read_to_string(&path).map_err(|_| {
            VerifyError::BackendUnavailable(format!(
                "missing vk hash metadata at {}",
                path.display()
            ))
        })?;
        parse_vk_hash(&path, raw.trim())
    }

    fn invoke_backend(
        &self,
        proof: &CircuitProof,
        expected_circuit: Circuit,
    ) -> Result<(), VerifyError> {
        let request_dir = unique_temp_dir(expected_circuit.slug());
        fs::create_dir_all(&request_dir).map_err(io_backend_error)?;

        let proof_path = request_dir.join("proof.bin");
        let public_inputs_path = request_dir.join("public_inputs.txt");
        fs::write(&proof_path, &proof.proof_bytes).map_err(io_backend_error)?;
        fs::write(
            &public_inputs_path,
            encode_public_inputs(&proof.public_inputs).join("\n"),
        )
        .map_err(io_backend_error)?;

        let circuit_artifacts = self.artifacts_dir.join(expected_circuit.slug());
        let output = Command::new(&self.verify_adapter)
            .arg("--circuit")
            .arg(expected_circuit.slug())
            .arg("--artifacts")
            .arg(&circuit_artifacts)
            .arg("--proof")
            .arg(&proof_path)
            .arg("--public-inputs")
            .arg(&public_inputs_path)
            .output()
            .map_err(io_backend_error)?;

        if output.status.success() {
            Ok(())
        } else {
            Err(VerifyError::BackendFailure(
                String::from_utf8_lossy(&output.stderr).trim().to_string(),
            ))
        }
    }
}

#[cfg(feature = "prover")]
impl BarretenbergProver {
    pub fn from_env() -> Result<Self, NoirToolchainError> {
        let toolchain = NoirToolchain::from_env()?;
        let prove_adapter = env::var("MERSENNET_BB_PROVE_ADAPTER")
            .map_err(|_| NoirToolchainError::MissingEnv("MERSENNET_BB_PROVE_ADAPTER"))?;
        Ok(Self {
            toolchain,
            prove_adapter,
        })
    }
}

#[cfg(feature = "prover")]
impl Prover for BarretenbergProver {
    fn prove(
        &self,
        circuit: Circuit,
        public_inputs: Vec<Fr>,
        witness: &WitnessInputs,
    ) -> Result<CircuitProof, ProveError> {
        let artifacts = self
            .toolchain
            .compile_circuit(circuit)
            .map_err(map_toolchain_error_to_prove)?;
        let witness_path = artifacts.package_dir.join("Prover.toml");
        fs::write(&witness_path, witness.to_toml()).map_err(io_prove_error)?;

        let proof_path = artifacts.package_dir.join("target").join("proof.bin");
        if let Some(parent) = proof_path.parent() {
            fs::create_dir_all(parent).map_err(io_prove_error)?;
        }

        let output = Command::new(&self.prove_adapter)
            .arg("--circuit")
            .arg(circuit.slug())
            .arg("--package-dir")
            .arg(&artifacts.package_dir)
            .arg("--artifacts")
            .arg(&artifacts.artifacts_dir)
            .arg("--witness")
            .arg(&witness_path)
            .arg("--proof")
            .arg(&proof_path)
            .output()
            .map_err(io_prove_error)?;

        if !output.status.success() {
            return Err(ProveError::BackendFailure(
                String::from_utf8_lossy(&output.stderr).trim().to_string(),
            ));
        }

        let proof_bytes = fs::read(&proof_path).map_err(|_| {
            ProveError::BackendFailure(format!(
                "proof adapter completed without creating {}",
                proof_path.display()
            ))
        })?;

        Ok(CircuitProof {
            circuit,
            public_inputs,
            proof_bytes,
            vk_hash: artifacts.vk_hash,
        })
    }
}

#[cfg(feature = "prover")]
impl Verifier for BarretenbergVerifier {
    fn verify(
        &self,
        proof: &CircuitProof,
        expected_circuit: Circuit,
        expected_public_inputs: &[Fr],
    ) -> Result<(), VerifyError> {
        if proof.circuit != expected_circuit {
            return Err(VerifyError::CircuitMismatch {
                expected: expected_circuit,
                actual: proof.circuit,
            });
        }
        if proof.public_inputs.len() != expected_public_inputs.len() {
            return Err(VerifyError::PublicInputLength {
                expected: expected_public_inputs.len(),
                actual: proof.public_inputs.len(),
            });
        }
        for (index, (actual, expected)) in proof
            .public_inputs
            .iter()
            .zip(expected_public_inputs.iter())
            .enumerate()
        {
            if actual != expected {
                return Err(VerifyError::PublicInputMismatch { index });
            }
        }

        let expected_vk_hash = self.expected_vk_hash(expected_circuit)?;
        if proof.vk_hash != expected_vk_hash {
            return Err(VerifyError::VerifyingKeyMismatch);
        }

        self.invoke_backend(proof, expected_circuit)
    }
}

#[cfg(feature = "prover")]
fn default_circuits_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("circuits")
}

#[cfg(feature = "prover")]
fn default_artifacts_dir(circuits_dir: &Path) -> PathBuf {
    circuits_dir
        .parent()
        .unwrap_or(circuits_dir)
        .join("params")
        .join("noir")
}

#[cfg(feature = "prover")]
fn io_backend_error(error: std::io::Error) -> VerifyError {
    VerifyError::BackendFailure(error.to_string())
}

#[cfg(feature = "prover")]
fn io_prove_error(error: std::io::Error) -> ProveError {
    ProveError::BackendFailure(error.to_string())
}

#[cfg(feature = "prover")]
fn map_toolchain_error_to_prove(error: NoirToolchainError) -> ProveError {
    match error {
        NoirToolchainError::MissingEnv(name) => {
            ProveError::BackendUnavailable(format!("missing required environment variable {name}"))
        }
        NoirToolchainError::Io(error) => ProveError::BackendFailure(error.to_string()),
        NoirToolchainError::CommandFailed { tool, message } => {
            ProveError::BackendFailure(format!("tool `{tool}` failed: {message}"))
        }
        NoirToolchainError::MissingArtifact(path) => {
            ProveError::BackendFailure(format!("missing artifact: {}", path.display()))
        }
        NoirToolchainError::InvalidVkHash { path, message } => ProveError::BackendFailure(format!(
            "invalid vk hash contents in {}: {message}",
            path.display()
        )),
    }
}

#[cfg(feature = "prover")]
fn encode_public_inputs(inputs: &[Fr]) -> Vec<String> {
    inputs
        .iter()
        .map(|value| hex::encode(value.to_bytes()))
        .collect()
}

#[cfg(feature = "prover")]
fn encode_field_for_noir(value: &Fr) -> String {
    use num_bigint::BigUint;

    BigUint::from_bytes_le(&value.to_bytes()).to_str_radix(10)
}

#[cfg(feature = "prover")]
fn unique_temp_dir(label: &str) -> PathBuf {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    env::temp_dir().join(format!("mersennet-{label}-{}-{now}", std::process::id()))
}

#[cfg(feature = "prover")]
fn copy_dir_recursive(source: &Path, destination: &Path) -> Result<(), NoirToolchainError> {
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let path = entry.path();
        let dest = destination.join(entry.file_name());
        if path.is_dir() {
            fs::create_dir_all(&dest)?;
            copy_dir_recursive(&path, &dest)?;
        } else {
            fs::copy(&path, &dest)?;
        }
    }
    Ok(())
}

#[cfg(feature = "prover")]
fn hash_directory(path: &Path) -> Result<[u8; 32], NoirToolchainError> {
    let mut files = Vec::new();
    collect_files(path, path, &mut files)?;
    files.sort_by(|a, b| a.0.cmp(&b.0));

    use sha3::{Digest, Keccak256};
    let mut hasher = Keccak256::new();
    for (relative, bytes) in files {
        hasher.update(relative.as_bytes());
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(&bytes);
    }
    let digest = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    Ok(out)
}

#[cfg(feature = "prover")]
fn collect_files(
    root: &Path,
    current: &Path,
    out: &mut Vec<(String, Vec<u8>)>,
) -> Result<(), NoirToolchainError> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(root, &path, out)?;
        } else {
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            out.push((relative, fs::read(&path)?));
        }
    }
    Ok(())
}

#[cfg(feature = "prover")]
fn parse_vk_hash(path: &Path, raw: &str) -> Result<[u8; 32], VerifyError> {
    let bytes = hex::decode(raw).map_err(|error| {
        VerifyError::BackendFailure(format!("invalid vk hash in {}: {error}", path.display()))
    })?;
    if bytes.len() != 32 {
        return Err(VerifyError::BackendFailure(format!(
            "invalid vk hash length in {}: expected 32 bytes, got {}",
            path.display(),
            bytes.len()
        )));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}

#[cfg(feature = "prover")]
fn wrapper_source(circuit: Circuit) -> &'static str {
    match circuit {
        Circuit::Spend => {
            r#"mod lib;
mod poseidon;
mod merkle;
mod note;
mod spend;

use crate::spend;

fn main(
    root: pub Field,
    nullifier: pub Field,
    new_commitment: pub Field,
    public_amount: pub Field,
    spent_note: spend::SpentNote,
    output_note: spend::OutputNote,
    merkle_path: [Field; 32],
    merkle_index_bits: [Field; 32],
    spend_sk: Field,
) {
    spend::verify(
        root,
        nullifier,
        new_commitment,
        public_amount,
        spent_note,
        output_note,
        merkle_path,
        merkle_index_bits,
        spend_sk,
    );
}
"#
        }
        Circuit::Output => {
            r#"mod lib;
mod poseidon;
mod merkle;
mod note;
mod output;

use crate::note::Note;

fn main(
    commitment: pub Field,
    asset_id: pub Field,
    public_amount: pub Field,
    note: Note,
) {
    output::verify(commitment, asset_id, public_amount, note);
}
"#
        }
        Circuit::JoinSplit => {
            r#"mod lib;
mod poseidon;
mod merkle;
mod note;
mod join_split;

use crate::note::Note;

fn main(
    root: pub Field,
    nullifier1: pub Field,
    nullifier2: pub Field,
    commitment1: pub Field,
    commitment2: pub Field,
    public_amount: pub Field,
    input1: Note,
    input2: Note,
    input1_path: [Field; 32],
    input1_index_bits: [Field; 32],
    input2_path: [Field; 32],
    input2_index_bits: [Field; 32],
    output1: Note,
    output2: Note,
    spend_sk: Field,
    input2_used: Field,
    output2_used: Field,
) {
    join_split::verify(
        root,
        nullifier1,
        nullifier2,
        commitment1,
        commitment2,
        public_amount,
        input1,
        input2,
        input1_path,
        input1_index_bits,
        input2_path,
        input2_index_bits,
        output1,
        output2,
        spend_sk,
        input2_used,
        output2_used,
    );
}
"#
        }
        Circuit::OrderPlace => {
            r#"mod lib;
mod poseidon;
mod merkle;
mod note;
mod order_place;

use crate::note::Note;

fn main(
    root: pub Field,
    nullifier: pub Field,
    new_commitment: pub Field,
    market_id: pub Field,
    side_hash: pub Field,
    price_band: pub Field,
    size_band: pub Field,
    oracle_price: pub Field,
    imm_required: pub Field,
    spent: Note,
    output: Note,
    spent_path: [Field; 32],
    spent_index_bits: [Field; 32],
    spend_sk: Field,
    side_salt: Field,
    side: Field,
) {
    order_place::verify(
        root,
        nullifier,
        new_commitment,
        market_id,
        side_hash,
        price_band,
        size_band,
        oracle_price,
        imm_required,
        spent,
        output,
        spent_path,
        spent_index_bits,
        spend_sk,
        side_salt,
        side,
    );
}
"#
        }
        Circuit::LiquidateClaim => {
            r#"mod lib;
mod poseidon;
mod merkle;
mod note;
mod liquidate_claim;

use crate::note::Note;

fn main(
    root: pub Field,
    market_id: pub Field,
    oracle_price: pub Field,
    liquidator_id: pub Field,
    claim_tag: pub Field,
    victim: Note,
    victim_path: [Field; 32],
    victim_index_bits: [Field; 32],
    notional: Field,
    maintenance_required: Field,
    equity: Field,
) {
    liquidate_claim::verify(
        root,
        market_id,
        oracle_price,
        liquidator_id,
        claim_tag,
        victim,
        victim_path,
        victim_index_bits,
        notional,
        maintenance_required,
        equity,
    );
}
"#
        }
        Circuit::LiquidateExecute => {
            r#"mod lib;
mod poseidon;
mod merkle;
mod note;
mod liquidate_execute;

use crate::note::Note;

fn main(
    root: pub Field,
    victim_nullifier: pub Field,
    bounty_commitment: pub Field,
    insurance_commitment: pub Field,
    winning_bid: pub Field,
    market_id: pub Field,
    oracle_price: pub Field,
    victim: Note,
    victim_path: [Field; 32],
    victim_index_bits: [Field; 32],
    victim_spend_sk: Field,
    bounty: Note,
    insurance: Note,
) {
    liquidate_execute::verify(
        root,
        victim_nullifier,
        bounty_commitment,
        insurance_commitment,
        winning_bid,
        market_id,
        oracle_price,
        victim,
        victim_path,
        victim_index_bits,
        victim_spend_sk,
        bounty,
        insurance,
    );
}
"#
        }
        Circuit::Test => {
            r#"fn main(x: pub Field) {
    assert(x == x);
}
"#
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_prove_then_verify_round_trip() {
        let v = MockVerifier::new();
        let inputs = vec![Fr::from_u64(1), Fr::from_u64(2), Fr::from_u64(3)];
        let proof = v.prove(Circuit::Test, inputs.clone());
        v.verify(&proof, Circuit::Test, &inputs).unwrap();
    }

    #[test]
    fn mock_rejects_wrong_circuit() {
        let v = MockVerifier::new();
        let inputs = vec![Fr::from_u64(1)];
        let proof = v.prove(Circuit::Spend, inputs.clone());
        let err = v
            .verify(&proof, Circuit::Output, &inputs)
            .expect_err("should fail");
        assert!(matches!(err, VerifyError::CircuitMismatch { .. }));
    }

    #[test]
    fn mock_rejects_tampered_public_input() {
        let v = MockVerifier::new();
        let inputs = vec![Fr::from_u64(1), Fr::from_u64(2)];
        let mut proof = v.prove(Circuit::Spend, inputs.clone());
        proof.public_inputs[1] = Fr::from_u64(3);
        let err = v
            .verify(&proof, Circuit::Spend, &inputs)
            .expect_err("should fail");
        assert!(matches!(err, VerifyError::PublicInputMismatch { .. }));
    }

    #[test]
    fn mock_rejects_tampered_proof_bytes() {
        let v = MockVerifier::new();
        let inputs = vec![Fr::from_u64(42)];
        let mut proof = v.prove(Circuit::Spend, inputs.clone());
        proof.proof_bytes[0] ^= 0xff;
        let err = v
            .verify(&proof, Circuit::Spend, &inputs)
            .expect_err("should fail");
        assert!(matches!(err, VerifyError::InvalidProof));
    }

    #[test]
    fn circuit_slugs_are_stable() {
        assert_eq!(Circuit::Spend.slug(), "spend");
        assert_eq!(Circuit::LiquidateExecute.slug(), "liquidate_execute");
    }

    #[cfg(feature = "prover")]
    #[test]
    fn witness_inputs_render_nested_toml() {
        let witness = WitnessInputs {
            fields: BTreeMap::from([
                ("root".to_string(), Fr::from_u64(1).into()),
                (
                    "note".to_string(),
                    WitnessValue::structure([
                        ("value_lo", Fr::from_u64(2)),
                        ("value_hi", Fr::from_u64(0)),
                    ]),
                ),
                (
                    "path".to_string(),
                    WitnessValue::array(vec![Fr::from_u64(3).into(), Fr::from_u64(4).into()]),
                ),
            ]),
        };

        let rendered = witness.to_toml();
        assert!(rendered.contains("root = \"1\""));
        assert!(rendered.contains("note = { value_hi = \"0\", value_lo = \"2\" }"));
        assert!(rendered.contains("path = [\"3\", \"4\"]"));
    }
}
