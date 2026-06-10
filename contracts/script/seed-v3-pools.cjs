const RPC = process.env.RPC_URL || 'http://46.225.30.187:8545';
const FROM = '0x7f5ce38fb2553e95dd8ef9182a80bc219c9a0d45';
const { keccak_256 } = require('@noble/hashes/sha3');

const WMRSN = '0x079bf1207b51acda83e2e8178344f62a883f8479';
const USDC  = '0xb22f77d89122e9e3784bfd3eee9616273f38238d';
const USDT  = '0x877feca38919acd7aaf7cb81f100e0454aa95c17';
const DAI   = '0xb88d63a65691effbf4b6808325b1588912c15cf4';

const NFT_POSITION_MANAGER = '0x6c12f22a793e0560ba0387d808fef69299f6ccb6';

const FEE_MEDIUM = 3000;
const FEE_LOW    = 500;
const TICK_SPACING_500  = 10;
const TICK_SPACING_3000 = 60;

const E6  = 10n ** 6n;
const E18 = 10n ** 18n;
const TWO_96 = 2n ** 96n;
const MAX_UINT256 = (2n ** 256n) - 1n;

function keccak256(data) { return Buffer.from(keccak_256(data)); }
function fnSelector(sig) { return '0x' + keccak256(Buffer.from(sig)).slice(0, 4).toString('hex'); }
function encodeAddress(addr) { return addr.replace('0x', '').toLowerCase().padStart(64, '0'); }
function encodeUint256(n) { return BigInt(n).toString(16).padStart(64, '0'); }

function encodeInt24(n) {
    if (n >= 0) return BigInt(n).toString(16).padStart(64, '0');
    return ((2n ** 256n) + BigInt(n)).toString(16).padStart(64, '0');
}

function sortTokens(a, b) {
    return a.toLowerCase() < b.toLowerCase() ? [a, b] : [b, a];
}

function sqrt(n) {
    if (n <= 0n) return 0n;
    let x = n, y = (x + 1n) / 2n;
    while (y < x) { x = y; y = (x + n / x) / 2n; }
    return x;
}

function computeSqrtPriceX96(dec0, dec1) {
    const diff = BigInt(dec1) - BigInt(dec0);
    if (diff === 0n) return TWO_96;
    if (diff > 0n) return sqrt(10n ** diff) * TWO_96;
    return TWO_96 / sqrt(10n ** (-diff));
}

function getMinMaxTick(tickSpacing) {
    const min = Math.ceil(-887272 / tickSpacing) * tickSpacing;
    const max = Math.floor(887272 / tickSpacing) * tickSpacing;
    return { min, max };
}

async function rpc(method, params = []) {
    for (let attempt = 0; attempt < 5; attempt++) {
        try {
            const res = await fetch(RPC, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ jsonrpc: '2.0', id: Date.now(), method, params })
            });
            if (res.status === 429 || res.status >= 500) {
                console.log(`    [rate limited, retry ${attempt + 1}/5]`);
                await new Promise(r => setTimeout(r, 2000 * (attempt + 1)));
                continue;
            }
            const text = await res.text();
            let data;
            try { data = JSON.parse(text); } catch {
                console.log(`    [invalid JSON response, retry ${attempt + 1}/5]`);
                await new Promise(r => setTimeout(r, 2000 * (attempt + 1)));
                continue;
            }
            if (data.error) throw new Error(`${method}: ${data.error.message}`);
            return data.result;
        } catch (e) {
            if (e.message.includes(method + ':')) throw e;
            console.log(`    [fetch error: ${e.message}, retry ${attempt + 1}/5]`);
            await new Promise(r => setTimeout(r, 2000 * (attempt + 1)));
        }
    }
    throw new Error(`${method}: max retries exceeded`);
}

let nextNonce = null;
async function sendTx(to, data, value = '0x0') {
    await new Promise(r => setTimeout(r, 300));
    if (nextNonce === null) {
        nextNonce = parseInt(await rpc('eth_getTransactionCount', [FROM, 'latest']), 16);
    }
    const nonce = '0x' + nextNonce.toString(16);
    nextNonce++;

    const txHash = await rpc('eth_sendTransaction', [{
        from: FROM,
        to,
        data,
        value,
        gas: '0x1C9C380',
        gasPrice: '0x3B9ACA00',
        nonce
    }]);

    for (let i = 0; i < 30; i++) {
        await new Promise(r => setTimeout(r, 2000));
        const receipt = await rpc('eth_getTransactionReceipt', [txHash]);
        if (receipt) {
            const status = receipt.status;
            const gas = parseInt(receipt.gas_used || receipt.gasUsed || '0', 16);
            return { txHash, status, gas };
        }
    }
    return { txHash, status: 'timeout', gas: 0 };
}

async function mintTokens(token, totalAmount, decimals, label) {
    const maxPerCall = 10000n * (10n ** BigInt(decimals));
    let remaining = totalAmount;
    let batch = 1;
    while (remaining > 0n) {
        const amount = remaining > maxPerCall ? maxPerCall : remaining;
        const data = fnSelector('mint(address,uint256)') + encodeAddress(FROM) + encodeUint256(amount);
        const human = Number(amount / (10n ** BigInt(decimals)));
        console.log(`  Minting ${human} ${label} (batch ${batch})...`);
        const r = await sendTx(token, data);
        console.log(`    TX: ${r.txHash} status=${r.status}`);
        if (r.status !== '0x1') throw new Error(`Mint failed for ${label} batch ${batch}: ${r.status}`);
        remaining -= amount;
        batch++;
    }
}

async function approve(token, spender, label) {
    const data = fnSelector('approve(address,uint256)') + encodeAddress(spender) + encodeUint256(MAX_UINT256);
    console.log(`  Approving ${label}...`);
    const r = await sendTx(token, data);
    console.log(`    TX: ${r.txHash} status=${r.status}`);
    if (r.status !== '0x1') throw new Error(`Approve failed for ${label}`);
}

async function depositWPRIM(amount) {
    const data = fnSelector('deposit()');
    const value = '0x' + amount.toString(16);
    console.log(`  Wrapping ${Number(amount / E18)} MRSN -> WMRSN...`);
    const r = await sendTx(WMRSN, data, value);
    console.log(`    TX: ${r.txHash} status=${r.status}`);
    if (r.status !== '0x1') throw new Error('WMRSN deposit failed');
}

async function createPool(token0, token1, fee, sqrtPriceX96, label) {
    const data = fnSelector('createAndInitializePoolIfNecessary(address,address,uint24,uint160)')
        + encodeAddress(token0) + encodeAddress(token1) + encodeUint256(fee) + encodeUint256(sqrtPriceX96);
    console.log(`  Creating pool: ${label} (fee=${fee})...`);
    const r = await sendTx(NFT_POSITION_MANAGER, data);
    console.log(`    TX: ${r.txHash} status=${r.status} gas=${r.gas}`);
    if (r.status !== '0x1') console.log(`    WARNING: Pool may already exist or creation failed`);
    return r;
}

async function mintPosition(token0, token1, fee, tickLower, tickUpper, amount0, amount1, label) {
    const deadline = BigInt(Math.floor(Date.now() / 1000) + 7200);
    const data = fnSelector('mint((address,address,uint24,int24,int24,uint256,uint256,uint256,uint256,address,uint256))')
        + encodeAddress(token0)
        + encodeAddress(token1)
        + encodeUint256(fee)
        + encodeInt24(tickLower)
        + encodeInt24(tickUpper)
        + encodeUint256(amount0)
        + encodeUint256(amount1)
        + encodeUint256(0)
        + encodeUint256(0)
        + encodeAddress(FROM)
        + encodeUint256(deadline);

    console.log(`  Minting position: ${label}...`);
    const r = await sendTx(NFT_POSITION_MANAGER, data);
    console.log(`    TX: ${r.txHash} status=${r.status} gas=${r.gas}`);
    if (r.status !== '0x1') console.log(`    WARNING: Position mint failed`);
    return r;
}

const POOLS = [
    { a: WMRSN, b: USDC, dA: 18, dB: 6,  fee: FEE_MEDIUM, amtA: 10000n * E18, amtB: 10000n * E6,  name: 'WMRSN/USDC' },
    { a: WMRSN, b: USDT, dA: 18, dB: 6,  fee: FEE_MEDIUM, amtA: 10000n * E18, amtB: 10000n * E6,  name: 'WMRSN/USDT' },
    { a: WMRSN, b: DAI,  dA: 18, dB: 18, fee: FEE_MEDIUM, amtA: 10000n * E18, amtB: 10000n * E18, name: 'WMRSN/DAI' },
    { a: USDC,  b: USDT, dA: 6,  dB: 6,  fee: FEE_LOW,    amtA: 10000n * E6,  amtB: 10000n * E6,  name: 'USDC/USDT' },
    { a: USDC,  b: DAI,  dA: 6,  dB: 18, fee: FEE_LOW,    amtA: 10000n * E6,  amtB: 10000n * E18, name: 'USDC/DAI' },
    { a: USDT,  b: DAI,  dA: 6,  dB: 18, fee: FEE_LOW,    amtA: 10000n * E6,  amtB: 10000n * E18, name: 'USDT/DAI' },
];

async function main() {
    console.log(`Deployer: ${FROM}`);
    console.log(`RPC: ${RPC}`);
    console.log(`NFT Position Manager: ${NFT_POSITION_MANAGER}\n`);

    const balance = BigInt(await rpc('eth_getBalance', [FROM, 'latest']));
    console.log(`MRSN balance: ${balance / E18} MRSN\n`);

    console.log('=== Step 1: Mint test tokens ===');
    await mintTokens(USDC, 30000n * E6, 6, 'USDC');
    await mintTokens(USDT, 30000n * E6, 6, 'USDT');
    await mintTokens(DAI, 30000n * E18, 18, 'DAI');

    console.log('\n=== Step 2: Wrap MRSN -> WMRSN ===');
    await depositWPRIM(40000n * E18);

    console.log('\n=== Step 3: Approve NFT Position Manager ===');
    await approve(WMRSN, NFT_POSITION_MANAGER, 'WMRSN');
    await approve(USDC, NFT_POSITION_MANAGER, 'USDC');
    await approve(USDT, NFT_POSITION_MANAGER, 'USDT');
    await approve(DAI, NFT_POSITION_MANAGER, 'DAI');

    console.log('\n=== Step 4: Create pools and add liquidity ===');
    for (const pool of POOLS) {
        const [token0, token1] = sortTokens(pool.a, pool.b);
        const swapped = token0.toLowerCase() !== pool.a.toLowerCase();
        const dec0 = swapped ? pool.dB : pool.dA;
        const dec1 = swapped ? pool.dA : pool.dB;
        const amt0 = swapped ? pool.amtB : pool.amtA;
        const amt1 = swapped ? pool.amtA : pool.amtB;

        const tickSpacing = pool.fee === FEE_LOW ? TICK_SPACING_500 : TICK_SPACING_3000;
        const { min: tickLower, max: tickUpper } = getMinMaxTick(tickSpacing);
        const sqrtPriceX96 = computeSqrtPriceX96(dec0, dec1);

        console.log(`\n--- ${pool.name} ---`);
        console.log(`  token0=${token0} (${dec0} dec)  token1=${token1} (${dec1} dec)`);
        console.log(`  sqrtPriceX96=${sqrtPriceX96}  ticks=[${tickLower},${tickUpper}]`);

        await createPool(token0, token1, pool.fee, sqrtPriceX96, pool.name);
        await mintPosition(token0, token1, pool.fee, tickLower, tickUpper, amt0, amt1, pool.name);
    }

    console.log('\n\n========================================');
    console.log('  V3 POOLS CREATED SUCCESSFULLY');
    console.log('========================================');
    POOLS.forEach(p => console.log(`  ${p.name}: fee=${p.fee / 10000}%`));
    console.log('========================================\n');
}

main().catch(e => { console.error('FATAL:', e.message); process.exit(1); });
