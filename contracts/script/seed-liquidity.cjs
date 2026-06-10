const { secp256k1 } = require('@noble/curves/secp256k1');
const { keccak_256 } = require('@noble/hashes/sha3');
const { readFileSync } = require('fs');

const RPC = process.env.RPC_URL || 'http://46.225.30.187/rpc';
if (!process.env.DEPLOYER_KEY) {
    console.error('ERROR: DEPLOYER_KEY environment variable is required');
    console.error('Usage: DEPLOYER_KEY=0x... node script/seed-liquidity.cjs');
    process.exit(1);
}
const PRIVATE_KEY = process.env.DEPLOYER_KEY.replace('0x', '');
const CHAIN_ID = 7919;

const deployments = JSON.parse(readFileSync(__dirname + '/../deployments.json', 'utf8'));
const ROUTER = deployments.contracts.PrimeSwapRouter;
const WMRSN = deployments.contracts.WMRSN;
const USDC = deployments.contracts.MockUSDC;
const USDT = deployments.contracts.MockUSDT;
const DAI = deployments.contracts.MockDAI;

function keccak256(data) { return Buffer.from(keccak_256(data)); }
function privateKeyToAddress(pk) { return '0x' + keccak256(secp256k1.getPublicKey(pk, false).slice(1)).slice(-20).toString('hex'); }
function u64BE(n) { const b = Buffer.alloc(8); b.writeBigUInt64BE(BigInt(n)); return b; }
function u256BE(n) { return Buffer.from(BigInt(n).toString(16).padStart(64, '0'), 'hex'); }
function u32BE(n) { const b = Buffer.alloc(4); b.writeUInt32BE(Number(n)); return b; }

function signingHash(tx) {
    return keccak256(Buffer.concat([
        u64BE(tx.chainId), u64BE(tx.nonce), u256BE(tx.gasPrice), u64BE(tx.gasLimit),
        tx.to ? Buffer.from(tx.to.replace('0x', ''), 'hex') : Buffer.alloc(20),
        u256BE(tx.value), tx.data,
    ]));
}

function signTx(tx, pk) {
    const hash = signingHash(tx);
    const sig = secp256k1.sign(hash, pk);
    return { r: sig.r, s: sig.s, v: BigInt(sig.recovery) + 35n + BigInt(tx.chainId) * 2n };
}

function encodeRawTx(tx, sig) {
    return Buffer.concat([
        u64BE(tx.chainId), u64BE(tx.nonce), u256BE(tx.gasPrice), u64BE(tx.gasLimit),
        tx.to ? Buffer.from(tx.to.replace('0x', ''), 'hex') : Buffer.alloc(20),
        u256BE(tx.value), u32BE(tx.data.length), tx.data,
        u256BE(sig.r), u256BE(sig.s), u64BE(sig.v),
    ]);
}

async function rpc(method, params) {
    const res = await fetch(RPC, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ jsonrpc: '2.0', id: Date.now(), method, params: params || [] }) });
    const data = await res.json();
    if (data.error) throw new Error(`${method}: ${data.error.message}`);
    return data.result;
}

let nonce = null;
async function sendTx(to, data, value = 0n) {
    const from = privateKeyToAddress(PRIVATE_KEY);
    if (nonce === null) nonce = parseInt(await rpc('eth_getTransactionCount', [from, 'latest']), 16);
    const tx = { chainId: CHAIN_ID, nonce, gasPrice: 1000000000n, gasLimit: 5000000, to, value, data: Buffer.from(data.replace('0x', ''), 'hex') };
    const sig = signTx(tx, PRIVATE_KEY);
    const raw = '0x' + encodeRawTx(tx, sig).toString('hex');
    const txHash = await rpc('eth_sendRawTransaction', [raw]);
    nonce++;

    for (let i = 0; i < 20; i++) {
        await new Promise(r => setTimeout(r, 3000));
        const receipt = await rpc('eth_getTransactionReceipt', [txHash]);
        if (receipt) {
            const status = receipt.status;
            const gas = parseInt(receipt.gas_used || receipt.gasUsed || '0', 16);
            return { txHash, status, gas };
        }
    }
    return { txHash, status: 'timeout', gas: 0 };
}

function fnSelector(sig) {
    return keccak256(Buffer.from(sig)).slice(0, 4).toString('hex');
}

function encodeAddress(addr) { return addr.replace('0x', '').padStart(64, '0'); }
function encodeUint256(n) { return BigInt(n).toString(16).padStart(64, '0'); }

const E6 = 10n ** 6n;
const E18 = 10n ** 18n;

async function mintTokens(token, amount, label) {
    const data = '0x' + fnSelector('mint(address,uint256)') + encodeAddress(privateKeyToAddress(PRIVATE_KEY)) + encodeUint256(amount);
    console.log(`  Minting ${label}...`);
    const r = await sendTx(token, data);
    console.log(`    TX: ${r.txHash} status=${r.status}`);
}

async function approve(token, spender, amount, label) {
    const data = '0x' + fnSelector('approve(address,uint256)') + encodeAddress(spender) + encodeUint256(amount);
    console.log(`  Approving ${label} for router...`);
    const r = await sendTx(token, data);
    console.log(`    TX: ${r.txHash} status=${r.status}`);
}

async function depositWPRIM(amount) {
    const data = '0x' + fnSelector('deposit()');
    console.log(`  Depositing ${Number(amount / E18)} MRSN -> WMRSN...`);
    const r = await sendTx(WMRSN, data, amount);
    console.log(`    TX: ${r.txHash} status=${r.status}`);
}

async function addLiquidity(tokenA, tokenB, amountA, amountB, label) {
    const deadline = BigInt(Math.floor(Date.now() / 1000) + 3600);
    const from = privateKeyToAddress(PRIVATE_KEY);
    const data = '0x' + fnSelector('addLiquidity(address,address,uint256,uint256,uint256,uint256,address,uint256)')
        + encodeAddress(tokenA) + encodeAddress(tokenB)
        + encodeUint256(amountA) + encodeUint256(amountB)
        + encodeUint256(0) + encodeUint256(0)
        + encodeAddress(from) + encodeUint256(deadline);
    console.log(`  Adding liquidity: ${label}...`);
    const r = await sendTx(ROUTER, data);
    console.log(`    TX: ${r.txHash} status=${r.status}`);
}

async function main() {
    const from = privateKeyToAddress(PRIVATE_KEY);
    console.log(`Deployer: ${from}`);
    console.log(`Router: ${ROUTER}\n`);

    const wprimAmount = 30000n * E18;
    const usdcAmount = 10000n * E6;
    const usdtAmount = 10000n * E6;
    const daiAmount = 10000n * E18;

    console.log('--- Step 1: Mint test tokens ---');
    await mintTokens(USDC, usdcAmount, `${Number(usdcAmount / E6)} USDC`);
    await mintTokens(USDT, usdtAmount, `${Number(usdtAmount / E6)} USDT`);
    await mintTokens(DAI, daiAmount, `${Number(daiAmount / E18)} DAI`);

    console.log('\n--- Step 2: Wrap MRSN -> WMRSN ---');
    await depositWPRIM(wprimAmount);

    console.log('\n--- Step 3: Approve router ---');
    const MAX = (2n ** 256n) - 1n;
    await approve(WMRSN, ROUTER, MAX, 'WMRSN');
    await approve(USDC, ROUTER, MAX, 'USDC');
    await approve(USDT, ROUTER, MAX, 'USDT');
    await approve(DAI, ROUTER, MAX, 'DAI');

    console.log('\n--- Step 4: Add liquidity ---');
    await addLiquidity(WMRSN, USDC, 10000n * E18, usdcAmount, 'WMRSN/USDC (10000 WMRSN + 10000 USDC)');
    await addLiquidity(WMRSN, USDT, 10000n * E18, usdtAmount, 'WMRSN/USDT (10000 WMRSN + 10000 USDT)');
    await addLiquidity(WMRSN, DAI, 10000n * E18, daiAmount, 'WMRSN/DAI (10000 WMRSN + 10000 DAI)');

    console.log('\n=== LIQUIDITY POOLS SEEDED SUCCESSFULLY ===');
    console.log('WMRSN/USDC: 10,000 WMRSN + 10,000 USDC (implied price: 1 MRSN = 1 USDC)');
    console.log('WMRSN/USDT: 10,000 WMRSN + 10,000 USDT (implied price: 1 MRSN = 1 USDT)');
    console.log('WMRSN/DAI:  10,000 WMRSN + 10,000 DAI  (implied price: 1 MRSN = 1 DAI)');
}

main().catch(e => { console.error('FATAL:', e.message); process.exit(1); });
