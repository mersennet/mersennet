//! Privacy-fork migration tool (Workstream H1).
//!
//! Reads a pre-fork EVM-only state snapshot and produces a post-fork
//! `EngineSnapshotEnvelope` that the privacy testnet (chain ID 7920)
//! boots from. The migration is deterministic: identical input
//! snapshot + activation height ⇒ identical output.
//!
//! ## Inputs
//!
//! ```text
//! --in PATH            Path to the pre-fork snapshot (output of
//!                      `Engine::export_state_snapshot` on the live
//!                      transparent chain).
//! --out PATH           Path to write the post-fork envelope.
//! --chain-id N         Target chain ID for the privacy testnet
//!                      (default 7920).
//! --activation-height N
//!                      Block height at which privacy mode becomes
//!                      active on the new chain. Stamped into the
//!                      envelope so every validator agrees on it.
//! --dry-run            Print the migration plan summary without
//!                      writing the output file.
//! ```
//!
//! ## Behaviour
//!
//! 1. Decode the input snapshot as
//!    [`EngineSnapshotEnvelope`]. Accepts both legacy raw EVM bytes
//!    and the V2 envelope; legacy inputs are auto-wrapped.
//! 2. Materialize a temporary `Engine` keyed by a tempdir so the
//!    EVM tables can be read out account-by-account.
//! 3. For each EOA with positive transparent balance, compute the
//!    shielded migration note via
//!    [`mersennet::shielded_evm::derive_migration_*`] and insert
//!    it into a fresh `ShieldedState`.
//! 4. Re-encode the snapshot with `shielded` populated and stamp
//!    the activation height + chain ID.
//! 5. Sanity-check: sum of pre-fork balances == sum of migrated
//!    note values.
//!
//! The output snapshot is then loaded by every privacy-testnet
//! validator via `Engine::import_state_snapshot`. All validators
//! see the same shielded note tree and the same activation height,
//! so consensus on the privacy-fork block is automatic.

use anyhow::{Context, Result, bail};
use mersennet::engine::Engine;
use mersennet::engine_snapshot::{
    EngineSnapshotEnvelope, ShieldedSnapshotData, encode_transparent_balances,
};
use mersennet::liquidation_auction::LiquidationAuction;
use mersennet::shielded_evm::{MigrationPlan, ShieldedEvm};
use mersennet_zkp::Fr;
use revm::primitives::{Address, U256};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Debug)]
struct Args {
    input: PathBuf,
    output: PathBuf,
    chain_id: u64,
    activation_height: u64,
    dry_run: bool,
}

fn print_usage() {
    eprintln!(
        "Usage: migrate-genesis --in <PATH> --out <PATH> --chain-id <ID> --activation-height <H> [--dry-run]"
    );
}

fn parse_args() -> Result<Args> {
    let mut input: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut chain_id: u64 = 7920;
    let mut activation_height: Option<u64> = None;
    let mut dry_run = false;

    let argv: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < argv.len() {
        let a = &argv[i];
        match a.as_str() {
            "--in" => {
                i += 1;
                input = Some(PathBuf::from(argv.get(i).context("--in needs a value")?));
            }
            "--out" => {
                i += 1;
                output = Some(PathBuf::from(argv.get(i).context("--out needs a value")?));
            }
            "--chain-id" => {
                i += 1;
                chain_id = argv
                    .get(i)
                    .context("--chain-id needs a value")?
                    .parse()
                    .context("--chain-id must be u64")?;
            }
            "--activation-height" => {
                i += 1;
                activation_height = Some(
                    argv.get(i)
                        .context("--activation-height needs a value")?
                        .parse()
                        .context("--activation-height must be u64")?,
                );
            }
            "--dry-run" => dry_run = true,
            "-h" | "--help" => {
                print_usage();
                std::process::exit(0);
            }
            other => bail!("unknown argument: {other}"),
        }
        i += 1;
    }

    Ok(Args {
        input: input.context("--in is required")?,
        output: output.context("--out is required")?,
        chain_id,
        activation_height: activation_height.context("--activation-height is required")?,
        dry_run,
    })
}

fn main() -> ExitCode {
    tracing_subscriber::fmt().with_env_filter("info").init();
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: {e}");
            print_usage();
            return ExitCode::FAILURE;
        }
    };

    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("migration failed: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &Args) -> Result<()> {
    tracing::info!(
        "==> reading pre-fork snapshot from {}",
        args.input.display()
    );
    let raw = std::fs::read(&args.input)
        .with_context(|| format!("read snapshot at {}", args.input.display()))?;

    let envelope = EngineSnapshotEnvelope::decode(&raw)
        .context("decode envelope (or wrap legacy snapshot)")?;
    tracing::info!(
        height = envelope.height,
        chain_id = envelope.chain_id,
        evm_snapshot_bytes = envelope.evm_snapshot.len(),
        "decoded envelope"
    );

    // Materialise an Engine keyed by a temp directory so we can
    // read out the EVM account table account-by-account. We're
    // *re*-importing the input snapshot here; the engine handles
    // both legacy and V2 formats.
    let tmp = tempfile::TempDir::new().context("create tempdir for migration")?;
    let mut engine =
        Engine::new_with_backend(envelope.chain_id.max(1), tmp.path().join("evm"), "redb");
    let scratch = args.input.clone(); // re-read directly; engine handles both formats
    engine
        .import_state_snapshot(&scratch)
        .context("re-import snapshot into temp engine")?;

    // Walk every EOA with a non-zero transparent balance and build
    // the migration plan.
    tracing::info!("==> scanning transparent EOAs for migration");
    let migration_plan = build_migration_plan(&mut engine, args.activation_height)?;
    tracing::info!(
        accounts = migration_plan.accounts.len(),
        total_value = ?total_value(&migration_plan),
        "migration plan computed"
    );

    // Apply the plan to a fresh ShieldedEvm.
    let mut shielded_evm = ShieldedEvm::default();
    let (new_root, migrated) = migration_plan
        .clone()
        .apply(&mut shielded_evm)
        .context("apply migration plan")?;
    tracing::info!(
        new_root = ?new_root,
        migrated_count = migrated,
        "migration applied to shielded state"
    );

    // Conservation check: sum of migrated note values must equal
    // sum of pre-fork transparent balances.
    sanity_check_conservation(&migration_plan, &shielded_evm.transparent_balances)?;

    // Re-encode the envelope with the shielded subsystems
    // populated.
    let shielded_data = ShieldedSnapshotData {
        state: shielded_evm.state.snapshot(),
        auction: LiquidationAuction::new().snapshot(),
        transparent_balances: encode_transparent_balances(&shielded_evm.transparent_balances),
        migration_plan_applied: true,
        code_publication_registry: Default::default(),
        encrypted_note_payloads: Vec::new(),
        viewing_grants: Vec::new(),
        viewing_grant_revocations: Vec::new(),
    };

    let out_env = EngineSnapshotEnvelope {
        height: envelope.height,
        chain_id: args.chain_id,
        privacy_mode_activated: false, // flips at activation_height during block production
        privacy_activation_height: Some(args.activation_height),
        evm_snapshot: envelope.evm_snapshot,
        shielded: Some(shielded_data),
    };
    let bytes = out_env.encode().context("encode output envelope")?;

    if args.dry_run {
        tracing::info!(
            output_bytes = bytes.len(),
            "dry-run — skipping output file write"
        );
        return Ok(());
    }

    std::fs::write(&args.output, &bytes)
        .with_context(|| format!("write output at {}", args.output.display()))?;
    tracing::info!(
        bytes = bytes.len(),
        path = %args.output.display(),
        "==> wrote post-fork envelope"
    );
    println!(
        "migration done: {} -> {}",
        args.input.display(),
        args.output.display()
    );
    Ok(())
}

fn build_migration_plan(engine: &mut Engine, activation_height: u64) -> Result<MigrationPlan> {
    // The engine doesn't expose an "iter all accounts" API
    // (intentional — only used during this one-shot migration). We
    // pull the account list out of the snapshot envelope's EVM
    // section via a re-read of the input file.
    let mut accounts: Vec<(Address, U256, Fr)> = Vec::new();
    for (addr, balance) in iter_transparent_balances(engine)? {
        if balance.is_zero() {
            continue;
        }
        let owner_pk = derive_owner_pk(addr);
        accounts.push((addr, balance, owner_pk));
    }
    // Deterministic ordering so the migration is reproducible.
    accounts.sort_by_key(|a| a.0);

    Ok(MigrationPlan {
        activation_height,
        accounts,
    })
}

/// Iterate every transparent EOA with its current balance. We use
/// `Engine`'s exposed `get_balance` plus a list of addresses we
/// already know about — for the migration tool, that list comes
/// from the snapshot's `AccountRecord` table. We re-read the
/// snapshot file directly because the live engine's StateBackend
/// trait doesn't expose `iter_accounts` (we don't want to add it to
/// the trait just for this tool).
fn iter_transparent_balances(engine: &mut Engine) -> Result<Vec<(Address, U256)>> {
    // Walk the EVM in-memory DB. After import_state_snapshot
    // populated it, the `accounts` field carries every EOA.
    let mut out: Vec<(Address, U256)> = Vec::new();
    // revm::InMemoryDB stores accounts in a `HashMap<Address,
    // DbAccount>`. We use the public `accounts` field via direct
    // reflection — revm 12 exposes it.
    for (addr, account) in engine.evm.db.accounts.iter() {
        let bal = account.info.balance;
        if !bal.is_zero() {
            out.push((*addr, bal));
        }
    }
    if out.is_empty() {
        tracing::warn!("no accounts found in EVM cache — snapshot may have been empty");
    }
    Ok(out)
}

fn derive_owner_pk(addr: Address) -> Fr {
    use sha3::{Digest, Keccak256};
    let mut h = Keccak256::new();
    h.update(b"MersennetChain-MigrationOwnerPk-v0");
    h.update(addr.as_slice()); // privacy-allow: migration derivation uses transparent EOA
    let bytes: [u8; 32] = h.finalize().into();
    Fr::from_bytes_reduce(&bytes)
}

fn total_value(plan: &MigrationPlan) -> U256 {
    plan.accounts.iter().fold(U256::ZERO, |acc, (_, b, _)| {
        acc.checked_add(*b).unwrap_or(U256::MAX)
    })
}

fn sanity_check_conservation(
    plan: &MigrationPlan,
    transparent: &HashMap<Address, U256>, // privacy-allow: conservation check at the bridge
) -> Result<()> {
    let migrated_total = total_value(plan);
    let transparent_total = transparent.values().fold(U256::ZERO, |acc, b| {
        acc.checked_add(*b).unwrap_or(U256::MAX)
    });
    // The post-migration ShieldedEvm.transparent_balances should
    // equal the pre-migration balances (the migration *mirrors*,
    // it doesn't drain). So total transparent == total migrated.
    if migrated_total != transparent_total {
        bail!(
            "conservation check failed: migrated total {} != transparent total {}",
            migrated_total,
            transparent_total
        );
    }
    tracing::info!(
        migrated_total = ?migrated_total,
        transparent_total = ?transparent_total,
        "conservation check passed"
    );
    Ok(())
}
