//! Genesis ceremony tool for Mersennet testnet.
//! Generates validator keys, genesis config, and Docker Compose for N validators.

use std::fs;
use std::path::Path;

const DEFAULT_VALIDATORS: u32 = 4;
/// Testnet chain id — the Mersenne prime 2^17 − 1 (mainnet uses 8191 = 2^13 − 1).
const DEFAULT_CHAIN_ID: u64 = 131_071;
const DEFAULT_OUTPUT_DIR: &str = "genesis-output";
const TOKENS_PER_VALIDATOR: &str = "10000000000000000000000000"; // 10M MRSN (18 decimals)
const STAKE_PER_VALIDATOR: &str = "1000000000000000000000000"; // 1M MRSN staked

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_args();
    let output_dir = Path::new(&args.output_dir);
    fs::create_dir_all(output_dir)?;

    let configs_dir = output_dir.join("configs");
    fs::create_dir_all(&configs_dir)?;

    let keys_dir = output_dir.join("keys");
    fs::create_dir_all(&keys_dir)?;

    let n = args.validators;
    let chain_id = args.chain_id;

    println!(
        "==> Generating genesis for {} validators (chain_id={})",
        n, chain_id
    );

    let mut validator_addresses = Vec::new();

    for i in 1..=n {
        let (signing_key, address) = mersennet::crypto::generate_keypair();
        let key_path = keys_dir.join(format!("validator-{}.json", i));
        fs::write(
            &key_path,
            serde_json::to_string_pretty(&serde_json::json!({
                "private_key": format!("0x{}", hex::encode(signing_key.to_bytes())),
            }))?,
        )?;
        let addr_hex = format!("0x{}", hex::encode(address.as_slice()));
        validator_addresses.push(addr_hex.clone());
    }

    let mut genesis_accounts: Vec<serde_json::Value> = validator_addresses
        .iter()
        .map(|addr| {
            serde_json::json!({
                "address": addr,
                "balance": TOKENS_PER_VALIDATOR,
                "nonce": 0
            })
        })
        .collect();

    let genesis_validators: Vec<serde_json::Value> = validator_addresses
        .iter()
        .map(|addr| {
            serde_json::json!({
                "address": addr,
                "stake": STAKE_PER_VALIDATOR
            })
        })
        .collect();

    let (faucet_key, faucet_address) = mersennet::crypto::generate_keypair();
    let faucet_addr_hex = format!("0x{}", hex::encode(faucet_address.as_slice()));
    let faucet_key_path = keys_dir.join("faucet-key.json");
    fs::write(
        &faucet_key_path,
        serde_json::to_string_pretty(&serde_json::json!({
            "private_key": format!("0x{}", hex::encode(faucet_key.to_bytes())),
        }))?,
    )?;
    genesis_accounts.push(serde_json::json!({
        "address": faucet_addr_hex,
        "balance": "10000000000000000000000000",
        "nonce": 0
    }));

    let genesis = serde_json::json!({
        "accounts": genesis_accounts,
        "validators": genesis_validators
    });

    let genesis_path = output_dir.join("genesis.json");
    fs::write(&genesis_path, serde_json::to_string_pretty(&genesis)?)?;
    println!("  wrote {}", genesis_path.display());

    for i in 1..=n {
        let peers: Vec<String> = (1..=n)
            .filter(|&p| p != i)
            .map(|p| format!("validator-{}:9090", p))
            .collect();

        let config = serde_json::json!({
            "engine": {
                "chain_id": chain_id,
                "state_path": "/data/state",
                "gas_limit_per_block": 30000000
            },
            "mempool": {
                "max_total": 10000,
                "max_per_sender": 1000,
                "bump_bps": 1000
            },
            "rpc": {
                "enabled": true,
                "addr": "0.0.0.0:8545"
            },
            "p2p": {
                "listen": "0.0.0.0:9090",
                "peers": peers,
                "block_time_ms": 1000,
                "node_key_path": "/data/node_key.json",
                "peer_store_path": "/data/peers.json"
            },
            "slashing": {
                "double_sign_bps": 500,
                "timeout_bps": 100,
                "escalation_step_bps": 25,
                "escalation_max_bps": 1000,
                "round_timeout_ms": 500,
                "unbonding_period": 2
            },
            "token_economics": {
                "max_supply": "618970019642690137449562111",
                "initial_reward_per_block": "2305843009213693951",
                "halving_interval": 33550336
            },
            "genesis": genesis
        });

        let config_path = configs_dir.join(format!("validator-{}.json", i));
        fs::write(&config_path, serde_json::to_string_pretty(&config)?)?;
        println!("  wrote {}", config_path.display());
    }

    let rpc_peers: Vec<String> = (1..=n).map(|p| format!("validator-{}:9090", p)).collect();
    let rpc_config = serde_json::json!({
        "engine": {
            "chain_id": chain_id,
            "state_path": "/data/state",
            "gas_limit_per_block": 30000000
        },
        "mempool": {
            "max_total": 10000,
            "max_per_sender": 1000,
            "bump_bps": 1000
        },
        "rpc": {
            "enabled": true,
            "addr": "0.0.0.0:8545"
        },
        "p2p": {
            "listen": "0.0.0.0:9090",
            "peers": rpc_peers,
            "block_time_ms": 1000,
            "node_key_path": "/data/node_key.json",
            "peer_store_path": "/data/peers.json"
        },
        "slashing": {
            "double_sign_bps": 500,
            "timeout_bps": 100,
            "escalation_step_bps": 25,
            "escalation_max_bps": 1000,
            "round_timeout_ms": 500,
            "unbonding_period": 2
        },
        "token_economics": {
            "max_supply": "618970019642690137449562111",
            "initial_reward_per_block": "2305843009213693951",
            "halving_interval": 33550336
        },
        "genesis": genesis
    });
    let rpc_config_path = configs_dir.join("rpc-node.json");
    fs::write(&rpc_config_path, serde_json::to_string_pretty(&rpc_config)?)?;
    println!("  wrote {}", rpc_config_path.display());

    let docker_compose = generate_docker_compose(n, chain_id);
    let compose_path = output_dir.join("docker-compose.yml");
    fs::write(&compose_path, docker_compose)?;
    println!("  wrote {}", compose_path.display());

    println!();
    println!("==> Genesis ceremony complete!");
    println!();
    println!("Summary:");
    println!("  Validators: {}", n);
    println!("  Chain ID:   {}", chain_id);
    println!("  Output:     {}", output_dir.display());
    println!();
    println!("Validator addresses (funded with 1M tokens each):");
    for (i, addr) in validator_addresses.iter().enumerate() {
        println!("  validator-{}: {}", i + 1, addr);
    }
    println!("  faucet:     {} (for testnet faucet)", faucet_addr_hex);
    println!();
    println!("To start the testnet:");
    println!(
        "  cd {} && docker compose up -d --build",
        output_dir.display()
    );
    println!();

    Ok(())
}

struct Args {
    validators: u32,
    chain_id: u64,
    output_dir: String,
}

fn parse_args() -> Args {
    let mut validators = DEFAULT_VALIDATORS;
    let mut chain_id = DEFAULT_CHAIN_ID;
    let mut output_dir = DEFAULT_OUTPUT_DIR.to_string();

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--validators" => {
                if let Some(v) = args.next() {
                    validators = v.parse().unwrap_or(DEFAULT_VALIDATORS);
                }
            }
            "--chain-id" => {
                if let Some(c) = args.next() {
                    chain_id = c.parse().unwrap_or(DEFAULT_CHAIN_ID);
                }
            }
            "--output-dir" => {
                if let Some(d) = args.next() {
                    output_dir = d;
                }
            }
            "--help" | "-h" => {
                println!("Usage: genesis [OPTIONS]");
                println!(
                    "  --validators N    Number of validators (default: {})",
                    DEFAULT_VALIDATORS
                );
                println!(
                    "  --chain-id ID    Chain ID (default: {})",
                    DEFAULT_CHAIN_ID
                );
                println!(
                    "  --output-dir DIR Output directory (default: {})",
                    DEFAULT_OUTPUT_DIR
                );
                std::process::exit(0);
            }
            _ => {}
        }
    }

    Args {
        validators,
        chain_id,
        output_dir,
    }
}

fn generate_docker_compose(n: u32, _chain_id: u64) -> String {
    let mut services = String::new();
    let mut volumes = String::new();

    for i in 1..=n {
        let port_rpc = 8544 + i;
        let port_p2p = 9089 + i;
        services.push_str(&format!(
            r#"  validator-{}:
    build: ..
    container_name: mersennet-validator-{}
    command: ["--config", "/etc/mersennet/config.json", "--validator", "--rpc"]
    volumes:
      - ./configs/validator-{}.json:/etc/mersennet/config.json:ro
      - ./keys/validator-{}.json:/data/node_key.json:ro
      - validator-{}-data:/data
    ports:
      - "{}:8545"
      - "{}:9090"
    networks:
      - mersennet-testnet
    healthcheck:
      test: ["CMD-SHELL", "curl -sf http://localhost:8545/health || exit 1"]
      interval: 10s
      timeout: 5s
      retries: 5
      start_period: 30s
    restart: unless-stopped
    environment:
      - RUST_LOG=info

"#,
            i, i, i, i, i, port_rpc, port_p2p
        ));
        volumes.push_str(&format!("  validator-{}-data:\n", i));
    }

    format!(
        r#"version: "3.9"

services:
{}
volumes:
{}
networks:
  mersennet-testnet:
    driver: bridge
"#,
        services, volumes
    )
}
