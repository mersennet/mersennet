(function () {
    'use strict';

    const RPC_URL = (function () {
        const params = new URLSearchParams(location.search || '');
        return params.get('rpc') || 'http://localhost:9944';
    })();
    const POLL_INTERVAL_MS = 2000;
    const BLOCKS_TO_FETCH = 12;
    const TXS_TO_SHOW = 50;

    function truncateHash(hash, start = 6, end = 4) {
        if (!hash || typeof hash !== 'string') return '—';
        const s = hash.startsWith('0x') ? hash.slice(2) : hash;
        if (s.length <= start + end) return hash;
        return '0x' + s.slice(0, start) + '…' + s.slice(-end);
    }

    function formatNumber(n) {
        if (n === undefined || n === null) return '—';
        const num = typeof n === 'string' ? (n.startsWith('0x') ? parseInt(n, 16) : parseInt(n, 10)) : Number(n);
        if (isNaN(num)) return String(n);
        return num.toLocaleString();
    }

    function hexToBigInt(hex) {
        if (!hex || typeof hex !== 'string') return 0n;
        const s = hex.startsWith('0x') ? hex.slice(2) : hex;
        return BigInt('0x' + (s || '0'));
    }

    function formatWei(hex) {
        const n = hexToBigInt(hex);
        if (n === 0n) return '0';
        if (n < 1000n) return n.toString();
        if (n < 1_000_000n) return (Number(n) / 1e3).toFixed(2) + ' K';
        if (n < 1_000_000_000n) return (Number(n) / 1e6).toFixed(2) + ' M';
        if (n < 1_000_000_000_000n) return (Number(n) / 1e9).toFixed(2) + ' G';
        return (Number(n) / 1e18).toFixed(4) + ' ETH';
    }

    function formatGwei(hex) {
        const n = hexToBigInt(hex);
        return (Number(n) / 1e9).toFixed(2) + ' Gwei';
    }

    function relativeTime(blockNum, latestBlock) {
        if (blockNum === undefined || latestBlock === undefined) return '—';
        const diff = Number(latestBlock) - Number(blockNum);
        if (diff === 0) return 'Just now';
        if (diff === 1) return '1 block ago';
        if (diff < 10) return diff + ' blocks ago';
        return 'Block #' + blockNum;
    }

    function parseBlockNum(hex) {
        if (!hex) return 0;
        const s = typeof hex === 'string' ? (hex.startsWith('0x') ? hex.slice(2) : hex) : String(hex);
        return parseInt(s, 16) || 0;
    }

    class PrimeChainExplorer {
        constructor() {
            this.currentTab = 'blocks';
            this.blocks = [];
            this.transactions = [];
            this.chainInfo = {};
            this.validators = [];
            this.orderBook = null;
            this.lastBlockNum = 0;
            this.pollTimer = null;
        }

        async rpc(method, params = []) {
            const response = await fetch(RPC_URL, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({
                    jsonrpc: '2.0',
                    id: Date.now(),
                    method,
                    params: Array.isArray(params) ? params : [params],
                }),
            });
            const data = await response.json();
            if (data.error) throw new Error(data.error.message || 'RPC error');
            return data.result;
        }

        async loadChainInfo() {
            try {
                const [chainId, blockNum, gasPrice] = await Promise.all([
                    this.rpc('eth_chainId'),
                    this.rpc('prime_blockNumber'),
                    this.rpc('eth_gasPrice'),
                ]);
                this.chainInfo = {
                    chainId: parseBlockNum(chainId),
                    blockNumber: parseBlockNum(blockNum),
                    baseFee: gasPrice,
                };
                return this.chainInfo;
            } catch (e) {
                console.error('loadChainInfo', e);
            }
        }

        async loadBlocks() {
            try {
                const latestHex = await this.rpc('prime_blockNumber');
                const latest = parseBlockNum(latestHex);
                const blocks = [];
                for (let i = 0; i < BLOCKS_TO_FETCH && latest >= i; i++) {
                    const num = latest - i;
                    const block = await this.rpc('prime_getBlockByNumber', ['0x' + num.toString(16), false]);
                    if (block) blocks.push(block);
                }
                const hadNew = this.blocks.length > 0 && blocks.length > 0 &&
                    parseBlockNum(blocks[0].number) > parseBlockNum(this.blocks[0]?.number);
                this.blocks = blocks;
                return { blocks, hadNew };
            } catch (e) {
                console.error('loadBlocks', e);
                return { blocks: [], hadNew: false };
            }
        }

        async loadBlockByNumber(num) {
            const hex = typeof num === 'number' ? '0x' + num.toString(16) : num;
            return this.rpc('prime_getBlockByNumber', [hex, true]);
        }

        async loadTransactions() {
            try {
                const latestHex = await this.rpc('prime_blockNumber');
                const latest = parseBlockNum(latestHex);
                const allTxs = [];
                for (let i = 0; i < 10 && latest >= i; i++) {
                    const num = latest - i;
                    const block = await this.rpc('prime_getBlockByNumber', ['0x' + num.toString(16), true]);
                    if (!block || !block.transactions) continue;
                    for (let j = 0; j < block.transactions.length; j++) {
                        const tx = block.transactions[j];
                        if (typeof tx === 'object' && tx.hash) {
                            allTxs.push({
                                ...tx,
                                blockNumber: block.number,
                                blockHash: block.hash,
                            });
                        }
                    }
                }
                const txHashes = new Set();
                this.transactions = allTxs.filter((t) => {
                    if (txHashes.has(t.hash)) return false;
                    txHashes.add(t.hash);
                    return true;
                }).slice(0, TXS_TO_SHOW);
                return this.transactions;
            } catch (e) {
                console.error('loadTransactions', e);
                return [];
            }
        }

        async loadTxReceipt(txHash) {
            return this.rpc('eth_getTransactionReceipt', [txHash]);
        }

        async loadValidators() {
            try {
                const list = await this.rpc('prime_validators');
                this.validators = Array.isArray(list) ? list : [];
                return this.validators;
            } catch (e) {
                console.error('loadValidators', e);
                this.validators = [];
                return [];
            }
        }

        async loadOrderBook(marketId) {
            try {
                const id = typeof marketId === 'number' ? marketId : parseInt(marketId, 10);
                const book = await this.rpc('primeorders_getOrderBook', ['0x' + id.toString(16)]);
                this.orderBook = book;
                return book;
            } catch (e) {
                console.error('loadOrderBook', e);
                this.orderBook = null;
                return null;
            }
        }

        renderChainInfo() {
            const c = this.chainInfo;
            const el = (id) => document.getElementById(id);
            if (el('chainId')) el('chainId').textContent = c.chainId != null ? String(c.chainId) : '—';
            if (el('currentBlock')) el('currentBlock').textContent = formatNumber(c.blockNumber);
            if (el('baseFee')) el('baseFee').textContent = c.baseFee ? formatGwei(c.baseFee) : '—';
        }

        renderBlocks(hadNew = false) {
            const container = document.getElementById('blocksList');
            if (!container) return;
            if (this.blocks.length === 0) {
                container.innerHTML = '<div class="loading">No blocks found. Is the node running?</div>';
                return;
            }
            const latest = parseBlockNum(this.chainInfo.blockNumber);
            container.innerHTML = this.blocks.map((b, i) => {
                const num = parseBlockNum(b.number);
                const txCount = Array.isArray(b.transactions) ? b.transactions.length : 0;
                const isNew = hadNew && i === 0;
                return `
                    <article class="block-card ${isNew ? 'new' : ''}" data-block="${num}">
                        <div class="block-number">#${formatNumber(num)}</div>
                        <div class="block-hash">${truncateHash(b.hash)}</div>
                        <div class="block-meta">
                            <span>${txCount} tx</span>
                            <span>${formatNumber(b.gas_used)} gas</span>
                            <span>${relativeTime(num, latest)}</span>
                        </div>
                    </article>
                `;
            }).join('');
            container.querySelectorAll('.block-card').forEach((card) => {
                card.addEventListener('click', () => this.showBlockDetail(parseInt(card.dataset.block, 10)));
            });
        }

        async showBlockDetail(num) {
            const overlay = document.getElementById('blockDetailOverlay');
            const content = document.getElementById('blockDetailContent');
            if (!overlay || !content) return;
            content.innerHTML = '<div class="loading">Loading block…</div>';
            overlay.classList.add('visible');
            overlay.setAttribute('aria-hidden', 'false');
            try {
                const block = await this.loadBlockByNumber(num);
                if (!block) {
                    content.innerHTML = '<div class="error-msg">Block not found.</div>';
                    return;
                }
                const txList = Array.isArray(block.transactions)
                    ? block.transactions.map((tx) => {
                        const h = typeof tx === 'object' ? tx.hash : tx;
                        return `<a href="#" class="detail-value tx-link" data-hash="${h}">${truncateHash(h)}</a>`;
                    }).join(', ')
                    : '—';
                content.innerHTML = `
                    <div class="detail-row"><span class="detail-label">Number</span><span class="detail-value">#${formatNumber(parseBlockNum(block.number))}</span></div>
                    <div class="detail-row"><span class="detail-label">Hash</span><span class="detail-value">${block.hash || '—'}</span></div>
                    <div class="detail-row"><span class="detail-label">Gas Used</span><span class="detail-value">${formatNumber(block.gas_used)}</span></div>
                    <div class="detail-row"><span class="detail-label">Gas Limit</span><span class="detail-value">${formatNumber(block.gas_limit)}</span></div>
                    <div class="detail-row"><span class="detail-label">Base Fee</span><span class="detail-value">${block.base_fee ? formatGwei(block.base_fee) : '—'}</span></div>
                    <div class="detail-row"><span class="detail-label">Transactions</span><span class="detail-value">${txList || '—'}</span></div>
                `;
                content.querySelectorAll('.tx-link').forEach((a) => {
                    a.addEventListener('click', (e) => {
                        e.preventDefault();
                        this.showTxDetail(a.dataset.hash);
                    });
                });
            } catch (e) {
                content.innerHTML = '<div class="error-msg">' + (e.message || 'Failed to load block') + '</div>';
            }
        }

        async showTxDetail(txHash) {
            const overlay = document.getElementById('txDetailOverlay');
            const content = document.getElementById('txDetailContent');
            const blockOverlay = document.getElementById('blockDetailOverlay');
            if (blockOverlay) blockOverlay.classList.remove('visible');
            if (!overlay || !content) return;
            content.innerHTML = '<div class="loading">Loading transaction…</div>';
            overlay.classList.add('visible');
            overlay.setAttribute('aria-hidden', 'false');
            try {
                const [tx, receipt] = await Promise.all([
                    this.rpc('eth_getTransactionByHash', [txHash]),
                    this.rpc('eth_getTransactionReceipt', [txHash]),
                ]);
                if (!tx) {
                    content.innerHTML = '<div class="error-msg">Transaction not found.</div>';
                    return;
                }
                const status = receipt ? (receipt.status === '0x1' ? 'Success' : 'Failed') : '—';
                const gasUsed = receipt ? formatNumber(receipt.gas_used) : '—';
                content.innerHTML = `
                    <div class="detail-row"><span class="detail-label">Hash</span><span class="detail-value">${tx.hash || '—'}</span></div>
                    <div class="detail-row"><span class="detail-label">From</span><span class="detail-value">${tx.from || '—'}</span></div>
                    <div class="detail-row"><span class="detail-label">To</span><span class="detail-value">${tx.to || 'Contract Creation'}</span></div>
                    <div class="detail-row"><span class="detail-label">Value</span><span class="detail-value">${formatWei(tx.value)}</span></div>
                    <div class="detail-row"><span class="detail-label">Gas Used</span><span class="detail-value">${gasUsed}</span></div>
                    <div class="detail-row"><span class="detail-label">Status</span><span class="detail-value ${status === 'Success' ? 'status-success' : 'status-fail'}">${status}</span></div>
                `;
            } catch (e) {
                content.innerHTML = '<div class="error-msg">' + (e.message || 'Failed to load transaction') + '</div>';
            }
        }

        renderTransactions() {
            const tbody = document.getElementById('transactionsList');
            if (!tbody) return;
            if (this.transactions.length === 0) {
                tbody.innerHTML = '<tr><td colspan="6" class="loading">No transactions found.</td></tr>';
                return;
            }
            tbody.innerHTML = this.transactions.map((tx) => {
                const receipt = tx.receipt;
                const status = receipt ? (String(receipt.status) === '0x1' ? 'Success' : 'Failed') : '…';
                const gasUsed = receipt ? formatNumber(receipt.gas_used) : '…';
                return `
                    <tr class="tx-row" data-hash="${tx.hash}">
                        <td class="hash">${truncateHash(tx.hash)}</td>
                        <td class="address">${truncateHash(tx.from)}</td>
                        <td class="address">${tx.to ? truncateHash(tx.to) : 'Contract'}</td>
                        <td>${formatWei(tx.value)}</td>
                        <td>${gasUsed}</td>
                        <td><span class="${status === 'Success' ? 'status-success' : 'status-fail'}">${status}</span></td>
                    </tr>
                `;
            }).join('');
            tbody.querySelectorAll('.tx-row').forEach((row) => {
                row.addEventListener('click', () => this.showTxDetail(row.dataset.hash));
            });
        }

        async enrichTransactionsWithReceipts() {
            const limit = Math.min(this.transactions.length, 15);
            const toFetch = [];
            for (let i = 0; i < limit; i++) {
                const tx = this.transactions[i];
                if (tx.receipt) continue;
                toFetch.push(this.loadTxReceipt(tx.hash).then((r) => { tx.receipt = r; }).catch(() => {}));
            }
            await Promise.all(toFetch);
            this.renderTransactions();
        }

        renderOrderBook(book) {
            const bidsEl = document.getElementById('bidsList');
            const asksEl = document.getElementById('asksList');
            const spreadEl = document.getElementById('spreadValue');
            const emptyEl = document.getElementById('orderbookEmpty');
            if (!bidsEl || !asksEl) return;
            if (!book || (!book.bids?.length && !book.asks?.length)) {
                bidsEl.innerHTML = '';
                asksEl.innerHTML = '';
                if (spreadEl) spreadEl.textContent = '—';
                if (emptyEl) emptyEl.classList.add('visible');
                return;
            }
            if (emptyEl) emptyEl.classList.remove('visible');
            const bids = book.bids || [];
            const asks = book.asks || [];
            const maxSize = Math.max(
                ...bids.map((l) => Number(hexToBigInt(l.size))),
                ...asks.map((l) => Number(hexToBigInt(l.size))),
                1
            );
            const levelHtml = (levels, side) =>
                levels.map((l) => {
                    const size = hexToBigInt(l.size);
                    const pct = (Number(size) / maxSize) * 100;
                    return `
                        <div class="orderbook-level ${side}">
                            <span>${formatWei(l.price)}</span>
                            <span>${formatNumber(size)}</span>
                            <div class="orderbook-depth-bar ${side}" style="width: ${Math.max(pct, 5)}%"></div>
                        </div>
                    `;
                }).join('');
            bidsEl.innerHTML = levelHtml(bids.slice(0, 15), 'bid');
            asksEl.innerHTML = levelHtml(asks.slice(0, 15), 'ask');
            const bestBid = bids[0] ? hexToBigInt(bids[0].price) : 0n;
            const bestAsk = asks[0] ? hexToBigInt(asks[0].price) : 0n;
            if (spreadEl) {
                spreadEl.textContent = bestBid && bestAsk
                    ? 'Spread: ' + formatWei('0x' + (bestAsk - bestBid).toString(16))
                    : '—';
            }
        }

        renderValidators() {
            const tbody = document.getElementById('validatorsList');
            const totalEl = document.getElementById('totalStake');
            if (!tbody) return;
            if (this.validators.length === 0) {
                tbody.innerHTML = '<tr><td colspan="3" class="loading">No validators or RPC method not available.</td></tr>';
                if (totalEl) totalEl.textContent = 'Total Stake: —';
                return;
            }
            let total = 0n;
            this.validators.forEach((v) => {
                total += hexToBigInt(v.stake);
            });
            if (totalEl) totalEl.textContent = 'Total Stake: ' + formatWei('0x' + total.toString(16));
            tbody.innerHTML = this.validators.map((v) => `
                <tr>
                    <td class="address">${v.address}</td>
                    <td>${formatWei(v.stake)}</td>
                    <td><span class="status-success">Active</span></td>
                </tr>
            `).join('');
        }

        switchTab(tab) {
            this.currentTab = tab;
            document.querySelectorAll('.tab').forEach((t) => t.classList.remove('active'));
            document.querySelectorAll('.tab-panel').forEach((p) => p.classList.remove('active'));
            const tabBtn = document.querySelector(`.tab[data-tab="${tab}"]`);
            const panel = document.getElementById(`panel-${tab}`);
            if (tabBtn) tabBtn.classList.add('active');
            if (panel) panel.classList.add('active');
            if (tab === 'orderbook') {
                const marketId = parseInt(document.getElementById('marketSelect')?.value || '0', 10);
                this.loadOrderBook(marketId).then((b) => this.renderOrderBook(b));
            }
        }

        async searchHandler(query) {
            const q = (query || '').trim();
            if (!q) return;
            const lower = q.toLowerCase();
            if (/^\d+$/.test(q)) {
                const num = parseInt(q, 10);
                this.switchTab('blocks');
                await this.showBlockDetail(num);
                this.toast('Block #' + num);
                return;
            }
            if (lower.startsWith('0x') && q.length === 66) {
                this.switchTab('transactions');
                await this.showTxDetail(q);
                this.toast('Transaction');
                return;
            }
            if (lower.startsWith('0x') && q.length === 42) {
                this.switchTab('transactions');
                this.toast('Address search: show transactions involving ' + truncateHash(q) + ' (filter not yet implemented)');
                return;
            }
            this.toast('Enter block number, tx hash (0x…), or address (0x…)', true);
        }

        toast(msg, isError = false) {
            const el = document.getElementById('toast');
            if (!el) return;
            el.textContent = msg;
            el.classList.toggle('error', isError);
            el.classList.add('visible');
            clearTimeout(this._toastTimer);
            this._toastTimer = setTimeout(() => el.classList.remove('visible'), 3000);
        }

        async poll() {
            await this.loadChainInfo();
            this.renderChainInfo();
            const { blocks, hadNew } = await this.loadBlocks();
            this.renderBlocks(hadNew);
            if (this.currentTab === 'transactions') {
                await this.loadTransactions();
                this.renderTransactions();
                this.enrichTransactionsWithReceipts();
            }
            if (this.currentTab === 'validators') {
                await this.loadValidators();
                this.renderValidators();
            }
            const marketId = parseInt(document.getElementById('marketSelect')?.value || '0', 10);
            if (this.currentTab === 'orderbook') {
                const book = await this.loadOrderBook(marketId);
                this.renderOrderBook(book);
            }
            const refreshBlocks = document.getElementById('blocksRefresh');
            if (refreshBlocks) refreshBlocks.textContent = 'Updated ' + new Date().toLocaleTimeString();
        }

        startPolling() {
            this.poll();
            this.pollTimer = setInterval(() => this.poll(), POLL_INTERVAL_MS);
        }

        stopPolling() {
            if (this.pollTimer) {
                clearInterval(this.pollTimer);
                this.pollTimer = null;
            }
        }
    }

    const explorer = new PrimeChainExplorer();

    document.addEventListener('DOMContentLoaded', () => {
        document.querySelectorAll('.tab').forEach((btn) => {
            btn.addEventListener('click', () => {
                explorer.switchTab(btn.dataset.tab);
            });
        });
        const searchInput = document.getElementById('searchInput');
        const searchBtn = document.getElementById('searchBtn');
        const doSearch = () => explorer.searchHandler(searchInput?.value);
        searchBtn?.addEventListener('click', doSearch);
        searchInput?.addEventListener('keydown', (e) => {
            if (e.key === 'Enter') doSearch();
        });
        document.getElementById('closeBlockDetail')?.addEventListener('click', () => {
            document.getElementById('blockDetailOverlay')?.classList.remove('visible');
        });
        document.getElementById('closeTxDetail')?.addEventListener('click', () => {
            document.getElementById('txDetailOverlay')?.classList.remove('visible');
        });
        document.getElementById('marketSelect')?.addEventListener('change', (e) => {
            explorer.loadOrderBook(parseInt(e.target.value, 10)).then((b) => explorer.renderOrderBook(b));
        });
        document.getElementById('blockDetailOverlay')?.addEventListener('click', (e) => {
            if (e.target.id === 'blockDetailOverlay') e.target.classList.remove('visible');
        });
        document.getElementById('txDetailOverlay')?.addEventListener('click', (e) => {
            if (e.target.id === 'txDetailOverlay') e.target.classList.remove('visible');
        });
        explorer.startPolling();
    });
})();
