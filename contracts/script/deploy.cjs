const { secp256k1 } = require('@noble/curves/secp256k1');
const { keccak_256 } = require('@noble/hashes/sha3');
const { readFileSync, writeFileSync } = require('fs');

const RPC = process.env.RPC_URL || 'http://46.225.30.187/rpc';
if (!process.env.DEPLOYER_KEY) {
    console.error('ERROR: DEPLOYER_KEY environment variable is required');
    console.error('Usage: DEPLOYER_KEY=0x... node script/deploy.cjs');
    process.exit(1);
}
const PRIVATE_KEY = process.env.DEPLOYER_KEY.replace('0x', '');
const CHAIN_ID = 7919;

function keccak256(data) {
    return Buffer.from(keccak_256(data));
}

function privateKeyToAddress(privKeyHex) {
    const pubKey = secp256k1.getPublicKey(privKeyHex, false).slice(1);
    return '0x' + keccak256(pubKey).slice(-20).toString('hex');
}

function u64BE(n) {
    const buf = Buffer.alloc(8);
    buf.writeBigUInt64BE(BigInt(n));
    return buf;
}

function u256BE(n) {
    const hex = BigInt(n).toString(16).padStart(64, '0');
    return Buffer.from(hex, 'hex');
}

function u32BE(n) {
    const buf = Buffer.alloc(4);
    buf.writeUInt32BE(Number(n));
    return buf;
}

function signingHash(tx) {
    return keccak256(Buffer.concat([
        u64BE(tx.chainId),
        u64BE(tx.nonce),
        u256BE(tx.gasPrice),
        u64BE(tx.gasLimit),
        tx.to ? Buffer.from(tx.to.replace('0x', ''), 'hex') : Buffer.alloc(20),
        u256BE(tx.value),
        tx.data,
    ]));
}

function signTx(tx, privKeyHex) {
    const hash = signingHash(tx);
    const sig = secp256k1.sign(hash, privKeyHex);
    const r = sig.r;
    const s = sig.s;
    const v = BigInt(sig.recovery) + 35n + BigInt(tx.chainId) * 2n;
    return { r, s, v };
}

function encodeRawTx(tx, sig) {
    return Buffer.concat([
        u64BE(tx.chainId),
        u64BE(tx.nonce),
        u256BE(tx.gasPrice),
        u64BE(tx.gasLimit),
        tx.to ? Buffer.from(tx.to.replace('0x', ''), 'hex') : Buffer.alloc(20),
        u256BE(tx.value),
        u32BE(tx.data.length),
        tx.data,
        u256BE(sig.r),
        u256BE(sig.s),
        u64BE(sig.v),
    ]);
}

async function rpc(method, params) {
    const res = await fetch(RPC, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ jsonrpc: '2.0', id: Date.now(), method, params: params || [] }),
    });
    const data = await res.json();
    if (data.error) throw new Error(`${method}: ${data.error.message}`);
    return data.result;
}

async function deploy(name, bytecodeHex, constructorArgsHex) {
    const fullHex = bytecodeHex.replace('0x', '') + (constructorArgsHex || '').replace('0x', '');
    const data = Buffer.from(fullHex, 'hex');

    const from = privateKeyToAddress(PRIVATE_KEY);
    const nonceHex = await rpc('eth_getTransactionCount', [from, 'latest']);
    const nonce = parseInt(nonceHex, 16);

    const tx = {
        chainId: CHAIN_ID,
        nonce,
        gasPrice: 1000000000n,
        gasLimit: 5000000,
        to: null,
        value: 0n,
        data,
    };

    const sig = signTx(tx, PRIVATE_KEY);
    const raw = encodeRawTx(tx, sig);
    const rawHex = '0x' + raw.toString('hex');

    console.log(`  Deploying ${name}... (nonce=${nonce}, data=${data.length} bytes)`);
    const txHash = await rpc('eth_sendRawTransaction', [rawHex]);
    console.log(`  TX: ${txHash}`);

    for (let i = 0; i < 30; i++) {
        await new Promise(r => setTimeout(r, 2000));
        const receipt = await rpc('eth_getTransactionReceipt', [txHash]);
        if (receipt) {
            const addr = receipt.contract_address || receipt.contractAddress;
            const gas = parseInt(receipt.gas_used || receipt.gasUsed || '0x0', 16);
            console.log(`  ✓ Deployed: ${addr} (gas: ${gas})\n`);
            return addr;
        }
    }
    console.log(`  WARNING: No receipt after 60s for ${txHash}\n`);
    return null;
}

function abiEncodeConstructor(nameStr, symbolStr, decimals) {
    const nameB = Buffer.from(nameStr, 'utf8');
    const symbolB = Buffer.from(symbolStr, 'utf8');
    const parts = [];
    // offset to name string, offset to symbol string, decimals
    parts.push(u256BE(96));
    parts.push(u256BE(160));
    parts.push(u256BE(decimals));
    // name string: length + padded data
    parts.push(u256BE(nameB.length));
    const namePad = Buffer.alloc(Math.ceil(nameB.length / 32) * 32);
    nameB.copy(namePad);
    parts.push(namePad);
    // symbol string: length + padded data
    parts.push(u256BE(symbolB.length));
    const symPad = Buffer.alloc(Math.ceil(symbolB.length / 32) * 32);
    symbolB.copy(symPad);
    parts.push(symPad);
    return Buffer.concat(parts).toString('hex');
}

async function main() {
    const from = privateKeyToAddress(PRIVATE_KEY);
    console.log(`Deployer: ${from}`);
    console.log(`Chain ID: ${CHAIN_ID}`);
    console.log(`RPC: ${RPC}\n`);

    const contracts = {};

    const mc3 = JSON.parse(readFileSync('out/Multicall3.sol/Multicall3.json', 'utf8'));
    contracts.multicall3 = await deploy('Multicall3', mc3.bytecode.object);

    const wprim = JSON.parse(readFileSync('out/WPRIM.sol/WPRIM.json', 'utf8'));
    contracts.wprim = await deploy('WPRIM', wprim.bytecode.object);

    const mock = JSON.parse(readFileSync('out/MockERC20.sol/MockERC20.json', 'utf8'));
    contracts.usdc = await deploy('MockUSDC', mock.bytecode.object, abiEncodeConstructor('USD Coin', 'USDC', 6));
    contracts.usdt = await deploy('MockUSDT', mock.bytecode.object, abiEncodeConstructor('Tether USD', 'USDT', 6));
    contracts.dai = await deploy('MockDAI', mock.bytecode.object, abiEncodeConstructor('Dai Stablecoin', 'DAI', 18));

    const factoryJson = JSON.parse(readFileSync('out/PrimeSwapFactory.sol/PrimeSwapFactory.json', 'utf8'));
    const factoryArgs = from.replace('0x', '').padStart(64, '0');
    contracts.factory = await deploy('PrimeSwapFactory', factoryJson.bytecode.object, factoryArgs);

    if (contracts.factory && contracts.wprim) {
        const routerJson = JSON.parse(readFileSync('out/PrimeSwapRouter.sol/PrimeSwapRouter.json', 'utf8'));
        const routerArgs =
            contracts.factory.replace('0x', '').padStart(64, '0') +
            contracts.wprim.replace('0x', '').padStart(64, '0');
        contracts.router = await deploy('PrimeSwapRouter', routerJson.bytecode.object, routerArgs);
    }

    console.log('\n=== DEPLOYED CONTRACTS ===');
    for (const [name, addr] of Object.entries(contracts)) {
        console.log(`${name.padEnd(15)} ${addr || 'FAILED'}`);
    }

    writeFileSync('deployments.json', JSON.stringify(contracts, null, 2));
    console.log('\nSaved to deployments.json');
}

main().catch(e => { console.error('FATAL:', e.message); process.exit(1); });
