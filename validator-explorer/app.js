/* =============================================================================
   PrimeNodes — Validator Dashboard for Prime Chain (Chain ID 7919)
   Single-page app: hash-based routing, RPC polling, canvas charts
   ============================================================================= */

(function () {
    'use strict';

    // ── Config ──────────────────────────────────────────────────────────
    const RPC_URL = '/rpc';
    const CHAIN_ID = 7919;
    const POLL_INTERVAL = 5000;
    const BLOCKS_TO_SCAN = 200;
    const EXPLORER_BASE = 'http://46.225.30.187';
    const CHART_COLORS = [
        '#9461FF', '#6A2FFF', '#00FFF9', '#4901FF', '#E8DCFF',
        '#3b82f6', '#22C55E', '#F59E0B', '#EF4444', '#f472b6',
        '#a78bfa', '#34d399', '#fbbf24', '#60a5fa', '#fb923c'
    ];

    const VALIDATOR_NAMES = {
        '0x1a09b94d7dd32cf1903d1745effffae23ce76bca': 'Validator Alpha',
        '0xd4668658dd943d90ea25883c291c789886e0b7fc': 'Validator Beta',
        '0xccf642e03dffaf65250a9716e2aa7c51c985d72c': 'Validator Gamma',
        '0xe7c512700ac6e4629549c642d50b7f598edf8e77': 'Validator Delta',
    };

    function cssVar(name) {
        return getComputedStyle(document.body).getPropertyValue(name).trim();
    }

    // ── State ───────────────────────────────────────────────────────────
    let state = {
        validators: [],
        currentBlock: 0,
        blockTimes: [],
        blockProducers: {},
        recentBlocks: [],
        lastUpdate: 0,
        connected: false,
        pollTimer: null,
        contractLookup: {
            address: '',
            loading: false,
            result: null,
            error: '',
        },
    };

    // ── RPC Helper ──────────────────────────────────────────────────────
    let rpcId = 1;

    async function rpcCall(method, params = []) {
        const res = await fetch(RPC_URL, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ jsonrpc: '2.0', id: rpcId++, method, params }),
        });
        const text = await res.text();
        let json;
        try {
            json = JSON.parse(text);
        } catch {
            throw new Error('Non-JSON response (status ' + res.status + '): ' + text.slice(0, 120));
        }
        if (json.error) throw new Error(json.error.message || 'RPC error');
        return json.result;
    }

    // ── Data Fetching ───────────────────────────────────────────────────
    async function fetchValidators() {
        try {
            const result = await rpcCall('prime_validators');
            if (Array.isArray(result)) {
                state.validators = result.map(v => ({
                    address: (v.address || '').toLowerCase(),
                    stake: v.stake || '0',
                    is_active: v.is_active !== false,
                }));
                state.validators.sort((a, b) => {
                    const sa = BigInt(a.stake || '0');
                    const sb = BigInt(b.stake || '0');
                    return sb > sa ? 1 : sb < sa ? -1 : 0;
                });
            }
        } catch (e) {
            console.warn('prime_validators RPC unavailable:', e.message);
            if (state.validators.length === 0 && Object.keys(state.blockProducers).length > 0) {
                state.validators = Object.entries(state.blockProducers).map(([addr, blocks]) => ({
                    address: addr,
                    stake: '0',
                    is_active: true,
                }));
            }
        }
    }

    async function fetchBlockData() {
        try {
            let hexNum;
            try {
                hexNum = await rpcCall('prime_blockNumber');
            } catch {
                hexNum = await rpcCall('eth_blockNumber');
            }
            const blockNum = parseInt(hexNum, 16);
            if (blockNum === state.currentBlock) return;

            const prevBlock = state.currentBlock;
            state.currentBlock = blockNum;

            const blocksToFetch = Math.min(BLOCKS_TO_SCAN, blockNum);
            const startBlock = blockNum - blocksToFetch + 1;
            const newBlocks = [];
            const producers = {};
            const times = [];

            const getBlockMethod = await (async () => {
                try {
                    const test = await rpcCall('prime_getBlockByNumber', ['0x' + blockNum.toString(16), false]);
                    if (test) return 'prime_getBlockByNumber';
                } catch {}
                return 'eth_getBlockByNumber';
            })();

            const batchSize = 20;
            for (let i = 0; i < blocksToFetch; i += batchSize) {
                const batch = [];
                for (let j = i; j < Math.min(i + batchSize, blocksToFetch); j++) {
                    const num = startBlock + j;
                    batch.push(rpcCall(getBlockMethod, ['0x' + num.toString(16), false]));
                }
                const results = await Promise.allSettled(batch);
                for (const r of results) {
                    if (r.status === 'fulfilled' && r.value) {
                        newBlocks.push(r.value);
                    }
                }
            }

            newBlocks.sort((a, b) => parseInt(a.number, 16) - parseInt(b.number, 16));

            for (let i = 1; i < newBlocks.length; i++) {
                const t1 = parseInt(newBlocks[i - 1].timestamp, 16);
                const t2 = parseInt(newBlocks[i].timestamp, 16);
                if (t2 > t1) times.push(t2 - t1);
            }

            const ZERO_ADDR = '0x0000000000000000000000000000000000000000';
            for (const block of newBlocks) {
                const raw = (block.proposer || block.miner || '').toLowerCase();
                const proposer = (raw && raw !== ZERO_ADDR) ? raw : '';
                if (proposer) {
                    producers[proposer] = (producers[proposer] || 0) + 1;
                }
            }

            state.blockTimes = times;
            state.blockProducers = producers;
            state.recentBlocks = newBlocks.slice(-50).reverse();
            state.lastUpdate = Date.now();
            state.connected = true;

            updateHeaderStats();
        } catch (e) {
            console.error('Block fetch error:', e);
            state.connected = false;
            updateHeaderStats();
        }
    }

    async function pollData() {
        await Promise.allSettled([fetchValidators(), fetchBlockData()]);
        renderCurrentPage();
    }

    function startPolling() {
        pollData();
        state.pollTimer = setInterval(pollData, POLL_INTERVAL);
    }

    function stopPolling() {
        if (state.pollTimer) {
            clearInterval(state.pollTimer);
            state.pollTimer = null;
        }
    }

    // ── Formatting Helpers ──────────────────────────────────────────────
    function formatPrim(weiStr) {
        if (!weiStr || weiStr === '0') return '0';
        try {
            const wei = BigInt(weiStr);
            const whole = wei / BigInt(10 ** 18);
            const frac = wei % BigInt(10 ** 18);
            const fracStr = frac.toString().padStart(18, '0').slice(0, 4);
            const wholeStr = whole.toLocaleString('en-US');
            return fracStr === '0000' ? wholeStr : `${wholeStr}.${fracStr}`;
        } catch {
            return '0';
        }
    }

    function formatPrimShort(weiStr) {
        if (!weiStr || weiStr === '0') return '0';
        try {
            const wei = BigInt(weiStr);
            const whole = Number(wei / BigInt(10 ** 18));
            if (whole >= 1e9) return (whole / 1e9).toFixed(1) + 'B';
            if (whole >= 1e6) return (whole / 1e6).toFixed(1) + 'M';
            if (whole >= 1e3) return (whole / 1e3).toFixed(1) + 'K';
            return whole.toLocaleString('en-US');
        } catch {
            return '0';
        }
    }

    function truncAddr(addr) {
        if (!addr || addr.length < 12) return addr || '';
        return addr.slice(0, 6) + '...' + addr.slice(-4);
    }

    function totalStake() {
        return state.validators.reduce((acc, v) => acc + BigInt(v.stake || '0'), BigInt(0));
    }

    function stakePct(stake) {
        const total = totalStake();
        if (total === BigInt(0)) return 0;
        return Number((BigInt(stake) * BigInt(10000)) / total) / 100;
    }

    function avgBlockTime() {
        if (!state.blockTimes.length) return 0;
        return state.blockTimes.reduce((a, b) => a + b, 0) / state.blockTimes.length;
    }

    function timeAgo(timestamp) {
        const secs = Math.floor(Date.now() / 1000) - timestamp;
        if (secs < 60) return secs + 's ago';
        if (secs < 3600) return Math.floor(secs / 60) + 'm ago';
        if (secs < 86400) return Math.floor(secs / 3600) + 'h ago';
        return Math.floor(secs / 86400) + 'd ago';
    }

    function validatorName(addr) {
        return VALIDATOR_NAMES[(addr || '').toLowerCase()] || '';
    }

    function isAddress(value) {
        return /^0x[a-fA-F0-9]{40}$/.test((value || '').trim());
    }

    function contractPublicationStatus(attestation) {
        if (!attestation) return 'unpublished';
        return attestation.metadataUri ? 'source-published' : 'attested';
    }

    function contractPublicationLabel(status) {
        switch (status) {
            case 'source-published': return 'Source-Published';
            case 'attested': return 'Attested';
            default: return 'Unpublished';
        }
    }

    function contractPublicationBadge(status) {
        if (status === 'source-published') {
            return '<span class="badge badge-source-published">Source-Published</span>';
        }
        if (status === 'attested') {
            return '<span class="badge badge-attested">Attested</span>';
        }
        return '<span class="badge badge-unpublished">Unpublished</span>';
    }

    async function lookupContractAttestation(address) {
        return rpcCall('prime_getCodeAttestation', [address, 'latest']);
    }

    async function refreshContractLookup(address) {
        state.contractLookup.address = address;
        state.contractLookup.error = '';

        if (!isAddress(address)) {
            state.contractLookup.loading = false;
            state.contractLookup.result = null;
            state.contractLookup.error = 'Enter a valid 20-byte contract address.';
            renderCurrentPage();
            return;
        }

        state.contractLookup.loading = true;
        renderCurrentPage();

        try {
            const attestation = await lookupContractAttestation(address);
            state.contractLookup.result = {
                address,
                attestation,
                status: contractPublicationStatus(attestation),
            };
            state.contractLookup.error = '';
        } catch (e) {
            state.contractLookup.result = null;
            state.contractLookup.error = e.message || 'Lookup failed';
        } finally {
            state.contractLookup.loading = false;
            renderCurrentPage();
        }
    }

    async function handleContractLookupSubmit(event) {
        event?.preventDefault?.();
        const input = document.getElementById('contractLookupInput');
        const address = (input?.value || state.contractLookup.address || '').trim().toLowerCase();
        await refreshContractLookup(address);
    }

    function contractLookupResultMarkup() {
        const { address, loading, result, error } = state.contractLookup;
        if (loading) {
            return '<div class="attestation-empty">Looking up contract attestation…</div>';
        }
        if (error) {
            return `<div class="attestation-error">${error}</div>`;
        }
        if (!address) {
            return '<div class="attestation-empty">Enter a contract address to see whether it is unpublished, attested, or source-published.</div>';
        }
        if (!result) {
            return '';
        }

        const { status, attestation } = result;
        if (!attestation) {
            return `
                <div class="attestation-card unpublished">
                    <div class="attestation-head">
                        <div>
                            <div class="attestation-title">${truncAddr(address)}</div>
                            <div class="attestation-sub">No public code attestation found for this contract.</div>
                        </div>
                        ${contractPublicationBadge(status)}
                    </div>
                    <div class="attestation-grid">
                        <div class="attestation-row">
                            <span class="attestation-key">Status</span>
                            <span class="attestation-value">${contractPublicationLabel(status)}</span>
                        </div>
                        <div class="attestation-row">
                            <span class="attestation-key">Contract</span>
                            <span class="attestation-value mono">${address}</span>
                        </div>
                    </div>
                    <div class="attestation-actions">
                        <a href="#/contract/${address}" class="lookup-button">Open Contract View</a>
                    </div>
                </div>
            `;
        }

        return `
            <div class="attestation-card ${status}">
                <div class="attestation-head">
                    <div>
                        <div class="attestation-title">${truncAddr(address)}</div>
                        <div class="attestation-sub">Public code attestation available through prime_getCodeAttestation.</div>
                    </div>
                    ${contractPublicationBadge(status)}
                </div>
                <div class="attestation-grid">
                    <div class="attestation-row">
                        <span class="attestation-key">Status</span>
                        <span class="attestation-value">${contractPublicationLabel(status)}</span>
                    </div>
                    <div class="attestation-row">
                        <span class="attestation-key">Deployer</span>
                        <span class="attestation-value mono">${attestation.deployer}</span>
                    </div>
                    <div class="attestation-row">
                        <span class="attestation-key">Code Hash</span>
                        <span class="attestation-value mono break-all">${attestation.codeHash}</span>
                    </div>
                    <div class="attestation-row">
                        <span class="attestation-key">Published At</span>
                        <span class="attestation-value mono">${parseInt(attestation.publishedAtBlock, 16)}</span>
                    </div>
                    <div class="attestation-row full">
                        <span class="attestation-key">Metadata URI</span>
                        <span class="attestation-value mono break-all">${attestation.metadataUri || 'None published'}</span>
                    </div>
                </div>
                <div class="attestation-actions">
                    <a href="#/contract/${address}" class="lookup-button">Open Contract View</a>
                </div>
            </div>
        `;
    }

    function displayAddr(addr) {
        const name = validatorName(addr);
        return name ? name : truncAddr(addr);
    }

    function validatorUptime(addr) {
        const totalBlocks = Object.values(state.blockProducers).reduce((a, b) => a + b, 0);
        if (totalBlocks === 0 || state.validators.length === 0) return null;
        const proposed = state.blockProducers[addr] || 0;
        const expected = totalBlocks / state.validators.length;
        if (expected === 0) return null;
        return Math.min(100, (proposed / expected) * 100);
    }

    function uptimeBadgeHtml(addr) {
        const pct = validatorUptime(addr);
        if (pct === null) return '<span class="badge" style="background:var(--bg-surface);color:var(--text-dim);">N/A</span>';
        if (pct < 50) return `<span class="badge badge-inactive">${pct.toFixed(0)}%</span>`;
        if (pct < 80) return `<span class="badge" style="background:var(--warn-bg);color:var(--warn);">${pct.toFixed(0)}%</span>`;
        return `<span class="badge badge-active">${pct.toFixed(0)}%</span>`;
    }

    // ── SVG Icons ───────────────────────────────────────────────────────
    const icons = {
        copy: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 01-2-2V4a2 2 0 012-2h9a2 2 0 012 2v1"/></svg>',
        check: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>',
        back: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="19" y1="12" x2="5" y2="12"/><polyline points="12 19 5 12 12 5"/></svg>',
        external: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M18 13v6a2 2 0 01-2 2H5a2 2 0 01-2-2V8a2 2 0 012-2h6"/><polyline points="15 3 21 3 21 9"/><line x1="10" y1="14" x2="21" y2="3"/></svg>',
    };

    // ── Header Updates ──────────────────────────────────────────────────
    function updateHeaderStats() {
        const statusEl = document.getElementById('hdrStatus');
        const blockEl = document.getElementById('hdrBlock');
        const footerBlockEl = document.getElementById('footerBlock');

        if (state.connected) {
            statusEl.innerHTML = '<span class="pulse-dot online"></span> Online';
        } else {
            statusEl.innerHTML = '<span class="pulse-dot offline"></span> Offline';
        }

        if (state.currentBlock > 0) {
            blockEl.textContent = state.currentBlock.toLocaleString();
            footerBlockEl.textContent = state.currentBlock.toLocaleString();
        }
    }

    // ── Toast ───────────────────────────────────────────────────────────
    function showToast(msg) {
        const t = document.getElementById('toast');
        t.textContent = msg;
        t.classList.add('visible');
        setTimeout(() => t.classList.remove('visible'), 2500);
    }

    function copyToClipboard(text) {
        navigator.clipboard.writeText(text).then(() => showToast('Copied to clipboard'));
    }

    // ── Router ──────────────────────────────────────────────────────────
    function getRoute() {
        const hash = location.hash.slice(1) || '/dashboard';
        if (hash.startsWith('/validator/')) {
            return { page: 'validator', address: hash.slice(11).toLowerCase() };
        }
        if (hash.startsWith('/contract/')) {
            return { page: 'contract', address: hash.slice(10).toLowerCase() };
        }
        if (hash === '/validators') return { page: 'validators' };
        if (hash === '/network') return { page: 'network' };
        return { page: 'dashboard' };
    }

    function navigate(hash) {
        location.hash = hash;
    }

    function renderCurrentPage() {
        const route = getRoute();
        const content = document.getElementById('pageContent');

        // Update active nav
        document.querySelectorAll('.nav-link').forEach(link => {
            link.classList.toggle('active', link.dataset.page === route.page);
        });

        switch (route.page) {
            case 'dashboard': renderDashboard(content); break;
            case 'validators': renderValidators(content); break;
            case 'validator': renderValidatorDetail(content, route.address); break;
            case 'contract': renderContractDetail(content, route.address); break;
            case 'network': renderNetwork(content); break;
            default: renderDashboard(content);
        }
    }

    // ── Dashboard Page ──────────────────────────────────────────────────
    function renderDashboard(container) {
        const total = totalStake();
        const activeCount = state.validators.filter(v => v.is_active).length;
        const avg = avgBlockTime();

        container.innerHTML = `
            <div class="container page-enter">
                <div class="hero-banner">
                    <h1 class="hero-title">Prime Chain Validator Dashboard</h1>
                    <p class="hero-sub">Real-time network monitoring &amp; validator analytics</p>
                </div>

                <div class="stats-grid">
                    <div class="stat-card">
                        <div class="stat-card-icon accent-bg">&#9670;</div>
                        <div class="stat-card-label">Total Validators</div>
                        <div class="stat-card-value">${state.validators.length}</div>
                        <div class="stat-card-sub">${activeCount} active</div>
                    </div>
                    <div class="stat-card">
                        <div class="stat-card-icon violet-bg">&#9733;</div>
                        <div class="stat-card-label">Total Staked</div>
                        <div class="stat-card-value">${formatPrimShort(total.toString())}</div>
                        <div class="stat-card-sub">PRIM</div>
                    </div>
                    <div class="stat-card">
                        <div class="stat-card-icon pink-bg">&#9632;</div>
                        <div class="stat-card-label">Block Height</div>
                        <div class="stat-card-value">${state.currentBlock > 0 ? state.currentBlock.toLocaleString() : '—'}</div>
                        <div class="stat-card-sub">Live updating</div>
                    </div>
                    <div class="stat-card">
                        <div class="stat-card-icon success-bg">&#9201;</div>
                        <div class="stat-card-label">Avg Block Time</div>
                        <div class="stat-card-value">${avg > 0 ? avg.toFixed(1) + 's' : '—'}</div>
                        <div class="stat-card-sub">Last ${state.blockTimes.length} blocks</div>
                    </div>
                    <div class="stat-card">
                        <div class="stat-card-icon warn-bg">&#9889;</div>
                        <div class="stat-card-label">Network Status</div>
                        <div class="stat-card-value">${state.connected ? '<span style="color:var(--success)">Operational</span>' : '<span style="color:var(--danger)">Down</span>'}</div>
                        <div class="stat-card-sub">${state.connected ? 'All systems normal' : 'Cannot reach RPC'}</div>
                    </div>
                </div>

                <div class="charts-grid">
                    <div class="panel">
                        <div class="panel-head">
                            <span class="panel-title">Block Production</span>
                            <span class="panel-action" onclick="location.hash='#/network'">View All &rarr;</span>
                        </div>
                        <div class="chart-container">
                            <div class="chart-canvas-wrap">
                                <canvas id="dashBlockChart" height="200"></canvas>
                            </div>
                        </div>
                        <div class="chart-legend" id="dashBlockLegend"></div>
                    </div>
                    <div class="panel">
                        <div class="panel-head">
                            <span class="panel-title">Stake Distribution</span>
                        </div>
                        <div class="chart-container">
                            <div class="chart-canvas-wrap" style="display:flex;justify-content:center;">
                                <canvas id="dashPieChart" width="220" height="220"></canvas>
                                <div class="pie-center">
                                    <div class="pie-center-value">${state.validators.length}</div>
                                    <div class="pie-center-label">Validators</div>
                                </div>
                            </div>
                        </div>
                        <div class="chart-legend" id="dashPieLegend"></div>
                    </div>
                </div>

                <div class="panel">
                    <div class="panel-head">
                        <span class="panel-title">Top Validators</span>
                        <a href="#/validators" class="panel-action">View All &rarr;</a>
                    </div>
                    <div class="overflow-x">
                        <table class="data-table">
                            <thead>
                                <tr>
                                    <th>Rank</th>
                                    <th>Validator</th>
                                    <th>Stake</th>
                                    <th>Status</th>
                                    <th>% of Total</th>
                                    <th>Blocks</th>
                                    <th>Uptime</th>
                                </tr>
                            </thead>
                            <tbody>
                                ${state.validators.slice(0, 10).map((v, i) => validatorRow(v, i)).join('')}
                                ${state.validators.length === 0 ? '<tr><td colspan="7" class="empty-state"><div class="empty-state-text">No validators found</div></td></tr>' : ''}
                            </tbody>
                        </table>
                    </div>
                </div>
            </div>
        `;

        requestAnimationFrame(() => {
            drawBlockChart('dashBlockChart', 'dashBlockLegend');
            drawPieChart('dashPieChart', 'dashPieLegend');
        });
    }

    // ── Validators List Page ────────────────────────────────────────────
    function renderValidators(container) {
        const total = totalStake();
        const activeCount = state.validators.filter(v => v.is_active).length;

        container.innerHTML = `
            <div class="container page-enter">
                <div style="margin-bottom:1.5rem;">
                    <h1 style="font-size:1.35rem;font-weight:700;letter-spacing:-0.3px;">Validators</h1>
                    <p style="color:var(--text-muted);font-size:0.85rem;margin-top:0.15rem;">
                        ${state.validators.length} validators &middot; ${activeCount} active &middot;
                        ${formatPrimShort(total.toString())} PRIM staked
                    </p>
                </div>

                <div class="stats-grid" style="grid-template-columns: repeat(3, 1fr); margin-bottom:1.5rem;">
                    <div class="stat-card">
                        <div class="stat-card-label">Total Validators</div>
                        <div class="stat-card-value">${state.validators.length}</div>
                    </div>
                    <div class="stat-card">
                        <div class="stat-card-label">Active</div>
                        <div class="stat-card-value" style="color:var(--success)">${activeCount}</div>
                    </div>
                    <div class="stat-card">
                        <div class="stat-card-label">Inactive</div>
                        <div class="stat-card-value" style="color:var(--danger)">${state.validators.length - activeCount}</div>
                    </div>
                </div>

                <div class="table-wrap">
                    <div class="overflow-x">
                        <table class="data-table">
                            <thead>
                                <tr>
                                    <th>Rank</th>
                                    <th>Validator</th>
                                    <th>Stake (PRIM)</th>
                                    <th>Status</th>
                                    <th>% of Total Stake</th>
                                    <th>Blocks Proposed</th>
                                    <th>Uptime</th>
                                </tr>
                            </thead>
                            <tbody>
                                ${state.validators.map((v, i) => validatorRow(v, i)).join('')}
                                ${state.validators.length === 0 ? '<tr><td colspan="7" class="empty-state"><div class="empty-state-icon">&#128270;</div><div class="empty-state-text">No validators discovered yet</div></td></tr>' : ''}
                            </tbody>
                        </table>
                    </div>
                </div>
            </div>
        `;
    }

    function validatorRow(v, index) {
        const pct = stakePct(v.stake);
        const blocks = state.blockProducers[v.address] || 0;
        const gradientClass = ['gradient-1', 'gradient-2', 'gradient-3'][index % 3];
        const name = validatorName(v.address);

        return `
            <tr onclick="location.hash='#/validator/${v.address}'">
                <td><span class="badge badge-rank">#${index + 1}</span></td>
                <td>
                    <div style="display:flex;flex-direction:column;gap:2px;">
                        ${name ? '<span style="font-weight:600;font-size:0.85rem;">' + name + '</span>' : ''}
                        <span class="td-addr">${truncAddr(v.address)}</span>
                    </div>
                </td>
                <td class="td-mono">${formatPrim(v.stake)}</td>
                <td>
                    <span class="badge ${v.is_active ? 'badge-active' : 'badge-inactive'}">
                        <span class="badge-dot"></span>
                        ${v.is_active ? 'Active' : 'Inactive'}
                    </span>
                </td>
                <td>
                    <div class="stake-bar-wrap">
                        <div class="stake-bar">
                            <div class="stake-bar-fill ${gradientClass}" style="width:${Math.max(pct, 1)}%"></div>
                        </div>
                        <span class="stake-pct">${pct.toFixed(1)}%</span>
                    </div>
                </td>
                <td class="td-mono">${blocks}</td>
                <td>${uptimeBadgeHtml(v.address)}</td>
            </tr>
        `;
    }

    // ── Validator Detail Page ───────────────────────────────────────────
    function renderValidatorDetail(container, address) {
        const v = state.validators.find(x => x.address === address);
        const blocks = state.blockProducers[address] || 0;
        const totalBlocks = Object.values(state.blockProducers).reduce((a, b) => a + b, 0) || 1;
        const blockPct = ((blocks / totalBlocks) * 100).toFixed(1);

        const proposedBlocks = state.recentBlocks.filter(b =>
            (b.proposer || b.miner || '').toLowerCase() === address
        );

        if (!v) {
            container.innerHTML = `
                <div class="container page-enter">
                    <a href="#/validators" class="back-link">${icons.back} Back to Validators</a>
                    <div class="empty-state">
                        <div class="empty-state-icon">&#128270;</div>
                        <div class="empty-state-text">Validator ${truncAddr(address)} not found</div>
                    </div>
                </div>
            `;
            return;
        }

        const pct = stakePct(v.stake);
        const rank = state.validators.indexOf(v) + 1;

        const name = validatorName(v.address);
        const uptime = validatorUptime(v.address);
        const uptimeStr = uptime !== null ? uptime.toFixed(1) + '%' : 'N/A';

        container.innerHTML = `
            <div class="container page-enter">
                <a href="#/validators" class="back-link">${icons.back} Back to Validators</a>

                <div class="detail-header">
                    <div class="detail-icon">&#9670;</div>
                    <div class="detail-title-group">
                        <div class="detail-title">
                            ${name || 'Validator #' + rank}
                            <span class="badge ${v.is_active ? 'badge-active' : 'badge-inactive'}">
                                <span class="badge-dot"></span>
                                ${v.is_active ? 'Active' : 'Inactive'}
                            </span>
                            ${uptimeBadgeHtml(v.address)}
                        </div>
                        <div class="detail-addr">
                            ${v.address}
                            <button class="copy-btn" onclick="event.stopPropagation(); window.__copyAddr('${v.address}')" title="Copy address">
                                ${icons.copy}
                            </button>
                            <a href="${EXPLORER_BASE}/#/address/${v.address}" target="_blank" class="copy-btn" title="View in Explorer" style="color:var(--text-muted);">
                                ${icons.external}
                            </a>
                        </div>
                    </div>
                </div>

                <div class="detail-stats-row detail-stats-4">
                    <div class="stat-card">
                        <div class="stat-card-label">Stake Amount</div>
                        <div class="stat-card-value">${formatPrim(v.stake)}</div>
                        <div class="stat-card-sub">PRIM (${pct.toFixed(2)}% of total)</div>
                    </div>
                    <div class="stat-card">
                        <div class="stat-card-label">Blocks Proposed</div>
                        <div class="stat-card-value">${blocks}</div>
                        <div class="stat-card-sub">${blockPct}% of recent ${BLOCKS_TO_SCAN} blocks</div>
                    </div>
                    <div class="stat-card">
                        <div class="stat-card-label">Uptime</div>
                        <div class="stat-card-value" style="color:${uptime !== null && uptime >= 80 ? 'var(--success)' : uptime !== null && uptime >= 50 ? 'var(--warn)' : 'var(--danger)'}">${uptimeStr}</div>
                        <div class="stat-card-sub">vs expected share of ${BLOCKS_TO_SCAN} blocks</div>
                    </div>
                    <div class="stat-card">
                        <div class="stat-card-label">Rank</div>
                        <div class="stat-card-value">#${rank}</div>
                        <div class="stat-card-sub">of ${state.validators.length} validators</div>
                    </div>
                </div>

                <div class="detail-card">
                    <div class="detail-card-title">Validator Info</div>
                    <div class="detail-rows">
                        ${name ? `
                        <div class="detail-row">
                            <span class="detail-label">Name</span>
                            <span class="detail-val" style="font-weight:600;">${name}</span>
                        </div>
                        ` : ''}
                        <div class="detail-row">
                            <span class="detail-label">Address</span>
                            <span class="detail-val mono" style="font-size:0.82rem;">${v.address}</span>
                        </div>
                        <div class="detail-row">
                            <span class="detail-label">Stake</span>
                            <span class="detail-val mono">${formatPrim(v.stake)} PRIM</span>
                        </div>
                        <div class="detail-row">
                            <span class="detail-label">Status</span>
                            <span class="detail-val">
                                <span class="badge ${v.is_active ? 'badge-active' : 'badge-inactive'}">
                                    <span class="badge-dot"></span>
                                    ${v.is_active ? 'Active' : 'Inactive'}
                                </span>
                            </span>
                        </div>
                        <div class="detail-row">
                            <span class="detail-label">Uptime</span>
                            <span class="detail-val">${uptimeBadgeHtml(v.address)} <span style="color:var(--text-muted);font-size:0.78rem;margin-left:6px;">(based on last ${BLOCKS_TO_SCAN} blocks)</span></span>
                        </div>
                        <div class="detail-row">
                            <span class="detail-label">% of Total Stake</span>
                            <span class="detail-val">
                                <div class="stake-bar-wrap" style="max-width:300px;">
                                    <div class="stake-bar">
                                        <div class="stake-bar-fill gradient-1" style="width:${Math.max(pct, 1)}%"></div>
                                    </div>
                                    <span class="stake-pct">${pct.toFixed(2)}%</span>
                                </div>
                            </span>
                        </div>
                        <div class="detail-row">
                            <span class="detail-label">Explorer</span>
                            <span class="detail-val">
                                <a href="${EXPLORER_BASE}/#/address/${v.address}" target="_blank">
                                    View on PrimeScan &rarr;
                                </a>
                            </span>
                        </div>
                    </div>
                </div>

                <div class="detail-card">
                    <div class="detail-card-title">Recent Blocks Proposed (${proposedBlocks.length})</div>
                    <div class="block-list">
                        ${proposedBlocks.length === 0 ? `
                            <div class="empty-state">
                                <div class="empty-state-text">No blocks proposed in the last ${BLOCKS_TO_SCAN} blocks</div>
                            </div>
                        ` : proposedBlocks.map(b => {
                            const num = parseInt(b.number, 16);
                            const ts = parseInt(b.timestamp, 16);
                            const txCount = (b.transactions || []).length;
                            return `
                                <div class="block-item">
                                    <a href="${EXPLORER_BASE}/#/block/${num}" target="_blank" class="block-item-num">#${num.toLocaleString()}</a>
                                    <span class="td-mono" style="color:var(--text-secondary);">${txCount} txn${txCount !== 1 ? 's' : ''}</span>
                                    <span class="block-item-time">${timeAgo(ts)}</span>
                                </div>
                            `;
                        }).join('')}
                    </div>
                </div>
            </div>
        `;
    }

    function renderContractDetail(container, address) {
        if (!isAddress(address)) {
            container.innerHTML = `
                <div class="container page-enter">
                    <a href="#/network" class="back-link">${icons.back} Back to Network Stats</a>
                    <div class="empty-state">
                        <div class="empty-state-icon">&#128270;</div>
                        <div class="empty-state-text">Contract address is invalid</div>
                    </div>
                </div>
            `;
            return;
        }

        const cached = state.contractLookup.result && state.contractLookup.result.address === address
            ? state.contractLookup.result
            : null;
        const cachedError = state.contractLookup.address === address ? state.contractLookup.error : '';
        const isLoading = state.contractLookup.loading && state.contractLookup.address === address;

        if (!cached && !cachedError && !isLoading) {
            refreshContractLookup(address);
        }

        const status = cached ? cached.status : 'unpublished';
        const attestation = cached?.attestation;
        const publishBlock = attestation ? parseInt(attestation.publishedAtBlock, 16) : null;

        container.innerHTML = `
            <div class="container page-enter">
                <a href="#/network" class="back-link">${icons.back} Back to Network Stats</a>

                <div class="detail-header">
                    <div class="detail-icon">&#9635;</div>
                    <div class="detail-title-group">
                        <div class="detail-title">
                            Contract Publication View
                            ${!isLoading && !cachedError ? contractPublicationBadge(status) : ''}
                        </div>
                        <div class="detail-addr">
                            ${address}
                            <button class="copy-btn" onclick="event.stopPropagation(); window.__copyAddr('${address}')" title="Copy address">
                                ${icons.copy}
                            </button>
                            <a href="${EXPLORER_BASE}/#/address/${address}" target="_blank" class="copy-btn" title="View in Explorer" style="color:var(--text-muted);">
                                ${icons.external}
                            </a>
                        </div>
                    </div>
                </div>

                <div class="detail-stats-row detail-stats-4">
                    <div class="stat-card">
                        <div class="stat-card-label">Publication Status</div>
                        <div class="stat-card-value">${isLoading ? 'Loading…' : cachedError ? 'Lookup failed' : contractPublicationLabel(status)}</div>
                        <div class="stat-card-sub">Derived from prime_getCodeAttestation</div>
                    </div>
                    <div class="stat-card">
                        <div class="stat-card-label">Code Hash</div>
                        <div class="stat-card-value contract-stat-value">${attestation ? truncAddr(attestation.codeHash) : '—'}</div>
                        <div class="stat-card-sub">Public only when the deployer opts in</div>
                    </div>
                    <div class="stat-card">
                        <div class="stat-card-label">Published At</div>
                        <div class="stat-card-value">${publishBlock != null ? publishBlock.toLocaleString() : '—'}</div>
                        <div class="stat-card-sub">Block number</div>
                    </div>
                    <div class="stat-card">
                        <div class="stat-card-label">Source Metadata</div>
                        <div class="stat-card-value">${attestation?.metadataUri ? 'Available' : 'None'}</div>
                        <div class="stat-card-sub">Metadata URI presence upgrades status to source-published</div>
                    </div>
                </div>

                ${isLoading ? '<div class="attestation-empty">Looking up contract attestation…</div>' : ''}
                ${cachedError ? `<div class="attestation-error">${cachedError}</div>` : ''}

                ${!isLoading && !cachedError ? `
                <div class="detail-card">
                    <div class="detail-card-title">Publication Details</div>
                    <div class="detail-rows">
                        <div class="detail-row">
                            <span class="detail-label">Contract</span>
                            <span class="detail-val mono break-all">${address}</span>
                        </div>
                        <div class="detail-row">
                            <span class="detail-label">Status</span>
                            <span class="detail-val">${contractPublicationBadge(status)}</span>
                        </div>
                        <div class="detail-row">
                            <span class="detail-label">Deployer</span>
                            <span class="detail-val mono break-all">${attestation?.deployer || 'Not publicly published'}</span>
                        </div>
                        <div class="detail-row">
                            <span class="detail-label">Code Hash</span>
                            <span class="detail-val mono break-all">${attestation?.codeHash || 'Not publicly published'}</span>
                        </div>
                        <div class="detail-row">
                            <span class="detail-label">Published At Block</span>
                            <span class="detail-val mono">${publishBlock != null ? publishBlock : 'Not publicly published'}</span>
                        </div>
                        <div class="detail-row">
                            <span class="detail-label">Metadata URI</span>
                            <span class="detail-val mono break-all">${attestation?.metadataUri || 'None published'}</span>
                        </div>
                    </div>
                </div>
                ` : ''}
            </div>
        `;
    }

    window.__copyAddr = function (addr) {
        copyToClipboard(addr);
        const btns = document.querySelectorAll('.copy-btn');
        btns.forEach(btn => {
            if (btn.onclick && btn.onclick.toString().includes(addr)) {
                btn.classList.add('copied');
                btn.innerHTML = icons.check;
                setTimeout(() => {
                    btn.classList.remove('copied');
                    btn.innerHTML = icons.copy;
                }, 1500);
            }
        });
    };

    // ── Network Stats Page ──────────────────────────────────────────────
    function renderNetwork(container) {
        const avg = avgBlockTime();
        const totalBlocks = Object.values(state.blockProducers).reduce((a, b) => a + b, 0);

        container.innerHTML = `
            <div class="container page-enter">
                <div style="margin-bottom:1.5rem;">
                    <h1 style="font-size:1.35rem;font-weight:700;letter-spacing:-0.3px;">Network Statistics</h1>
                    <p style="color:var(--text-muted);font-size:0.85rem;margin-top:0.15rem;">
                        Block production and stake distribution analytics
                    </p>
                </div>

                <div class="stats-grid" style="grid-template-columns: repeat(4, 1fr); margin-bottom:1.5rem;">
                    <div class="stat-card">
                        <div class="stat-card-label">Blocks Scanned</div>
                        <div class="stat-card-value">${totalBlocks}</div>
                    </div>
                    <div class="stat-card">
                        <div class="stat-card-label">Unique Proposers</div>
                        <div class="stat-card-value">${Object.keys(state.blockProducers).length}</div>
                    </div>
                    <div class="stat-card">
                        <div class="stat-card-label">Avg Block Time</div>
                        <div class="stat-card-value">${avg > 0 ? avg.toFixed(2) + 's' : '—'}</div>
                    </div>
                    <div class="stat-card">
                        <div class="stat-card-label">Current Block</div>
                        <div class="stat-card-value">${state.currentBlock > 0 ? state.currentBlock.toLocaleString() : '—'}</div>
                    </div>
                </div>

                <div class="panel">
                    <div class="panel-head">
                        <span class="panel-title">Contract Publication Status</span>
                        <span style="font-size:0.75rem;color:var(--text-muted);">prime_getCodeAttestation labels contracts as unpublished, attested, or source-published</span>
                    </div>
                    <form class="lookup-form" id="contractLookupForm">
                        <input
                            id="contractLookupInput"
                            class="lookup-input mono"
                            type="text"
                            spellcheck="false"
                            placeholder="0x... contract address"
                            value="${state.contractLookup.address}"
                        />
                        <button class="lookup-button" type="submit">
                            ${state.contractLookup.loading ? 'Checking…' : 'Check Status'}
                        </button>
                    </form>
                    ${contractLookupResultMarkup()}
                </div>

                <div class="charts-grid">
                    <div class="panel">
                        <div class="panel-head">
                            <span class="panel-title">Blocks per Validator</span>
                            <span style="font-size:0.75rem;color:var(--text-muted);">Last ${BLOCKS_TO_SCAN} blocks</span>
                        </div>
                        <div class="chart-container">
                            <div class="chart-canvas-wrap">
                                <canvas id="netBlockChart" height="280"></canvas>
                            </div>
                        </div>
                        <div class="chart-legend" id="netBlockLegend"></div>
                    </div>
                    <div class="panel">
                        <div class="panel-head">
                            <span class="panel-title">Stake Distribution</span>
                        </div>
                        <div class="chart-container">
                            <div class="chart-canvas-wrap" style="display:flex;justify-content:center;">
                                <canvas id="netPieChart" width="260" height="260"></canvas>
                                <div class="pie-center">
                                    <div class="pie-center-value">${formatPrimShort(totalStake().toString())}</div>
                                    <div class="pie-center-label">Total PRIM</div>
                                </div>
                            </div>
                        </div>
                        <div class="chart-legend" id="netPieLegend"></div>
                    </div>
                </div>

                <div class="panel">
                    <div class="panel-head">
                        <span class="panel-title">Block Time History</span>
                        <span style="font-size:0.75rem;color:var(--text-muted);">Last ${state.blockTimes.length} blocks</span>
                    </div>
                    <div class="chart-container">
                        <div class="chart-canvas-wrap">
                            <canvas id="netBlockTimeChart" height="160"></canvas>
                        </div>
                    </div>
                </div>

                <div class="panel">
                    <div class="panel-head">
                        <span class="panel-title">Block Production Breakdown</span>
                    </div>
                    <div class="overflow-x">
                        <table class="data-table">
                            <thead>
                                <tr>
                                    <th>#</th>
                                    <th>Proposer</th>
                                    <th>Blocks</th>
                                    <th>% of Total</th>
                                    <th>Visual</th>
                                </tr>
                            </thead>
                            <tbody>
                                ${producerTableRows(totalBlocks)}
                            </tbody>
                        </table>
                    </div>
                </div>

                <div class="panel">
                    <div class="panel-head">
                        <span class="panel-title">Recent Blocks</span>
                        <span style="font-size:0.75rem;color:var(--text-muted);">Latest ${Math.min(state.recentBlocks.length, 20)} blocks</span>
                    </div>
                    <div class="overflow-x">
                        <table class="data-table">
                            <thead>
                                <tr>
                                    <th>Block</th>
                                    <th>Proposer</th>
                                    <th>Txns</th>
                                    <th>Gas Used</th>
                                    <th>Time</th>
                                </tr>
                            </thead>
                            <tbody>
                                ${recentBlockRows()}
                            </tbody>
                        </table>
                    </div>
                </div>
            </div>
        `;

        requestAnimationFrame(() => {
            drawBlockChart('netBlockChart', 'netBlockLegend');
            drawPieChart('netPieChart', 'netPieLegend');
            drawBlockTimeChart('netBlockTimeChart');
            const lookupForm = document.getElementById('contractLookupForm');
            if (lookupForm) {
                lookupForm.addEventListener('submit', handleContractLookupSubmit, { once: true });
            }
        });
    }

    function producerTableRows(totalBlocks) {
        const entries = Object.entries(state.blockProducers)
            .sort((a, b) => b[1] - a[1]);

        if (!entries.length) {
            return '<tr><td colspan="5" class="empty-state"><div class="empty-state-text">No block data available</div></td></tr>';
        }

        return entries.map(([addr, count], i) => {
            const pct = totalBlocks > 0 ? ((count / totalBlocks) * 100) : 0;
            const gradientClass = ['gradient-1', 'gradient-2', 'gradient-3'][i % 3];
            const name = validatorName(addr);
            return `
                <tr onclick="location.hash='#/validator/${addr}'">
                    <td class="td-mono">${i + 1}</td>
                    <td>
                        <div style="display:flex;flex-direction:column;gap:2px;">
                            ${name ? '<span style="font-weight:600;font-size:0.85rem;">' + name + '</span>' : ''}
                            <span class="td-addr">${truncAddr(addr)}</span>
                        </div>
                    </td>
                    <td class="td-mono">${count}</td>
                    <td class="td-mono">${pct.toFixed(1)}%</td>
                    <td>
                        <div class="stake-bar" style="min-width:120px;">
                            <div class="stake-bar-fill ${gradientClass}" style="width:${Math.max(pct, 1)}%"></div>
                        </div>
                    </td>
                </tr>
            `;
        }).join('');
    }

    function recentBlockRows() {
        const blocks = state.recentBlocks.slice(0, 20);
        if (!blocks.length) {
            return '<tr><td colspan="5" class="empty-state"><div class="empty-state-text">No blocks yet</div></td></tr>';
        }
        return blocks.map(b => {
            const num = parseInt(b.number, 16);
            const ts = parseInt(b.timestamp, 16);
            const txCount = (b.transactions || []).length;
            const proposer = (b.proposer || b.miner || '').toLowerCase();
            const name = validatorName(proposer);
            const gasUsed = parseInt(b.gasUsed || '0x0', 16);
            const gasLimit = parseInt(b.gasLimit || '0x0', 16);
            const gasPct = gasLimit > 0 ? ((gasUsed / gasLimit) * 100).toFixed(1) : '0';
            return `
                <tr>
                    <td><a href="${EXPLORER_BASE}/#/block/${num}" target="_blank" style="color:var(--accent);font-weight:600;">#${num.toLocaleString()}</a></td>
                    <td>
                        <div style="display:flex;flex-direction:column;gap:2px;">
                            ${name ? '<span style="font-weight:600;font-size:0.82rem;">' + name + '</span>' : ''}
                            <span class="td-addr">${truncAddr(proposer)}</span>
                        </div>
                    </td>
                    <td class="td-mono">${txCount}</td>
                    <td class="td-mono">${gasPct}%</td>
                    <td style="color:var(--text-muted);font-size:0.8rem;">${timeAgo(ts)}</td>
                </tr>
            `;
        }).join('');
    }

    function drawBlockTimeChart(canvasId) {
        const canvas = document.getElementById(canvasId);
        if (!canvas || state.blockTimes.length < 2) return;

        const ctx = canvas.getContext('2d');
        const dpr = window.devicePixelRatio || 1;
        const rect = canvas.parentElement.getBoundingClientRect();
        const w = rect.width;
        const h = canvas.height;

        canvas.width = w * dpr;
        canvas.height = h * dpr;
        canvas.style.width = w + 'px';
        canvas.style.height = h + 'px';
        ctx.scale(dpr, dpr);

        const times = state.blockTimes.slice(-50);
        const maxTime = Math.max(...times, 5);
        const minTime = Math.min(...times, 0);
        const padding = { top: 15, right: 15, bottom: 25, left: 40 };
        const chartW = w - padding.left - padding.right;
        const chartH = h - padding.top - padding.bottom;

        ctx.clearRect(0, 0, w, h);

        const gridColor = cssVar('--border') || '#F1F5F7';
        const mutedColor = cssVar('--text-secondary') || '#99B2C6';
        const lineColor = cssVar('--color-secondary') || '#6A2FFF';

        ctx.strokeStyle = gridColor;
        ctx.lineWidth = 1;
        for (let i = 0; i <= 3; i++) {
            const y = padding.top + (chartH / 3) * i;
            ctx.beginPath();
            ctx.moveTo(padding.left, y);
            ctx.lineTo(w - padding.right, y);
            ctx.stroke();

            const val = maxTime - (maxTime - minTime) * (i / 3);
            ctx.fillStyle = mutedColor;
            ctx.font = '10px "JetBrains Mono", monospace';
            ctx.textAlign = 'right';
            ctx.fillText(val.toFixed(1) + 's', padding.left - 6, y + 4);
        }

        const points = times.map((t, i) => ({
            x: padding.left + (i / (times.length - 1)) * chartW,
            y: padding.top + ((maxTime - t) / (maxTime - minTime || 1)) * chartH,
        }));

        ctx.beginPath();
        points.forEach((p, i) => i === 0 ? ctx.moveTo(p.x, p.y) : ctx.lineTo(p.x, p.y));
        ctx.strokeStyle = lineColor;
        ctx.lineWidth = 2;
        ctx.stroke();

        const gradient = ctx.createLinearGradient(0, padding.top, 0, padding.top + chartH);
        gradient.addColorStop(0, 'rgba(106,47,255,0.15)');
        gradient.addColorStop(1, 'rgba(106,47,255,0)');
        ctx.beginPath();
        points.forEach((p, i) => i === 0 ? ctx.moveTo(p.x, p.y) : ctx.lineTo(p.x, p.y));
        ctx.lineTo(points[points.length - 1].x, padding.top + chartH);
        ctx.lineTo(points[0].x, padding.top + chartH);
        ctx.closePath();
        ctx.fillStyle = gradient;
        ctx.fill();

        points.forEach(p => {
            ctx.beginPath();
            ctx.arc(p.x, p.y, 2.5, 0, Math.PI * 2);
            ctx.fillStyle = lineColor;
            ctx.fill();
        });

        ctx.fillStyle = mutedColor;
        ctx.font = '9px Sora, sans-serif';
        ctx.textAlign = 'center';
        ctx.fillText('Oldest', padding.left, padding.top + chartH + 16);
        ctx.fillText('Latest', w - padding.right, padding.top + chartH + 16);
    }

    // ── Canvas Charts ───────────────────────────────────────────────────
    function drawBlockChart(canvasId, legendId) {
        const canvas = document.getElementById(canvasId);
        const legendEl = document.getElementById(legendId);
        if (!canvas) return;

        const ctx = canvas.getContext('2d');
        const dpr = window.devicePixelRatio || 1;
        const rect = canvas.parentElement.getBoundingClientRect();
        const w = rect.width;
        const h = canvas.height;

        canvas.width = w * dpr;
        canvas.height = h * dpr;
        canvas.style.width = w + 'px';
        canvas.style.height = h + 'px';
        ctx.scale(dpr, dpr);

        const entries = Object.entries(state.blockProducers)
            .sort((a, b) => b[1] - a[1])
            .slice(0, 15);

        if (!entries.length) {
            ctx.fillStyle = cssVar('--text-secondary') || '#99B2C6';
            ctx.font = '14px Sora, sans-serif';
            ctx.textAlign = 'center';
            ctx.fillText('Collecting block data...', w / 2, h / 2);
            return;
        }

        const maxVal = Math.max(...entries.map(e => e[1]));
        const padding = { top: 20, right: 20, bottom: 40, left: 20 };
        const chartW = w - padding.left - padding.right;
        const chartH = h - padding.top - padding.bottom;
        const barWidth = Math.min(40, (chartW / entries.length) * 0.7);
        const gap = (chartW - barWidth * entries.length) / (entries.length + 1);

        ctx.clearRect(0, 0, w, h);

        const bGridColor = cssVar('--border') || '#F1F5F7';
        ctx.strokeStyle = bGridColor;
        ctx.lineWidth = 1;
        for (let i = 0; i <= 4; i++) {
            const y = padding.top + (chartH / 4) * i;
            ctx.beginPath();
            ctx.moveTo(padding.left, y);
            ctx.lineTo(w - padding.right, y);
            ctx.stroke();
        }

        // Bars
        entries.forEach(([addr, count], i) => {
            const barH = maxVal > 0 ? (count / maxVal) * chartH : 0;
            const x = padding.left + gap + i * (barWidth + gap);
            const y = padding.top + chartH - barH;

            const gradient = ctx.createLinearGradient(x, y, x, padding.top + chartH);
            gradient.addColorStop(0, CHART_COLORS[i % CHART_COLORS.length]);
            gradient.addColorStop(1, 'rgba(148,97,255,0.05)');

            ctx.beginPath();
            const r = Math.min(4, barWidth / 2);
            ctx.moveTo(x + r, y);
            ctx.lineTo(x + barWidth - r, y);
            ctx.quadraticCurveTo(x + barWidth, y, x + barWidth, y + r);
            ctx.lineTo(x + barWidth, padding.top + chartH);
            ctx.lineTo(x, padding.top + chartH);
            ctx.lineTo(x, y + r);
            ctx.quadraticCurveTo(x, y, x + r, y);
            ctx.fillStyle = gradient;
            ctx.fill();

            ctx.fillStyle = cssVar('--text-primary') || '#3A334D';
            ctx.font = '600 10px "JetBrains Mono", monospace';
            ctx.textAlign = 'center';
            ctx.fillText(count.toString(), x + barWidth / 2, y - 6);

            ctx.fillStyle = cssVar('--text-secondary') || '#99B2C6';
            ctx.font = '500 9px "JetBrains Mono", monospace';
            const chartLabel = validatorName(addr) || truncAddr(addr).slice(0, 8);
            ctx.fillText(chartLabel.slice(0, 12), x + barWidth / 2, padding.top + chartH + 16);
        });

        if (legendEl) {
            legendEl.innerHTML = entries.map(([addr, count], i) => `
                <div class="chart-legend-item">
                    <span class="chart-legend-dot" style="background:${CHART_COLORS[i % CHART_COLORS.length]}"></span>
                    ${displayAddr(addr)} (${count})
                </div>
            `).join('');
        }
    }

    function drawPieChart(canvasId, legendId) {
        const canvas = document.getElementById(canvasId);
        const legendEl = document.getElementById(legendId);
        if (!canvas) return;

        const ctx = canvas.getContext('2d');
        const dpr = window.devicePixelRatio || 1;
        const size = parseInt(canvas.width) || 220;

        canvas.width = size * dpr;
        canvas.height = size * dpr;
        canvas.style.width = size + 'px';
        canvas.style.height = size + 'px';
        ctx.scale(dpr, dpr);

        const total = totalStake();
        const validators = state.validators.slice(0, 15);

        if (!validators.length || total === BigInt(0)) {
            ctx.fillStyle = cssVar('--text-secondary') || '#99B2C6';
            ctx.font = '14px Sora, sans-serif';
            ctx.textAlign = 'center';
            ctx.fillText('No data', size / 2, size / 2);
            return;
        }

        const cx = size / 2;
        const cy = size / 2;
        const outerR = size / 2 - 8;
        const innerR = outerR * 0.58;

        let startAngle = -Math.PI / 2;

        validators.forEach((v, i) => {
            const pct = Number((BigInt(v.stake) * BigInt(10000)) / total) / 10000;
            const sweep = pct * Math.PI * 2;
            const endAngle = startAngle + sweep;

            ctx.beginPath();
            ctx.arc(cx, cy, outerR, startAngle, endAngle);
            ctx.arc(cx, cy, innerR, endAngle, startAngle, true);
            ctx.closePath();
            ctx.fillStyle = CHART_COLORS[i % CHART_COLORS.length];
            ctx.fill();

            // Slight separator
            ctx.strokeStyle = '#0b0b12';
            ctx.lineWidth = 2;
            ctx.stroke();

            startAngle = endAngle;
        });

        // Handle "others" if validators were truncated
        if (state.validators.length > 15) {
            const othersStake = state.validators.slice(15).reduce((a, v) => a + BigInt(v.stake || '0'), BigInt(0));
            const pct = Number((othersStake * BigInt(10000)) / total) / 10000;
            const sweep = pct * Math.PI * 2;
            const endAngle = startAngle + sweep;

            ctx.beginPath();
            ctx.arc(cx, cy, outerR, startAngle, endAngle);
            ctx.arc(cx, cy, innerR, endAngle, startAngle, true);
            ctx.closePath();
            ctx.fillStyle = '#334155';
            ctx.fill();
            ctx.strokeStyle = '#0b0b12';
            ctx.lineWidth = 2;
            ctx.stroke();
        }

        if (legendEl) {
            const items = validators.map((v, i) => `
                <div class="chart-legend-item">
                    <span class="chart-legend-dot" style="background:${CHART_COLORS[i % CHART_COLORS.length]}"></span>
                    ${displayAddr(v.address)} (${stakePct(v.stake).toFixed(1)}%)
                </div>
            `);
            if (state.validators.length > 15) {
                items.push(`
                    <div class="chart-legend-item">
                        <span class="chart-legend-dot" style="background:#334155"></span>
                        Others
                    </div>
                `);
            }
            legendEl.innerHTML = items.join('');
        }
    }

    // ── Initialization ──────────────────────────────────────────────────
    function init() {
        window.addEventListener('hashchange', renderCurrentPage);

        var mobileNavBtn = document.getElementById('mobileNavBtn');
        var nav = document.querySelector('.nav');
        if (mobileNavBtn && nav) {
            mobileNavBtn.addEventListener('click', function() {
                nav.classList.toggle('mobile-open');
            });
            nav.addEventListener('click', function(e) {
                if (e.target.closest('.nav-link')) nav.classList.remove('mobile-open');
            });
        }

        if (!location.hash || location.hash === '#' || location.hash === '#/') {
            location.hash = '#/dashboard';
        }

        startPolling();
    }

    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', init);
    } else {
        init();
    }
})();
