use k256::ecdsa::SigningKey;
use mersennet::config::{AppConfig, load_config, parse_address, parse_u256};
use mersennet::consensus::ValidatorChange;
use mersennet::engine::{Engine, Transaction};
use mersennet::governance::{Governance, ProposalKind};
use mersennet::identity::load_or_create_identity;
use mersennet::network::{Message, RoundStage};
use mersennet::prometheus;
use mersennet_network::net_transport::{GossipConfig, TcpSync, UdpGossip};
use mersennet_network::p2p::{NetworkNode, Node, P2pMessage, P2pNetwork};
use mersennet_rpc::{rpc, ws};
use revm::primitives::{Address, Bytes, U256, keccak256};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tracing::info;

/// Push a `MersennetOrdersTrades` WS notification for every fill recorded in a
/// freshly produced block. The matching engine records `Trade` domain events
/// (including those from `mersennet_orders_submitOrder` RPC fills, which drain
/// into the block at production), but nothing pushed them to subscribers — so
/// the indexer's trade subscription, and thus trades/volume/charts, stayed
/// empty. Payload is emitted in DECIMAL (the indexer rejects hex price/size).
fn notify_orders_trades(
    mgr: &mut ws::WsSubscriptionManager,
    events: &[mersennet::events::DomainEvent],
) {
    use mersennet::events::{DomainEvent, MersennetOrdersEvent};
    use mersennet::mersennet_orders::Side;
    for ev in events {
        if let DomainEvent::MersennetOrders(MersennetOrdersEvent::Trade {
            taker,
            maker,
            market_id,
            side,
            price,
            size,
        }) = ev
        {
            let trade = serde_json::json!({
                "taker": format!("{}", taker),
                "maker": format!("{}", maker),
                "market_id": market_id.0,
                "side": match side {
                    Side::Buy => "buy",
                    Side::Sell => "sell",
                },
                "price": price.to_string(),
                "size": size.to_string(),
            });
            mgr.notify_trade(&trade, market_id.0);
        }
    }
}

/// Engine handle for the signal handler, so a termination signal can wait
/// for the current block commit before the process exits.
static SHUTDOWN_ENGINE: std::sync::OnceLock<Arc<Mutex<Engine>>> = std::sync::OnceLock::new();

fn main() -> anyhow::Result<()> {
    // `mersennet --version` prints the same string web3_clientVersion reports,
    // so packaging and operators can identify a binary without starting it.
    if std::env::args()
        .skip(1)
        .any(|a| a == "--version" || a == "-V")
    {
        println!(
            "Mersennet/{}-{}",
            env!("CARGO_PKG_VERSION"),
            option_env!("MERSENNET_GIT_SHA").unwrap_or("dev")
        );
        return Ok(());
    }
    // initialize structured tracing from env and install Prometheus metrics
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let _prom = prometheus::init();
    // Emit a startup gauge so /metrics is non-empty on boot
    metrics::gauge!("mersennet_up", 1.0);

    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_clone = Arc::clone(&shutdown);
    // SIGINT/SIGTERM/SIGHUP (the `termination` feature covers systemd's
    // SIGTERM — previously only Ctrl-C was handled, so `systemctl stop`
    // killed the process at an arbitrary point, sometimes between writing
    // the account state and recording the block height; the node then
    // resumed one block behind its own state and wedged until a full
    // resync). Taking the engine lock waits for any in-flight block commit
    // to complete, after which the persisted state is self-consistent.
    ctrlc::set_handler(move || {
        info!("shutting down: waiting for the in-flight block commit...");
        shutdown_clone.store(true, Ordering::SeqCst);
        if let Some(engine) = SHUTDOWN_ENGINE.get() {
            match engine.lock() {
                Ok(guard) => {
                    if let Err(err) = guard.flush_state() {
                        tracing::warn!(%err, "state flush on shutdown failed");
                    }
                    info!("state committed; exiting");
                }
                Err(_) => info!("engine lock poisoned; exiting"),
            }
        }
        std::process::exit(0);
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

    match app_config.engine.resume_root_check.as_str() {
        "fatal" => mersennet::engine::set_resume_root_check_fatal(true),
        "warn" => {}
        other => tracing::warn!(
            value = other,
            "engine.resume_root_check: unknown value, using \"warn\""
        ),
    }
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
    engine.mersennet_orders_set_margin_params(
        app_config.mersennet_orders.initial_margin_bps,
        app_config.mersennet_orders.maintenance_margin_bps,
    );
    engine.set_allow_unsigned_orders_rpc(app_config.mersennet_orders.allow_unsigned_orders_rpc);
    let bridge_limit = if app_config.bridge.max_queue_len == 0 {
        None
    } else {
        Some(app_config.bridge.max_queue_len)
    };
    engine.set_bridge_limits(bridge_limit);

    // Genesis accounts are seeded ONLY on a fresh chain. This used to run on
    // every startup, overwriting each listed account's balance AND nonce with
    // the genesis values — so any restarted node reset e.g. the system CLOB
    // address to nonce 0, could no longer import blocks whose txs carried the
    // advanced nonce, and stalled (and diverged its state root from peers).
    if engine.latest_height() == 0 {
        for account in &app_config.genesis.accounts {
            let address = parse_address(&account.address)?;
            let balance = parse_u256(&account.balance)?;
            engine.fund_account(address, balance, account.nonce);
        }
    }

    for validator in &app_config.genesis.validators {
        let address = parse_address(&validator.address)?;
        let stake = parse_u256(&validator.stake)?;
        engine.add_validator(address, stake)?;
        info!(
            address = %validator.address,
            stake = %validator.stake,
            "registered genesis validator"
        );
    }

    // Seed CLOB markets + collateral deterministically at genesis so the
    // order book is identical on every node (consensus-deterministic
    // under single-leader production). Only applied when starting a fresh
    // chain (block_number == 1); a resumed chain already has them.
    if engine.latest_height() == 0 {
        for m in &app_config.genesis.markets {
            let tick = parse_u256(&m.tick_size).unwrap_or(U256::from(1));
            let lot = parse_u256(&m.lot_size).unwrap_or(U256::from(1));
            let id = engine.mersennet_orders_add_market(m.symbol.clone(), tick, lot);
            info!(symbol = %m.symbol, market_id = id.0, "seeded genesis market");
        }
        for c in &app_config.genesis.collateral {
            if let (Ok(owner), Ok(amount)) = (parse_address(&c.owner), parse_u256(&c.amount)) {
                engine.mersennet_orders_deposit_collateral(owner, amount);
                info!(owner = %c.owner, amount = %c.amount, "seeded genesis collateral");
            }
        }
        for a in &app_config.genesis.collateral_assets {
            match (
                parse_address(&a.token),
                parse_u256(&a.value_num),
                parse_u256(&a.value_den),
            ) {
                (Ok(token), Ok(num), Ok(den)) => {
                    engine.register_collateral_asset(
                        token,
                        a.weight_bps,
                        num,
                        den,
                        U256::from(a.balances_slot),
                    );
                    info!(token = %a.token, weight_bps = a.weight_bps, "registered genesis collateral asset");
                }
                _ => info!(token = %a.token, "skipped malformed genesis collateral asset"),
            }
        }
    }

    engine.set_unbonding_period(app_config.slashing.unbonding_period);
    engine.set_fee_market_params(
        app_config.engine.gas_limit_per_block,
        app_config.engine.fee_elasticity_multiplier,
        app_config.engine.fee_max_change_denominator,
    );
    engine.set_round_timeout_ms(app_config.slashing.round_timeout_ms);
    engine.set_validator_set_params(app_config.validator_set.to_params());
    // A restart must resume with the set the network is using, not the
    // genesis set from the config (see Engine::restore_consensus_set).
    engine.restore_consensus_set();
    if app_config.validator_set.activation_height > 0 {
        info!(
            activation_height = app_config.validator_set.activation_height,
            epoch_blocks = app_config.validator_set.epoch_blocks,
            min_self_stake_mrsn = app_config.validator_set.min_self_stake_mrsn,
            max_validators = app_config.validator_set.max_validators,
            "open validator set configured"
        );
    }
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
    // ───── Privacy hard-fork wiring (Workstream H) ─────
    //
    // Apply the operator-supplied privacy config. The defaults
    // leave the privacy subsystem inactive; chain-7920 (privacy
    // testnet) configs set `mode_activated: true` or stamp an
    // activation height.
    engine.set_dkg_epoch_length(app_config.privacy.dkg_epoch_length_blocks);
    if let Some(h) = app_config.privacy.activation_height {
        engine.set_privacy_activation_height(h);
        info!(
            activation_height = h,
            "privacy activation height configured"
        );
    }
    if app_config.privacy.mode_activated {
        engine.activate_privacy_mode();
    }
    info!(
        privacy_mode_activated = engine.privacy_mode_activated(),
        dkg_epoch_length_blocks = app_config.privacy.dkg_epoch_length_blocks,
        threshold_k = app_config.privacy.threshold_k,
        threshold_n = app_config.privacy.threshold_n,
        "privacy subsystem configured"
    );

    if handle_snapshot_cli(&mut engine, &cli)? {
        return Ok(());
    }

    info!("╔══════════════════════════════════════╗");
    info!(
        "║       Mersennet v{}-{}        ║",
        env!("CARGO_PKG_VERSION"),
        option_env!("MERSENNET_GIT_SHA").unwrap_or("dev")
    );
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
            // Since the validator set is permissionless, any node can be in
            // the active set from one epoch to the next: a full node whose
            // operator registers it must start proposing and voting at the
            // boundary without a restart or a mode switch. So every node
            // registers its identity and runs the producer and voter threads;
            // both act only while the identity is in the current set
            // (`--mode validator` is kept for the fleet units; it behaves the
            // same way).
            let is_validator = true;
            let mode_label = if matches!(cli.mode, NodeMode::Validator) {
                "validator"
            } else {
                "full"
            };
            engine.set_local_validator(identity.address);
            info!(
                mode = mode_label,
                listen = %app_config.p2p.listen,
                peers = app_config.p2p.peers.len(),
                validator_addr = %format!("0x{}", hex::encode(identity.address.as_slice())),
                "starting network node"
            );

            let mut gossip_config = GossipConfig {
                listen_addr: app_config.p2p.listen.clone(),
                bootstrap_peers: app_config.p2p.peers.clone(),
                ..GossipConfig::default()
            };
            // Local multi-node testnets live on 127.0.0.1; on a real network a
            // loopback "peer" only makes a node gossip to itself.
            gossip_config.allow_loopback_peers = gossip_config.uses_loopback();

            let engine = Arc::new(Mutex::new(engine));
            let _ = SHUTDOWN_ENGINE.set(engine.clone());
            let network = NetworkNode::new(&gossip_config)?;
            let operator = app_config
                .p2p
                .operator_address
                .as_deref()
                .map(|a| a.trim().trim_start_matches("0x"))
                .filter(|a| a.len() == 40)
                .and_then(|a| hex::decode(a).ok())
                .map(|b| Address::from_slice(&b));
            if app_config.p2p.operator_address.is_some() && operator.is_none() {
                tracing::warn!(
                    "p2p.operator_address is not a valid 0x address; whoami will report no operator"
                );
            }
            let attestor = Arc::new(mersennet::identity::NodeAttestor {
                signing_key: identity.signing_key.clone(),
                identity: identity.address,
                operator,
                version: format!(
                    "Mersennet/{}-{}",
                    env!("CARGO_PKG_VERSION"),
                    option_env!("MERSENNET_GIT_SHA").unwrap_or("dev")
                ),
            });
            rpc::set_node_identity(attestor.identity_json());
            network.start_networking_with_attestor(engine.clone(), &gossip_config, Some(attestor));

            // Transaction relay: locally-submitted (RPC) transactions must
            // reach the validators. Blocks are broadcast on production, but
            // a full/observer node otherwise holds mempool txs forever —
            // this loop gossips every pending tx exactly once per (from,
            // nonce) so they propagate to block producers.
            {
                let eng_relay = engine.clone();
                let net_relay = network.clone();
                let shutdown_relay = Arc::clone(&shutdown);
                std::thread::Builder::new()
                    .name("tx-relay".into())
                    .spawn(move || {
                        use std::collections::{HashMap, HashSet};
                        use std::time::{Duration, Instant};

                        // Gossip is UDP and fire-and-forget. Broadcasting each
                        // (sender, nonce) exactly once meant a single lost
                        // datagram orphaned that nonce forever: peers held every
                        // later tx from the sender as future-nonce, nothing ever
                        // re-sent the head, and the wallet wedged until this node
                        // restarted (the market maker sat like that for two
                        // weeks). Re-broadcast anything still pending with
                        // exponential backoff; a replacement (new hash for the
                        // same nonce) goes out immediately. Entries are dropped
                        // as soon as the tx leaves the pending pool, so the map
                        // is bounded by pool size.
                        struct Relayed {
                            hash: Option<revm::primitives::B256>,
                            last: Instant,
                            attempts: u32,
                        }
                        const RETRY_BASE: Duration = Duration::from_secs(2);
                        const RETRY_CAP: Duration = Duration::from_secs(30);
                        const MAX_RETRIES_PER_TICK: usize = 64;
                        fn retry_delay(attempts: u32) -> Duration {
                            let shift = attempts.saturating_sub(1).min(4);
                            (RETRY_BASE * 2u32.pow(shift)).min(RETRY_CAP)
                        }

                        let mut relayed: HashMap<(revm::primitives::Address, u64), Relayed> =
                            HashMap::new();
                        loop {
                            if shutdown_relay.load(Ordering::SeqCst) {
                                break;
                            }
                            std::thread::sleep(Duration::from_millis(250));
                            // Snapshot pending txs AND their raw envelopes
                            // (wallet-submitted txs must be relayed verbatim so
                            // peers can verify the Ethereum signature).
                            let pending: Vec<(mersennet::engine::Transaction, Option<Vec<u8>>)> = {
                                let Ok(eng) = eng_relay.lock() else { break };
                                eng.mempool_pending_snapshot()
                                    .into_iter()
                                    .map(|tx| {
                                        let raw = tx.hash.and_then(|h| eng.raw_tx_for(&h));
                                        (tx, raw)
                                    })
                                    .collect()
                            };
                            // Forget txs that mined or were evicted.
                            let live: HashSet<(revm::primitives::Address, u64)> =
                                pending.iter().map(|(tx, _)| (tx.from, tx.nonce)).collect();
                            relayed.retain(|key, _| live.contains(key));

                            let now = Instant::now();
                            let mut retries = 0usize;
                            for (tx, raw) in pending {
                                let key = (tx.from, tx.nonce);
                                let (due, is_retry) = match relayed.get(&key) {
                                    None => (true, false),
                                    Some(prev) if prev.hash != tx.hash => (true, false),
                                    Some(prev) => (
                                        now.duration_since(prev.last) >= retry_delay(prev.attempts),
                                        true,
                                    ),
                                };
                                if !due {
                                    continue;
                                }
                                if is_retry {
                                    if retries >= MAX_RETRIES_PER_TICK {
                                        continue;
                                    }
                                    retries += 1;
                                }
                                if let Err(err) = net_relay.broadcast_tx_with_raw(&tx, raw.as_deref()) {
                                    tracing::warn!(%err, "tx relay broadcast error");
                                }
                                let entry = relayed.entry(key).or_insert(Relayed {
                                    hash: tx.hash,
                                    last: now,
                                    attempts: 0,
                                });
                                if entry.hash != tx.hash {
                                    entry.attempts = 0;
                                }
                                entry.hash = tx.hash;
                                entry.last = now;
                                entry.attempts += 1;
                                if entry.attempts == 3 {
                                    tracing::info!(
                                        sender = %tx.from,
                                        nonce = tx.nonce,
                                        "pending tx still unconfirmed after 2 relays; retrying with backoff"
                                    );
                                }
                            }
                        }
                    })
                    .ok();
            }

            let ws_manager = Arc::new(Mutex::new(ws::WsSubscriptionManager::new()));
            if let Ok(engine_guard) = engine.lock() {
                ws::set_privacy_mode_activated(engine_guard.privacy_mode_activated());
            }

            if app_config.ws.enabled {
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

            if is_validator {
                let block_time = std::time::Duration::from_millis(app_config.p2p.block_time_ms);
                let eng = engine.clone();
                let net = network.clone();
                let shutdown_producer = Arc::clone(&shutdown);
                let zk_enabled = app_config.zk.enabled;
                let zk_interval = app_config.zk.checkpoint_interval;
                let ws_mgr_producer = ws_manager.clone();
                let my_addr = identity.address;
                let producer_key = identity.signing_key.clone();
                let compact_wire_height = app_config.p2p.compact_wire_height;
                let fast_failover_height = app_config.p2p.fast_failover_height;
                std::thread::Builder::new()
                    .name("block-producer".into())
                    .spawn(move || {
                        info!(
                            block_time_ms = block_time.as_millis(),
                            "block production started (leader-gated BFT)"
                        );
                        // Failover state: the height we are currently
                        // waiting to be produced, and how long we've
                        // waited. If the elected leader for a height
                        // fails to produce within a timeout, we bump the
                        // round to rotate to the next leader so a dead
                        // leader cannot halt the chain.
                        let mut waiting_height: u64 = 0;
                        let mut waiting_round: u64 = 0;
                        let mut deadline_skip_logged_for: u64 = 0;
                        let mut waited_ms: u64 = 0;
                        // Round timeout: give the elected leader several
                        // block-times to produce (plus slack for gossip)
                        // before rotating. Generous relative to the
                        // block time so healthy leaders never trip it.
                        // Legacy: 8 block-times + 3 s (19 s at 2 s blocks), sized when
                        // large blocks could not propagate. Fast: 3 block-times + 2 s
                        // (8 s), from `p2p.fast_failover_height`; recomputed per height
                        // below so every node rotates on the same clock.
                        let legacy_round_timeout_ms = block_time.as_millis() as u64 * 8 + 3000;
                        let fast_round_timeout_ms = block_time.as_millis() as u64 * 3 + 2000;
                        // Poll frequently so we detect a new height (and
                        // our turn to lead) with low latency.
                        let poll = std::time::Duration::from_millis(100);
                        // Pace production against the last block we
                        // observed rather than a fixed extra sleep, so the
                        // per-height rotation latency is not added on top
                        // of a full block time.
                        let mut last_seen_height: u64 = 0;
                        let mut last_block_at = std::time::Instant::now();
                        // Startup sync grace: before producing our FIRST block,
                        // give the gossip/sync layer time to tell us the network
                        // head. Without this, a validator that restarted with a
                        // wiped/behind chain would immediately produce a
                        // genesis-adjacent block (parent 0x0 is legal on an empty
                        // chain), fork itself, and then reject every canonical
                        // block as a parent-hash mismatch. Waiting lets
                        // `highest_observed_height` populate so the `behind`
                        // check below gates production correctly. On a genuine
                        // genesis start every node just waits this once.
                        let boot = std::time::Instant::now();
                        let startup_sync_grace = std::time::Duration::from_secs(15);
                        let mut produced_since_boot = false;
                        let mut in_set: Option<bool> = None;
                        loop {
                            if shutdown_producer.load(Ordering::SeqCst) {
                                break;
                            }
                            // Only members of the current active set propose.
                            // Membership changes at epoch boundaries (open
                            // validator set), so re-check every poll.
                            let (member, height_now) = {
                                let Ok(e) = eng.lock() else { break };
                                (e.validator_addresses().contains(&my_addr), e.latest_height())
                            };
                            if !mersennet_network::net_transport::compact_wire()
                                && height_now >= compact_wire_height
                            {
                                mersennet_network::net_transport::set_compact_wire(true);
                                info!(height = height_now, "gossip payloads now base64 (compact wire format)");
                            }
                            if in_set != Some(member) {
                                if member {
                                    info!(identity = %format!("0x{}", hex::encode(my_addr.as_slice())), "this node is in the active validator set: proposing blocks when leader");
                                } else if in_set.is_some() {
                                    info!("this node left the active validator set: no longer proposing");
                                } else {
                                    info!("this node is not in the active validator set: following only (register at trade.mersennet.com/staking)");
                                }
                                in_set = Some(member);
                            }
                            if !member {
                                std::thread::sleep(block_time);
                                continue;
                            }
                            std::thread::sleep(poll);

                            // Hold off the very first production until we've had
                            // a chance to learn the network head.
                            if !produced_since_boot && boot.elapsed() < startup_sync_grace {
                                continue;
                            }

                            // Determine the next height and whether this
                            // node is its elected leader (at the current
                            // failover round).
                            let head = {
                                let Ok(e) = eng.lock() else { break };
                                e.latest_height()
                            };
                            if head > last_seen_height {
                                last_seen_height = head;
                                last_block_at = std::time::Instant::now();
                            }
                            let next_height = head.saturating_add(1);
                            let round_timeout_ms = if next_height >= fast_failover_height {
                                fast_round_timeout_ms
                            } else {
                                legacy_round_timeout_ms
                            };

                            // Reset failover tracking when we advance to a
                            // new height.
                            if next_height != waiting_height {
                                waiting_height = next_height;
                                waiting_round = 0;
                                waited_ms = 0;
                            } else {
                                waited_ms = waited_ms.saturating_add(poll.as_millis() as u64);
                                if waited_ms >= round_timeout_ms {
                                    waiting_round = waiting_round.saturating_add(1);
                                    waited_ms = 0;
                                    tracing::warn!(
                                        height = next_height,
                                        round = waiting_round,
                                        "leader timed out — rotating to next leader"
                                    );
                                }
                            }

                            // Do not produce while behind the network head. A
                            // validator that restarted or briefly diverged is
                            // still catching up; producing now would build on a
                            // stale head and fork the chain (the re-fork failure
                            // mode). Sync to the canonical head first. Slack of 0:
                            // if the network is at height N and we are at N, we
                            // may lead N+1; if the network is already past N we
                            // must sync.
                            let behind = {
                                let Ok(e) = eng.lock() else { break };
                                e.is_behind_network(0)
                            };
                            if behind {
                                continue;
                            }

                            // Rounds are local timers, so a validator that has
                            // just caught up (restart, heal) starts at round 0
                            // while the rest of the network may already have
                            // timed out on *its* slot and rotated. On Sep 12
                            // validator 3 rejoined exactly as its own slot
                            // expired, produced a round-0 block in the same
                            // second the round-1 leader did, and forked itself
                            // (no reorg). Anchor the round to chain time: the
                            // parent block's timestamp tells everyone how long
                            // this height has been pending, so a late arrival
                            // adopts the round the network is actually in and
                            // only proposes if it leads *that* round.
                            let time_round = {
                                let Ok(e) = eng.lock() else { break };
                                e.block_by_number(head)
                                    .map(|b| {
                                        let now = std::time::SystemTime::now()
                                            .duration_since(std::time::UNIX_EPOCH)
                                            .map(|d| d.as_secs())
                                            .unwrap_or(0);
                                        // Peers start their round timer when the parent
                                        // arrives (~its timestamp) and rotate every
                                        // round_timeout; mirror that.
                                        let pending_ms = now.saturating_sub(b.timestamp).saturating_mul(1000);
                                        pending_ms / round_timeout_ms
                                    })
                                    .unwrap_or(0)
                            };
                            if time_round > waiting_round {
                                tracing::info!(
                                    height = next_height,
                                    local_round = waiting_round,
                                    chain_time_round = time_round,
                                    "adopting the network's failover round from chain time (late arrival)"
                                );
                                waiting_round = time_round;
                                waited_ms = 0;
                            }
                            // Never propose within two block times of a rotation
                            // deadline: peers time their rounds from when the parent
                            // reached them, so a proposal this close to the boundary
                            // races the next round's leader (validator 2 lost that
                            // race by ~100 ms after a restart on Sep 13 and forked).
                            // The next leader takes the height instead; cost: one
                            // rotation, ~19 s, only when a leader is already late.
                            {
                                let Ok(e) = eng.lock() else { break };
                                if let Some(b) = e.block_by_number(head) {
                                    let now = std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .map(|d| d.as_secs())
                                        .unwrap_or(0);
                                    let pending_ms = now.saturating_sub(b.timestamp).saturating_mul(1000);
                                    let into_round = pending_ms % round_timeout_ms;
                                    let margin = block_time.as_millis() as u64 * 2;
                                    if into_round + margin >= round_timeout_ms {
                                        if deadline_skip_logged_for != next_height {
                                            deadline_skip_logged_for = next_height;
                                            tracing::info!(
                                                height = next_height,
                                                round = waiting_round,
                                                into_round_ms = into_round,
                                                "too close to the round deadline — leaving this height to the next leader"
                                            );
                                        }
                                        continue;
                                    }
                                }
                            }

                            let am_leader = {
                                let Ok(e) = eng.lock() else { break };
                                // The network already finalized this height (the
                                // votes beat the block to us): wait for block-sync
                                // instead of proposing an orphan.
                                if e.finalized_hash(next_height).is_some() {
                                    continue;
                                }
                                e.is_leader(next_height, waiting_round)
                            };
                            if !am_leader {
                                // Not our turn — the elected leader
                                // produces; we import via gossip and vote.
                                continue;
                            }
                            // Pace to the target block time relative to the
                            // last observed block, so consecutive blocks
                            // are spaced by ~block_time without stacking
                            // the rotation-detection latency on top.
                            let since = last_block_at.elapsed();
                            if since < block_time {
                                std::thread::sleep(block_time - since);
                            }

                            let block = {
                                let mut e = match eng.lock() {
                                    Ok(e) => e,
                                    Err(_) => break,
                                };
                                // Re-check leadership under the lock: the
                                // height may have advanced while we paced.
                                if e.latest_height().saturating_add(1) != next_height
                                    || !e.is_leader(next_height, waiting_round)
                                {
                                    continue;
                                }
                                let _ = my_addr;
                                match e.execute_block() {
                                    Ok(mut b) => {
                                        ws::set_privacy_mode_activated(e.privacy_mode_activated());
                                        // `consensus.proposer()` uses a weighted
                                        // priority lottery that does NOT match
                                        // `leader_for_height` (rotation + round
                                        // failover). Overwrite with this node's
                                        // address — we only produce when
                                        // `is_leader` says we are the leader —
                                        // so import auth and the signature agree.
                                        b.proposer = my_addr;
                                        b.coinbase = my_addr;
                                        b.consensus.proposer = my_addr;
                                        let sig = mersennet::crypto::sign_block_proposal(
                                            b.number,
                                            b.hash,
                                            &producer_key,
                                        );
                                        b.proposer_sig = Some(sig);
                                        // Also stamp the chain copy so peer sync
                                        // serves the signed block, not the
                                        // unsigned snapshot stored mid-produce.
                                        e.attach_proposer_sig(b.number, my_addr, sig);
                                        b
                                    }
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

                            if let Ok(mut mgr) = ws_mgr_producer.lock() {
                                mgr.purge_transparent_subscriptions();
                                let block_json = serde_json::json!({
                                    "number": format!("0x{:x}", block.number),
                                    "hash": format!("{}", block.hash),
                                    "parentHash": format!("{}", block.parent_hash),
                                    "timestamp": format!("0x{:x}", block.timestamp),
                                    "gasLimit": format!("0x{:x}", block.gas_limit),
                                    "gasUsed": format!("0x{:x}", block.gas_used),
                                    "baseFeePerGas": format!("0x{:x}", block.base_fee),
                                    "stateRoot": format!("{}", block.state_root),
                                    "miner": format!("{}", block.proposer),
                                    "nonce": "0x0000000000000000",
                                    "difficulty": "0x0",
                                    "logsBloom": "0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
                                    "sha3Uncles": "0x0000000000000000000000000000000000000000000000000000000000000000",
                                    "receiptsRoot": "0x0000000000000000000000000000000000000000000000000000000000000000",
                                    "transactionsRoot": format!("{}", block.hash),
                                    "extraData": "0x",
                                });
                                mgr.notify_new_block(&block_json);
                                notify_orders_trades(&mut mgr, &block.domain_events);

                                for tx in &block.transactions {
                                    let hash = mersennet::crypto::tx_signing_hash(tx);
                                    mgr.notify_new_tx(&format!("{}", hash));
                                }

                                for (rx_idx, receipt) in block.receipts.iter().enumerate() {
                                    for (log_idx, log) in receipt.logs.iter().enumerate() {
                                        let log_json = serde_json::json!({
                                            "address": format!("{}", log.address),
                                            "topics": log.topics.iter().map(|t| format!("{}", t)).collect::<Vec<_>>(),
                                            "data": format!("0x{}", hex::encode(&log.data)),
                                            "blockNumber": format!("0x{:x}", block.number),
                                            "blockHash": format!("{}", block.hash),
                                            "transactionHash": format!("{}", block.hash),
                                            "transactionIndex": format!("0x{:x}", rx_idx),
                                            "logIndex": format!("0x{:x}", log_idx),
                                            "removed": false,
                                        });
                                        mgr.notify_log(
                                            &log_json,
                                            log.address,
                                            &log.topics,
                                        );
                                    }
                                }

                                // ───── Shielded WS dispatch (C3) ─────
                                //
                                // Walk this block's domain events and
                                // fan them out to the privacy-mode WS
                                // subscriptions. Each payload is
                                // address-free by construction (the
                                // shielded events only carry roots,
                                // commitments, and market-level
                                // aggregates; CI K2 enforces that).
                                for ev in &block.domain_events {
                                    if let mersennet::events::DomainEvent::Shielded(sev) = ev {
                                        match sev {
                                            mersennet::events::ShieldedEvent::ShieldedRootAdvanced { block_number, new_root, notes_added, nullifiers_added } => {
                                                let payload = serde_json::json!({
                                                    "blockNumber": format!("0x{:x}", block_number),
                                                    "newRoot":     format!("0x{}", hex::encode(new_root)),
                                                    "notesAdded":  format!("0x{:x}", notes_added),
                                                    "nullifiersAdded": format!("0x{:x}", nullifiers_added),
                                                });
                                                mgr.notify_shielded_root(&payload);
                                            }
                                            mersennet::events::ShieldedEvent::FbaCleared { market_id, clearing_price, matched_size, intent_count } => {
                                                let payload = serde_json::json!({
                                                    "marketId":      format!("0x{:x}", market_id.0),
                                                    "clearingPrice": format!("0x{:x}", clearing_price),
                                                    "matchedSize":   format!("0x{:x}", matched_size),
                                                    "intentCount":   format!("0x{:x}", intent_count),
                                                });
                                                mgr.notify_clearing_price(&payload, market_id.0);
                                            }
                                            mersennet::events::ShieldedEvent::LiquidationSettled { market_id, winner_bond_commitment, winning_bid } => {
                                                let payload = serde_json::json!({
                                                    "marketId":              format!("0x{:x}", market_id.0),
                                                    "winnerBondCommitment":  format!("0x{}", hex::encode(winner_bond_commitment)),
                                                    "winningBid":            format!("0x{:x}", winning_bid),
                                                });
                                                mgr.notify_auction_settled(&payload, market_id.0);
                                            }
                                            mersennet::events::ShieldedEvent::MempoolBatchAdmitted { .. } => {
                                                // Not a public-facing WS topic — keep it
                                                // in domain_events for indexing only.
                                            }
                                        }
                                    }
                                }

                                // State-proof WS dispatch (C3+C4):
                                // fire `newStateProof` whenever the
                                // block carries one.
                                if let Some(proof) = &block.state_proof {
                                    let payload = serde_json::json!({
                                        "blockNumber":   format!("0x{:x}", proof.block_height),
                                        "prevStateRoot": format!("0x{}", hex::encode(proof.prev_state_root.as_slice())),
                                        "newStateRoot":  format!("0x{}", hex::encode(proof.new_state_root.as_slice())),
                                        "blockHash":     format!("0x{}", hex::encode(proof.block_hash.as_slice())),
                                        "txCount":       format!("0x{:x}", proof.tx_count),
                                        "proofType":     format!("{:?}", proof.proof_type),
                                    });
                                    mgr.notify_state_proof(&payload);
                                }
                            }

                            produced_since_boot = true;
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

            // BFT finality voter thread. On every validator, as blocks are
            // applied (whether produced locally or imported via gossip),
            // sign a vote over (height, block_hash) and broadcast it. When
            // votes representing >= 2/3 of stake are collected for a height,
            // that block is finalized (see Engine::record_finality_vote).
            // Decoupling voting from production means both the leader and
            // followers vote through the identical path.
            if is_validator {
                let eng_voter = engine.clone();
                let net_voter = network.clone();
                let shutdown_voter = Arc::clone(&shutdown);
                let voter_key = identity.signing_key.clone();
                // The highest height this key has voted for survives restarts
                // (and state heals, which only swap the state directory): a
                // voter that came back with last_voted = 0 re-voted its last
                // 16 heights, and after a heal onto the canonical branch those
                // were different hashes — conflicting votes for honest reasons.
                let last_voted_path = std::path::Path::new(&app_config.engine.state_path)
                    .parent()
                    .map(|d| d.join("voter_last_voted"))
                    .unwrap_or_else(|| std::path::PathBuf::from("voter_last_voted"));
                std::thread::Builder::new()
                    .name("bft-voter".into())
                    .spawn(move || {
                        let mut last_voted: u64 = std::fs::read_to_string(&last_voted_path)
                            .ok()
                            .and_then(|s| s.trim().parse::<u64>().ok())
                            .unwrap_or(0);
                        if last_voted > 0 {
                            tracing::info!(
                                last_voted,
                                "bft-voter resuming after the last persisted vote"
                            );
                        }
                        let persist = |h: u64| {
                            let tmp = last_voted_path.with_extension("tmp");
                            if std::fs::write(&tmp, format!("{h}\n")).is_ok() {
                                let _ = std::fs::rename(&tmp, &last_voted_path);
                            }
                        };
                        loop {
                            if shutdown_voter.load(Ordering::SeqCst) {
                                break;
                            }
                            std::thread::sleep(std::time::Duration::from_millis(150));
                            // Snapshot applied blocks above last_voted.
                            let to_vote: Vec<(u64, revm::primitives::B256)> = {
                                let Ok(e) = eng_voter.lock() else { break };
                                let head = e.latest_height();
                                let mut out = Vec::new();
                                // Not in the active set: nothing to sign.
                                // Track the head so that joining at an epoch
                                // boundary starts voting from there, not
                                // from heights we merely followed.
                                if let Some(me) = e.local_validator()
                                    && !e.validator_addresses().contains(&me)
                                {
                                    if head > last_voted {
                                        last_voted = head;
                                    }
                                    continue;
                                }
                                // Vote for a bounded window so a lagging
                                // voter catches up without flooding.
                                let start =
                                    last_voted.saturating_add(1).max(head.saturating_sub(16));
                                for h in start..=head {
                                    if let Some(b) = e.block_by_number(h) {
                                        out.push((h, b.hash));
                                    }
                                }
                                out
                            };
                            for (height, block_hash) in to_vote {
                                let (r, s, y) =
                                    mersennet::crypto::sign_vote(height, block_hash, &voter_key);
                                // Record our own vote locally too.
                                if let Ok(mut e) = eng_voter.lock() {
                                    let me = e.local_validator();
                                    if let Some(me) = me {
                                        e.record_finality_vote(height, block_hash, me);
                                    }
                                }
                                if let Err(err) =
                                    net_voter.broadcast_vote(height, block_hash, r, s, y)
                                {
                                    tracing::debug!(%err, height, "vote broadcast error");
                                }
                                if height > last_voted {
                                    last_voted = height;
                                    persist(height);
                                }
                            }
                        }
                    })?;
            }

            // Liveness watchdog (see WatchdogConfig). Exit codes: 3 = head stalled
            // while the network moved on, 4 = engine lock unobtainable.
            if app_config.watchdog.enabled {
                let eng = engine.clone();
                let shutdown_wd = Arc::clone(&shutdown);
                let wd = app_config.watchdog.clone();
                std::thread::Builder::new()
                    .name("liveness-watchdog".into())
                    .spawn(move || {
                        let check_every = std::time::Duration::from_secs(30);
                        let mut last_head: u64 = 0;
                        let mut last_change = std::time::Instant::now();
                        let mut lock_failing_since: Option<std::time::Instant> = None;
                        let reference_head = |url: &str| -> Option<u64> {
                            if url.is_empty() {
                                return None;
                            }
                            let resp = ureq::post(url)
                                .timeout(std::time::Duration::from_secs(8))
                                .set("content-type", "application/json")
                                .send_string(r#"{"jsonrpc":"2.0","id":1,"method":"eth_blockNumber","params":[]}"#)
                                .ok()?;
                            let body = resp.into_string().ok()?;
                            let v: serde_json::Value = serde_json::from_str(&body).ok()?;
                            let hex = v.get("result")?.as_str()?;
                            u64::from_str_radix(hex.trim_start_matches("0x"), 16).ok()
                        };
                        loop {
                            if shutdown_wd.load(Ordering::SeqCst) {
                                break;
                            }
                            std::thread::sleep(check_every);
                            // Take the engine lock without blocking forever.
                            let mut snapshot: Option<(u64, u64)> = None;
                            let started = std::time::Instant::now();
                            while started.elapsed() < std::time::Duration::from_secs(10) {
                                match eng.try_lock() {
                                    Ok(e) => {
                                        snapshot = Some((e.latest_height(), e.highest_observed_height()));
                                        break;
                                    }
                                    Err(std::sync::TryLockError::Poisoned(_)) => {
                                        tracing::error!("engine lock is poisoned; exiting so systemd restarts the node");
                                        std::process::exit(4);
                                    }
                                    Err(_) => std::thread::sleep(std::time::Duration::from_millis(250)),
                                }
                            }
                            let Some((head, observed)) = snapshot else {
                                let since = *lock_failing_since.get_or_insert_with(std::time::Instant::now);
                                if since.elapsed().as_secs() >= wd.lock_secs {
                                    tracing::error!(
                                        held_secs = since.elapsed().as_secs(),
                                        "engine lock unobtainable; node is wedged — exiting so systemd restarts it"
                                    );
                                    std::process::exit(4);
                                }
                                continue;
                            };
                            lock_failing_since = None;
                            if head != last_head {
                                last_head = head;
                                last_change = std::time::Instant::now();
                                continue;
                            }
                            let stalled = last_change.elapsed().as_secs();
                            if stalled < wd.stall_secs {
                                continue;
                            }
                            // Stalled. Is the network ahead? Gossip's high-water
                            // mark first, a reference RPC as a second opinion.
                            let network = reference_head(&wd.reference_rpc).unwrap_or(0).max(observed);
                            if network >= head.saturating_add(wd.min_gap_blocks) {
                                tracing::error!(
                                    head,
                                    network_head = network,
                                    stalled_secs = stalled,
                                    "head has not advanced while the network moved on — exiting so systemd restarts the node"
                                );
                                if let Ok(e) = eng.try_lock() {
                                    let _ = e.flush_state();
                                }
                                std::process::exit(3);
                            }
                            tracing::warn!(
                                head,
                                stalled_secs = stalled,
                                "head not advancing, but no evidence the network is ahead (network-wide halt or no peers) — not restarting"
                            );
                        }
                    })?;
            }

            // Block-watcher thread: polls for new blocks and notifies WS subscribers.
            // This enables WS events on full nodes that receive blocks via gossip
            // (not just validators that produce blocks).
            if app_config.ws.enabled {
                let eng_watcher = engine.clone();
                let ws_mgr_watcher = ws_manager.clone();
                std::thread::Builder::new()
                    .name("ws-block-watcher".into())
                    .spawn(move || {
                        let mut last_height: u64 = eng_watcher
                            .lock()
                            .map(|e| e.latest_height())
                            .unwrap_or(0);
                        loop {
                            std::thread::sleep(std::time::Duration::from_millis(200));
                            let current_height = {
                                let Ok(eng) = eng_watcher.lock() else { break };
                                ws::set_privacy_mode_activated(eng.privacy_mode_activated());
                                eng.latest_height()
                            };
                            if current_height <= last_height {
                                continue;
                            }
                            for n in (last_height + 1)..=current_height {
                                let block = {
                                    let Ok(eng) = eng_watcher.lock() else { break };
                                    eng.block_by_number(n).cloned()
                                };
                                let Some(block) = block else { continue };
                                if let Ok(mut mgr) = ws_mgr_watcher.lock() {
                                    mgr.purge_transparent_subscriptions();
                                    let block_json = serde_json::json!({
                                        "number": format!("0x{:x}", block.number),
                                        "hash": format!("{}", block.hash),
                                        "parentHash": format!("0x{:064x}", block.number.saturating_sub(1)),
                                        "timestamp": format!("0x{:x}", block.timestamp),
                                        "gasLimit": format!("0x{:x}", block.gas_limit),
                                        "gasUsed": format!("0x{:x}", block.gas_used),
                                        "baseFeePerGas": format!("0x{:x}", block.base_fee),
                                        "stateRoot": format!("{}", block.state_root),
                                        "miner": format!("{}", block.proposer),
                                        "nonce": "0x0000000000000000",
                                        "difficulty": "0x0",
                                    });
                                    mgr.notify_new_block(&block_json);
                                    notify_orders_trades(&mut mgr, &block.domain_events);
                                    for tx in &block.transactions {
                                        let hash = mersennet::crypto::tx_signing_hash(tx);
                                        mgr.notify_new_tx(&format!("{}", hash));
                                    }
                                }
                            }
                            last_height = current_height;
                        }
                    })?;
            }

            if let Some(path) = cli.config_path.clone() {
                start_config_watcher(engine.clone(), path)?;
            }

            if app_config.rpc.enabled {
                rpc::set_trusted_ips(&app_config.rpc.trusted_ips);
                rpc::set_client_version(format!(
                    "Mersennet/{}-{}",
                    env!("CARGO_PKG_VERSION"),
                    option_env!("MERSENNET_GIT_SHA").unwrap_or("dev")
                ));
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

fn start_config_watcher(engine: Arc<Mutex<Engine>>, config_path: String) -> anyhow::Result<()> {
    let mut last_modified = std::fs::metadata(&config_path)
        .and_then(|meta| meta.modified())
        .ok();

    std::thread::spawn(move || {
        loop {
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
        }
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
    engine.set_slashing_bps(config.slashing.double_sign_bps, config.slashing.timeout_bps);
    engine.set_slashing_escalation(
        config.slashing.escalation_step_bps,
        config.slashing.escalation_max_bps,
    );
    engine.mersennet_orders_set_margin_params(
        config.mersennet_orders.initial_margin_bps,
        config.mersennet_orders.maintenance_margin_bps,
    );
    engine.set_allow_unsigned_orders_rpc(config.mersennet_orders.allow_unsigned_orders_rpc);
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
        engine
            .evm
            .state
            .load_mersennet_orders(&mut engine.orders.state)?;
        engine.evm.state.load_bridge_queues(
            &mut engine.bridge.orders_to_evm,
            &mut engine.bridge.evm_to_orders,
        )?;
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
                if let Some(value) = args.next()
                    && let Ok(parsed) = value.parse()
                {
                    config.mempool_max = Some(parsed);
                }
            }
            "--mempool-per-sender" => {
                if let Some(value) = args.next()
                    && let Ok(parsed) = value.parse()
                {
                    config.mempool_per_sender = Some(parsed);
                }
            }
            "--mempool-bump-bps" => {
                if let Some(value) = args.next()
                    && let Ok(parsed) = value.parse()
                {
                    config.mempool_bump_bps = Some(parsed);
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
                if let Some(value) = args.next()
                    && let Ok(parsed) = value.parse()
                {
                    config.snapshot_chunk_size = Some(parsed);
                }
            }
            "--snapshot-max-bytes" => {
                if let Some(value) = args.next()
                    && let Ok(parsed) = value.parse()
                {
                    config.snapshot_max_bytes = Some(parsed);
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
            halving_interval: 33_550_336,
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
        0x60, 0x0a, 0x60, 0x0c, 0x60, 0x00, 0x39, 0x60, 0x0a, 0x60, 0x00, 0xf3, 0x60, 0x2a, 0x60,
        0x00, 0x52, 0x60, 0x20, 0x60, 0x00, 0xf3,
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
    let tally = governance.execute(
        proposal_id,
        exec_block.number,
        exec_block.finalized,
        total_stake,
        |kind| match kind {
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
                engine.set_token_economics(
                    *max_supply,
                    *initial_reward_per_block,
                    *halving_interval,
                );
                Ok(())
            }
        },
    )?;
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
        tally.yes_stake, tally.no_stake, tally.quorum_reached, tally.passed
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
    info!(
        "consensus block_hash=0x{}",
        hex::encode(block.consensus.block_hash)
    );
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
    let mut engine_a = Engine::new_with_state(131071, "state-node-a");
    let mut engine_b = Engine::new_with_state(131071, "state-node-b");

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
            chain_id: Some(131071),
            signature: None,
            tx_type: 0,
            shielded_payload: None,
            hash: None,
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
        allow_loopback_peers: true,
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

    if let Some(mut stream) = server_stream
        && let Some(packet) = TcpSync::recv_packet(&mut stream)?
    {
        info!(
            "tcp sync: topic={} bytes={}",
            packet.topic,
            packet.data.len()
        );
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
