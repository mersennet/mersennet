use k256::ecdsa::SigningKey;
use prime_chain::config::{load_config, parse_address, parse_u256, AppConfig};
use prime_chain::consensus::ValidatorChange;
use prime_chain::engine::{Engine, Transaction};
use prime_chain::governance::{Governance, ProposalKind};
use prime_chain::identity::load_or_create_identity;
use prime_chain::net_transport::{GossipConfig, TcpSync, UdpGossip};
use prime_chain::network::{Message, RoundStage};
use prime_chain::p2p::{NetworkNode, Node, P2pMessage, P2pNetwork};
use prime_chain::{prometheus, rpc, ws};
use revm::primitives::{keccak256, Address, Bytes, U256};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tracing::info;
use tracing_subscriber;

fn main() -> anyhow::Result<()> {
    // initialize structured tracing from env and install Prometheus metrics
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let _prom = prometheus::init();
    // Emit a startup gauge so /metrics is non-empty on boot
    metrics::gauge!("prime_chain_up", 1.0);

    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_clone = Arc::clone(&shutdown);
    ctrlc::set_handler(move || {
        info!("shutting down...");
        shutdown_clone.store(true, Ordering::SeqCst);
    })?;

    let cli = read_cli_config();
    let mut app_config = match cli.config_path.as_deref() {
        Some(path) => load_config(path)?,
        None => AppConfig::default(),
    };
    apply_cli_overrides(&mut app_config, &cli);

    let identity = load_or_create_identity(&app_config.p2p.node_key_path)?;
    info!(
        address = format!("0x{}", hex::encode(identity.address.as_slice())),
        "node identity loaded"
    );

    let mut engine = Engine::new_with_backend(
        app_config.engine.chain_id,
        &app_config.engine.state_path,
        &app_config.engine.storage_backend,
    );
    engine.set_mempool_limits(
        app_config.mempool.max_total,
        app_config.mempool.max_per_sender,
        app_config.mempool.bump_bps,
    );
    engine.prime_orders_set_margin_params(
        app_config.prime_orders.initial_margin_bps,
        app_config.prime_orders.maintenance_margin_bps,
    );
    let bridge_limit = if app_config.bridge.max_queue_len == 0 {
        None
    } else {
        Some(app_config.bridge.max_queue_len)
    };
    engine.set_bridge_limits(bridge_limit);

    for account in &app_config.genesis.accounts {
        let address = parse_address(&account.address)?;
        let balance = parse_u256(&account.balance)?;
        engine.fund_account(address, balance, account.nonce);
    }

    engine.set_unbonding_period(app_config.slashing.unbonding_period);
    engine.set_fee_market_params(
        app_config.engine.gas_limit_per_block,
        app_config.engine.fee_elasticity_multiplier,
        app_config.engine.fee_max_change_denominator,
    );
    engine.set_round_timeout_ms(app_config.slashing.round_timeout_ms);
    engine.set_slashing_bps(
        app_config.slashing.double_sign_bps,
        app_config.slashing.timeout_bps,
    );
    engine.set_slashing_escalation(
        app_config.slashing.escalation_step_bps,
        app_config.slashing.escalation_max_bps,
    );
    let max_supply = parse_u256(&app_config.token_economics.max_supply)?;
    let initial_reward = parse_u256(&app_config.token_economics.initial_reward_per_block)?;
    engine.set_token_economics(
        max_supply,
        initial_reward,
        app_config.token_economics.halving_interval,
    );
    if handle_snapshot_cli(&mut engine, &cli)? {
        return Ok(());
    }

    info!("╔══════════════════════════════════════╗");
    info!("║        Prime Chain v7.0              ║");
    info!("║   Production Node Starting...        ║");
    info!("╚══════════════════════════════════════╝");
    info!(
        chain_id = app_config.engine.chain_id,
        storage = %app_config.engine.storage_backend,
        rpc = app_config.rpc.enabled,
        ws = app_config.ws.enabled,
        noise = app_config.p2p.noise_enabled,
        "node configuration"
    );

    match cli.mode {
        NodeMode::Devnet => {
            run_devnet_demo(&mut engine, max_supply, initial_reward, &app_config)?;
            if app_config.rpc.enabled {
                let engine = Arc::new(Mutex::new(engine));
                if app_config.ws.enabled {
                    let ws_manager = Arc::new(Mutex::new(ws::WsSubscriptionManager::new()));
                    let ws_addr = app_config.ws.addr.clone();
                    let ws_mgr = ws_manager.clone();
                    std::thread::Builder::new()
                        .name("ws-server".into())
                        .spawn(move || {
                            info!(addr = %ws_addr, "starting WebSocket server");
                            ws::WsServer::start(&ws_addr, ws_mgr);
                        })?;
                    info!("WebSocket subscriptions enabled on {}", app_config.ws.addr);
                }
                if let Some(path) = cli.config_path.clone() {
                    start_config_watcher(engine.clone(), path)?;
                }
                rpc::serve(engine, &app_config.rpc.addr)?;
            }
        }
        NodeMode::Full | NodeMode::Validator => {
            let is_validator = matches!(cli.mode, NodeMode::Validator);
            info!(
                mode = if is_validator { "validator" } else { "full" },
                listen = %app_config.p2p.listen,
                peers = app_config.p2p.peers.len(),
                "starting network node"
            );

            let gossip_config = GossipConfig {
                listen_addr: app_config.p2p.listen.clone(),
                bootstrap_peers: app_config.p2p.peers.clone(),
                ..GossipConfig::default()
            };

            let engine = Arc::new(Mutex::new(engine));
            let network = NetworkNode::new(&gossip_config)?;
            network.start_networking(engine.clone(), &gossip_config);

            if is_validator {
                let block_time =
                    std::time::Duration::from_millis(app_config.p2p.block_time_ms);
                let eng = engine.clone();
                let net = network.clone();
                let shutdown_producer = Arc::clone(&shutdown);
                let zk_enabled = app_config.zk.enabled;
                let zk_interval = app_config.zk.checkpoint_interval;
                std::thread::Builder::new()
                    .name("block-producer".into())
                    .spawn(move || {
                        info!(
                            block_time_ms = block_time.as_millis(),
                            "block production started"
                        );
                        loop {
                            if shutdown_producer.load(Ordering::SeqCst) {
                                break;
                            }
                            std::thread::sleep(block_time);
                            let block = {
                                let mut e = match eng.lock() {
                                    Ok(e) => e,
                                    Err(_) => break,
                                };
                                match e.execute_block() {
                                    Ok(b) => b,
                                    Err(err) => {
                                        tracing::warn!(%err, "block production error");
                                        continue;
                                    }
                                }
                            };
                            info!(
                                number = block.number,
                                txs = block.transactions.len(),
                                "produced block"
                            );
                            if let Err(err) = net.broadcast_block(&block) {
                                tracing::warn!(%err, "block broadcast error");
                            }
                            if block.number % 100 == 0 {
                                info!(
                                    height = block.number,
                                    txs_total = block.transactions.len(),
                                    state_root = %block.state_root,
                                    "health: node operational"
                                );
                            }
                            if zk_enabled && block.number % zk_interval == 0 {
                                tracing::debug!(
                                    height = block.number,
                                    "ZK proof checkpoint would be generated (mock)"
                                );
                            }
                        }
                    })?;
            }

            if app_config.ws.enabled {
                let ws_manager = Arc::new(Mutex::new(ws::WsSubscriptionManager::new()));
                let ws_addr = app_config.ws.addr.clone();
                let ws_mgr = ws_manager.clone();
                std::thread::Builder::new()
                    .name("ws-server".into())
                    .spawn(move || {
                        info!(addr = %ws_addr, "starting WebSocket server");
                        ws::WsServer::start(&ws_addr, ws_mgr);
                    })?;
                info!("WebSocket subscriptions enabled on {}", app_config.ws.addr);
            }

            if let Some(path) = cli.config_path.clone() {
                start_config_watcher(engine.clone(), path)?;
            }

            if app_config.rpc.enabled {
                rpc::serve(engine, &app_config.rpc.addr)?;
            } else {
                info!("node running (no RPC). Press Ctrl+C to stop.");
                loop {
                    if shutdown.load(Ordering::SeqCst) {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_secs(1));
                }
            }
        }
    }

    Ok(())
}

fn start_config_watcher(
    engine: Arc<Mutex<Engine>>,
    config_path: String,
) -> anyhow::Result<()> {
    let mut last_modified = std::fs::metadata(&config_path)
        .and_then(|meta| meta.modified())
        .ok();

    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(2));
        let Ok(meta) = std::fs::metadata(&config_path) else {
            continue;
        };
        let Ok(modified) = meta.modified() else {
            continue;
        };
        if last_modified.map(|prev| prev >= modified).unwrap_or(false) {
            continue;
        }
        last_modified = Some(modified);

        let Ok(config) = load_config(&config_path) else {
            continue;
        };
        if let Ok(mut engine) = engine.lock() {
            apply_runtime_config(&mut engine, &config);
        }
        tracing::info!(path = %config_path, "config hot-reloaded");
    });

    Ok(())
}

fn apply_runtime_config(engine: &mut Engine, config: &AppConfig) {
    engine.set_mempool_limits(
        config.mempool.max_total,
        config.mempool.max_per_sender,
        config.mempool.bump_bps,
    );
    engine.set_fee_market_params(
        config.engine.gas_limit_per_block,
        config.engine.fee_elasticity_multiplier,
        config.engine.fee_max_change_denominator,
    );
    engine.set_round_timeout_ms(config.slashing.round_timeout_ms);
    engine.set_slashing_bps(
        config.slashing.double_sign_bps,
        config.slashing.timeout_bps,
    );
    engine.set_slashing_escalation(
        config.slashing.escalation_step_bps,
        config.slashing.escalation_max_bps,
    );
    engine.prime_orders_set_margin_params(
        config.prime_orders.initial_margin_bps,
        config.prime_orders.maintenance_margin_bps,
    );
    let bridge_limit = if config.bridge.max_queue_len == 0 {
        None
    } else {
        Some(config.bridge.max_queue_len)
    };
    engine.set_bridge_limits(bridge_limit);
}

fn handle_snapshot_cli(engine: &mut Engine, cli: &CliConfig) -> anyhow::Result<bool> {
    if let Some(addr) = &cli.snapshot_listen {
        let tcp = TcpSync::bind(addr)?;
        info!("snapshot listen: {addr}");
        let mut stream = None;
        for _ in 0..50 {
            if let Some(accepted) = tcp.accept_once()? {
                stream = Some(accepted);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let Some(mut stream) = stream else {
            return Ok(true);
        };

        let height = engine.latest_height();
        let state_root = engine.evm.state.commit_state(
            &engine.evm.db,
            &engine.orders.state,
            &engine.bridge.orders_to_evm,
            &engine.bridge.evm_to_orders,
            height,
        )?;
        let snapshot = engine.evm.state.export_snapshot_bytes(height, state_root)?;
        let chunk_size = cli.snapshot_chunk_size.unwrap_or(256 * 1024);
        let _header = TcpSync::send_snapshot(&mut stream, &snapshot, chunk_size)?;
        info!("snapshot sent");
        return Ok(true);
    }

    if let Some(addr) = &cli.snapshot_fetch {
        let timeout = std::time::Duration::from_secs(5);
        let mut stream = TcpSync::connect(addr, timeout)?;
        let max_bytes = cli.snapshot_max_bytes.unwrap_or(256 * 1024 * 1024);
        let snapshot = TcpSync::recv_snapshot(&mut stream, max_bytes)?;
        if let Some(out) = &cli.snapshot_out {
            std::fs::write(out, &snapshot)?;
            info!("snapshot saved to {out}");
        }
        let meta = engine.evm.state.import_snapshot_bytes(&snapshot)?;
        engine.evm.db = revm::db::InMemoryDB::default();
        engine.evm.state.load_into_db(&mut engine.evm.db)?;
        engine.evm.state.load_prime_orders(&mut engine.orders.state)?;
        engine
            .evm
            .state
            .load_bridge_queues(&mut engine.bridge.orders_to_evm, &mut engine.bridge.evm_to_orders)?;
        engine.chain.clear();
        engine.block_number = meta.height.saturating_add(1);
        info!("snapshot imported at height {}", meta.height);
        return Ok(true);
    }

    Ok(false)
}

struct CliConfig {
    config_path: Option<String>,
    state_path: Option<String>,
    mempool_max: Option<usize>,
    mempool_per_sender: Option<usize>,
    mempool_bump_bps: Option<u64>,
    rpc_enabled: bool,
    rpc_addr: Option<String>,
    mode: NodeMode,
    snapshot_listen: Option<String>,
    snapshot_fetch: Option<String>,
    snapshot_out: Option<String>,
    snapshot_chunk_size: Option<usize>,
    snapshot_max_bytes: Option<usize>,
    node_key_path: Option<String>,
    peer_store_path: Option<String>,
}

fn read_cli_config() -> CliConfig {
    let mut config = CliConfig {
        config_path: None,
        state_path: None,
        mempool_max: None,
        mempool_per_sender: None,
        mempool_bump_bps: None,
        rpc_enabled: false,
        rpc_addr: None,
        mode: NodeMode::Devnet,
        snapshot_listen: None,
        snapshot_fetch: None,
        snapshot_out: None,
        snapshot_chunk_size: None,
        snapshot_max_bytes: None,
        node_key_path: None,
        peer_store_path: None,
    };

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--config" => {
                if let Some(value) = args.next() {
                    config.config_path = Some(value);
                }
            }
            "--state" => {
                if let Some(value) = args.next() {
                    config.state_path = Some(value);
                }
            }
            "--mempool-max" => {
                if let Some(value) = args.next() {
                    if let Ok(parsed) = value.parse() {
                        config.mempool_max = Some(parsed);
                    }
                }
            }
            "--mempool-per-sender" => {
                if let Some(value) = args.next() {
                    if let Ok(parsed) = value.parse() {
                        config.mempool_per_sender = Some(parsed);
                    }
                }
            }
            "--mempool-bump-bps" => {
                if let Some(value) = args.next() {
                    if let Ok(parsed) = value.parse() {
                        config.mempool_bump_bps = Some(parsed);
                    }
                }
            }
            "--rpc" => {
                config.rpc_enabled = true;
            }
            "--rpc-addr" => {
                if let Some(value) = args.next() {
                    config.rpc_addr = Some(value);
                }
            }
            "--mode" => {
                if let Some(value) = args.next() {
                    config.mode = parse_mode(&value);
                }
            }
            "--devnet" => {
                config.mode = NodeMode::Devnet;
            }
            "--validator" => {
                config.mode = NodeMode::Validator;
            }
            "--snapshot-listen" => {
                if let Some(value) = args.next() {
                    config.snapshot_listen = Some(value);
                }
            }
            "--snapshot-fetch" => {
                if let Some(value) = args.next() {
                    config.snapshot_fetch = Some(value);
                }
            }
            "--snapshot-out" => {
                if let Some(value) = args.next() {
                    config.snapshot_out = Some(value);
                }
            }
            "--snapshot-chunk-size" => {
                if let Some(value) = args.next() {
                    if let Ok(parsed) = value.parse() {
                        config.snapshot_chunk_size = Some(parsed);
                    }
                }
            }
            "--snapshot-max-bytes" => {
                if let Some(value) = args.next() {
                    if let Ok(parsed) = value.parse() {
                        config.snapshot_max_bytes = Some(parsed);
                    }
                }
            }
            "--node-key-path" => {
                if let Some(value) = args.next() {
                    config.node_key_path = Some(value);
                }
            }
            "--peer-store-path" => {
                if let Some(value) = args.next() {
                    config.peer_store_path = Some(value);
                }
            }
            _ => {}
        }
    }

    config
}

fn apply_cli_overrides(config: &mut AppConfig, cli: &CliConfig) {
    if let Some(path) = &cli.state_path {
        config.engine.state_path = path.clone();
    }
    if let Some(max) = cli.mempool_max {
        config.mempool.max_total = max;
    }
    if let Some(per_sender) = cli.mempool_per_sender {
        config.mempool.max_per_sender = per_sender;
    }
    if let Some(bump) = cli.mempool_bump_bps {
        config.mempool.bump_bps = bump;
    }
    if cli.rpc_enabled {
        config.rpc.enabled = true;
    }
    if let Some(addr) = &cli.rpc_addr {
        config.rpc.addr = addr.clone();
    }
    if let Some(path) = &cli.node_key_path {
        config.p2p.node_key_path = path.clone();
    }
    if let Some(path) = &cli.peer_store_path {
        config.p2p.peer_store_path = path.clone();
    }
}

#[derive(Debug, Clone, Copy)]
enum NodeMode {
    Devnet,
    Full,
    Validator,
}

fn parse_mode(value: &str) -> NodeMode {
    match value.to_lowercase().as_str() {
        "full" => NodeMode::Full,
        "validator" => NodeMode::Validator,
        _ => NodeMode::Devnet,
    }
}

fn run_devnet_demo(
    engine: &mut Engine,
    max_supply: U256,
    initial_reward: U256,
    config: &AppConfig,
) -> anyhow::Result<()> {
    let dummy_private_key = "0x1111111111111111111111111111111111111111111111111111111111111111";
    let alice = private_key_to_address(dummy_private_key)?;
    let bob = Address::from_slice(&[0x22; 20]);
    let carol = Address::from_slice(&[0x33; 20]);
    let dave = Address::from_slice(&[0x44; 20]);

    engine.add_validator(alice, U256::from(50u64))?;
    engine.add_validator(bob, U256::from(30u64))?;
    engine.add_validator(carol, U256::from(15u64))?;
    engine.add_validator(dave, U256::from(5u64))?;
    engine.stake_validator(bob, U256::from(10u64))?;
    engine.unbond_validator(dave, U256::from(2u64))?;
    engine.slash_validator(dave, U256::from(1u64), "double-sign")?;
    engine.set_miss_precommit(dave);
    engine.consensus.set_miss_prevote(carol);

    let mut governance = Governance::new(6_000, 5_000, 0, 1);
    let proposal_id = governance.submit(
        "Reduce gas limit",
        ProposalKind::SetFeeMarket {
            gas_limit_per_block: 20_000_000,
            elasticity_multiplier: 2,
            max_change_denominator: 8,
        },
        engine.block_number,
    )?;

    let proposal_token = governance.submit(
        "Adjust token economics",
        ProposalKind::SetTokenEconomics {
            max_supply,
            initial_reward_per_block: initial_reward,
            halving_interval: 4_200_000,
        },
        engine.block_number,
    )?;
    info!("queued proposal: {proposal_token}");

    for validator in engine.consensus.validators() {
        governance.vote(
            proposal_id,
            engine.block_number,
            validator.address,
            validator.stake,
            true,
        )?;
    }

    engine.fund_account(alice, U256::from(10_000_000_000u64), 0);

    engine.transfer(
        alice,
        bob,
        U256::from(1_000u64),
        21_000,
        U256::from(1u64),
        0,
    )?;

    // Simple contract that returns 0x2a (42) for any call.
    // Creation bytecode: deploys runtime bytecode 0x602a60005260206000f3.
    let contract_creation = Bytes::from_static(&[
        0x60, 0x0a, 0x60, 0x0c, 0x60, 0x00, 0x39, 0x60, 0x0a, 0x60, 0x00, 0xf3, 0x60, 0x2a,
        0x60, 0x00, 0x52, 0x60, 0x20, 0x60, 0x00, 0xf3,
    ]);

    engine.deploy_contract(
        alice,
        contract_creation,
        1_000_000,
        U256::from(1u64),
        1,
        U256::ZERO,
    )?;

    let block = engine.execute_block()?;
    let _ = engine.execute_block()?;
    let exec_block = engine.execute_block()?;

    let total_stake = engine.consensus.total_stake();
    let tally = governance.execute(proposal_id, exec_block.number, exec_block.finalized, total_stake, |kind| match kind {
        ProposalKind::SetFeeMarket {
            gas_limit_per_block,
            elasticity_multiplier,
            max_change_denominator,
        } => {
            engine.set_fee_market_params(
                *gas_limit_per_block,
                *elasticity_multiplier,
                *max_change_denominator,
            );
            Ok(())
        }
        ProposalKind::SetTokenEconomics {
            max_supply,
            initial_reward_per_block,
            halving_interval,
        } => {
            engine.set_token_economics(*max_supply, *initial_reward_per_block, *halving_interval);
            Ok(())
        }
    })?;
    info!(
        "block {}: chain_id={} txs={} gas_used={} gas_limit={} base_fee={} coinbase=0x{}",
        block.number,
        block.chain_id,
        block.transactions.len(),
        block.gas_used,
        block.gas_limit,
        block.base_fee,
        hex::encode(block.coinbase)
    );
    info!("state root: 0x{}", hex::encode(block.state_root));
    info!(
        "governance: yes={} no={} quorum={} passed={}",
        tally.yes_stake,
        tally.no_stake,
        tally.quorum_reached,
        tally.passed
    );
    info!("governance: total_stake={}", tally.total_stake);
    if let Some(proposal) = governance.proposal(proposal_id) {
        info!("proposal {}: {}", proposal.id, proposal.title);
    }
    info!("governance: votes={}", governance.vote_count(proposal_id));
    if let Some(sample) = governance.sample_vote(proposal_id) {
        info!(
            "sample vote: voter=0x{} support={} stake={}",
            hex::encode(sample.voter),
            sample.support,
            sample.stake
        );
    }
    info!("next base fee: {}", engine.base_fee);
    info!(
        "block hash=0x{} proposer=0x{} finalized={} votes={}/{} committed_stake={} threshold={}",
        hex::encode(block.hash),
        hex::encode(block.proposer),
        block.finalized,
        block.consensus.votes.len(),
        block.consensus.total_stake,
        block.consensus.committed_stake,
        block.consensus.threshold
    );
    for round in &block.finality_rounds {
        info!(
            "finality round {}: prevote_stake={} precommit_stake={} finalized={} timeout_ms={} timed_out={}",
            round.round,
            round.prevote_stake,
            round.precommit_stake,
            round.finalized,
            round.timeout_ms,
            round.timed_out
        );
    }
    if !block.slashing_evidence.is_empty() {
        for evidence in &block.slashing_evidence {
            info!(
                "evidence: validator=0x{} kind={} height={} round={}",
                hex::encode(evidence.validator),
                evidence.kind.as_str(),
                evidence.height,
                evidence.round
            );
        }
    }
    info!("consensus block_hash=0x{}", hex::encode(block.consensus.block_hash));
    if let Some(first_vote) = block.consensus.votes.first() {
        info!(
            "first vote: validator=0x{} block_hash=0x{} stake={}",
            hex::encode(first_vote.validator),
            hex::encode(first_vote.block_hash),
            first_vote.stake
        );
    }
    info!(
        "rewards: scheduled={} remaining_supply={} total_distributed={} burned={}",
        block.consensus.scheduled_reward,
        block.consensus.remaining_supply,
        block.consensus.total_reward,
        block.consensus.burned_reward
    );
    for reward in &block.rewards {
        info!(
            "reward: validator=0x{} amount={}",
            hex::encode(reward.address),
            reward.amount
        );
    }
    for change in &block.applied_validator_changes {
        match change {
            ValidatorChange::Stake { address, amount } => {
                info!(
                    "validator change: stake validator=0x{} amount={}",
                    hex::encode(address),
                    amount
                );
            }
            ValidatorChange::Unbond { address, amount } => {
                info!(
                    "validator change: unbond validator=0x{} amount={}",
                    hex::encode(address),
                    amount
                );
            }
        }
    }
    for slash in &block.slashes {
        info!(
            "slash: validator=0x{} amount={} reason={} height={}",
            hex::encode(slash.address),
            slash.amount,
            slash.reason,
            slash.height
        );
    }
    for (index, receipt) in block.receipts.iter().enumerate() {
        info!(
            "tx {}: success={} gas_used={} output_len={} error={}",
            index,
            receipt.success,
            receipt.gas_used,
            receipt.output.len(),
            receipt.error.as_deref().unwrap_or("none")
        );
    }

    let alice_balance = engine.get_balance(alice)?;
    let bob_balance = engine.get_balance(bob)?;
    let carol_balance = engine.get_balance(carol)?;
    let dave_balance = engine.get_balance(dave)?;
    info!("alice balance: {}", alice_balance);
    info!("bob balance: {}", bob_balance);
    info!("carol balance: {}", carol_balance);
    info!("dave balance: {}", dave_balance);
    info!("mempool empty: {}", engine.mempool_is_empty());

    run_p2p_demo()?;
    run_network_demo(&config.p2p.peer_store_path)?;

    Ok(())
}

fn run_p2p_demo() -> anyhow::Result<()> {
    let mut network = P2pNetwork::default();
    let mut engine_a = Engine::new_with_state(7919, "state-node-a");
    let mut engine_b = Engine::new_with_state(7919, "state-node-b");

    engine_a.add_validator(Address::from_slice(&[0x11; 20]), U256::from(10u64))?;
    engine_b.add_validator(Address::from_slice(&[0x11; 20]), U256::from(10u64))?;

    let node_a = Node::new("node-a", engine_a);
    let node_b = Node::new("node-b", engine_b);
    network.add_node(node_a);
    network.add_node(node_b);

    let block = {
        let node = network.node_mut("node-a").unwrap();
        node.engine.execute_block()?
    };
    network.broadcast("node-a", P2pMessage::Block(block.clone()));
    network.broadcast(
        "node-a",
        P2pMessage::Tx(Transaction {
            from: Address::from_slice(&[0x55; 20]),
            to: Some(Address::from_slice(&[0x66; 20])),
            value: U256::from(1u64),
            data: Bytes::new(),
            gas_limit: 21_000,
            gas_price: U256::from(1u64),
            nonce: 0,
            chain_id: Some(7919),
            signature: None,
        }),
    );
    network.broadcast(
        "node-a",
        P2pMessage::Vote(Message {
            from: Address::from_slice(&[0x11; 20]),
            round: 0,
            stage: RoundStage::Prevote,
            block_hash: block.hash,
            height: block.number,
        }),
    );
    network.sync_blocks("node-b", "node-a");

    let node_b = network.node_mut("node-b").unwrap();
    node_b.process_inbox()?;
    info!(
        "p2p sync: node-b height={} node-a height={}",
        node_b.engine.latest_height(),
        network.node_mut("node-a").unwrap().engine.latest_height()
    );

    Ok(())
}

fn run_network_demo(peer_store_path: &str) -> anyhow::Result<()> {
    let config = GossipConfig {
        listen_addr: "127.0.0.1:42000".to_string(),
        bootstrap_peers: vec!["127.0.0.1:42001".to_string()],
        fanout: 1,
        max_peers: 8,
        peer_ttl_secs: 5,
        max_seen: 128,
        seen_ttl: std::time::Duration::from_secs(30),
        retry_base: std::time::Duration::from_millis(50),
        retry_max: std::time::Duration::from_secs(2),
        peer_discovery_topic: "peer".to_string(),
    };
    let mut node_a = UdpGossip::bind_with_config("127.0.0.1:42000", config.clone())?;
    let mut node_b = UdpGossip::bind_with_config("127.0.0.1:42001", config)?;
    let _ = node_a.load_peers_from_file(peer_store_path);
    node_a.add_peer("127.0.0.1:42001")?;
    node_b.add_peer("127.0.0.1:42000")?;

    let packet = node_a.new_packet("tx", vec![1, 2, 3], 2);
    node_a.announce_self(2)?;
    node_a.broadcast(&packet)?;

    if let Some(packet) = node_b.recv_and_gossip(std::time::Duration::from_millis(200))? {
        info!(
            "udp gossip: topic={} bytes={} ttl={} id={}",
            packet.topic,
            packet.data.len(),
            packet.ttl,
            packet.id
        );
    }
    node_b.prune_peers();
    let _ = node_a.save_peers_to_file(peer_store_path);

    let tcp = TcpSync::bind("127.0.0.1:43000")?;
    let mut client = TcpSync::connect("127.0.0.1:43000", std::time::Duration::from_secs(1))?;
    let tcp_packet = node_a.new_packet("block", vec![9, 9, 9], 1);
    TcpSync::send_packet(&mut client, &tcp_packet)?;

    let mut server_stream = None;
    for _ in 0..10 {
        if let Some(stream) = tcp.accept_once()? {
            server_stream = Some(stream);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }

    if let Some(mut stream) = server_stream {
        if let Some(packet) = TcpSync::recv_packet(&mut stream)? {
            info!("tcp sync: topic={} bytes={}", packet.topic, packet.data.len());
        }
    }

    Ok(())
}

fn private_key_to_address(hex_key: &str) -> anyhow::Result<Address> {
    let key_bytes = hex::decode(hex_key.trim_start_matches("0x"))?;
    let signing_key = SigningKey::from_slice(&key_bytes)?;
    let verifying_key = signing_key.verifying_key();
    let encoded = verifying_key.to_encoded_point(false);
    let public_key = encoded.as_bytes();
    let hash = keccak256(&public_key[1..]);
    Ok(Address::from_slice(&hash[12..]))
}
