import { createHash } from 'crypto';
import { readFileSync } from 'fs';
import { secp256k1 } from '@noble/curves/secp256k1';
import { keccak_256 } from '@noble/hashes/sha3';

const RPC = process.env.RPC_URL || 'http://46.225.30.187/rpc';
const PRIVATE_KEY = process.env.DEPLOYER_KEY || '78861a117e94a1597857ae14a5afa0bd2da2f029981a8dfa1f241ee721a99ede';
const CHAIN_ID = 7919n;

function keccak256(data) {
    return Buffer.from(keccak_256(data));
}

function privateKeyToAddress(privKey) {
    const pubKey = secp256k1.getPublicKey(privKey, false).slice(1);
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
    const parts = [
        u64BE(tx.chainId),
        u64BE(tx.nonce),
        u256BE(tx.gasPrice),
        u64BE(tx.gasLimit),
        tx.to ? Buffer.from(tx.to.replace('0x', ''), 'hex') : Buffer.alloc(20),
        u256BE(tx.value),
        tx.data,
    ];
    return keccak256(Buffer.concat(parts));
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
    const parts = [
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
    ];
    return Buffer.concat(parts);
}

async function rpc(method, params = []) {
    const res = await fetch(RPC, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ jsonrpc: '2.0', id: Date.now(), method, params }),
    });
    const data = await res.json();
    if (data.error) throw new Error(`${method}: ${data.error.message}`);
    return data.result;
}

async function deploy(name, bytecodeHex, constructorArgs = '') {
    const fullBytecode = bytecodeHex + constructorArgs.replace('0x', '');
    const data = Buffer.from(fullBytecode.replace('0x', ''), 'hex');

    const from = privateKeyToAddress(PRIVATE_KEY);
    const nonceHex = await rpc('eth_getTransactionCount', [from, 'latest']);
    const nonce = parseInt(nonceHex, 16);

    const tx = {
        chainId: Number(CHAIN_ID),
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

    for (let i = 0; i < 20; i++) {
        await new Promise(r => setTimeout(r, 2000));
        const receipt = await rpc('eth_getTransactionReceipt', [txHash]);
        if (receipt) {
            const addr = receipt.contractAddress;
            const gas = parseInt(receipt.gasUsed || '0x0', 16);
            console.log(`  Deployed: ${addr} (gas: ${gas})`);
            return addr;
        }
    }
    console.log(`  WARNING: No receipt after 40s for ${txHash}`);
    return null;
}

async function main() {
    const from = privateKeyToAddress(PRIVATE_KEY);
    console.log(`Deployer: ${from}`);
    console.log(`Chain ID: ${CHAIN_ID}`);
    console.log(`RPC: ${RPC}\n`);

    const contracts = {};

    const mc3 = JSON.parse(readFileSync('out/Multicall3.sol/Multicall3.json', 'utf8'));
    contracts.multicall3 = await deploy('Multicall3', mc3.bytecode.object);

    const wprim = JSON.parse(readFileSync('out/WMRSN.sol/WMRSN.json', 'utf8'));
    contracts.wprim = await deploy('WMRSN', wprim.bytecode.object);

    const mock = JSON.parse(readFileSync('out/MockERC20.sol/MockERC20.json', 'utf8'));
    const abi = new TextEncoder();

    const encodeString = (s) => {
        const offset = Buffer.alloc(32); offset.writeBigUInt64BE(96n, 24);
        return offset;
    };

    function abiEncodeConstructor(name, symbol, decimals) {
        const nameBytes = Buffer.from(name, 'utf8');
        const symbolBytes = Buffer.from(symbol, 'utf8');
        const parts = [];
        parts.push(u256BE(96n));
        parts.push(u256BE(160n));
        parts.push(u256BE(BigInt(decimals)));
        parts.push(u256BE(BigInt(nameBytes.length)));
        const namePad = Buffer.alloc(Math.ceil(nameBytes.length / 32) * 32);
        nameBytes.copy(namePad);
        parts.push(namePad);
        parts.push(u256BE(BigInt(symbolBytes.length)));
        const symPad = Buffer.alloc(Math.ceil(symbolBytes.length / 32) * 32);
        symbolBytes.copy(symPad);
        parts.push(symPad);
        return Buffer.concat(parts).toString('hex');
    }

    contracts.usdc = await deploy('MockUSDC', mock.bytecode.object, abiEncodeConstructor('USD Coin', 'USDC', 6));
    contracts.usdt = await deploy('MockUSDT', mock.bytecode.object, abiEncodeConstructor('Tether USD', 'USDT', 6));
    contracts.dai = await deploy('MockDAI', mock.bytecode.object, abiEncodeConstructor('Dai Stablecoin', 'DAI', 18));

    const factory = JSON.parse(readFileSync('out/PrimeSwapFactory.sol/PrimeSwapFactory.json', 'utf8'));
    const factoryArgs = u256BE(BigInt(from)).toString('hex').slice(24).padStart(64, '0');
    contracts.factory = await deploy('PrimeSwapFactory', factory.bytecode.object, factoryArgs.padStart(64, '0'));

    if (contracts.factory && contracts.wprim) {
        const router = JSON.parse(readFileSync('out/PrimeSwapRouter.sol/PrimeSwapRouter.json', 'utf8'));
        const routerArgs =
            contracts.factory.replace('0x', '').padStart(64, '0') +
            contracts.wprim.replace('0x', '').padStart(64, '0');
        contracts.router = await deploy('PrimeSwapRouter', router.bytecode.object, routerArgs);
    }

    console.log('\n=== DEPLOYED CONTRACTS ===');
    for (const [name, addr] of Object.entries(contracts)) {
        console.log(`${name.padEnd(15)} ${addr || 'FAILED'}`);
    }

    const json = JSON.stringify(contracts, null, 2);
    const { writeFileSync } = await import('fs');
    writeFileSync('deployments.json', json);
    console.log('\nSaved to deployments.json');
}

main().catch(e => { console.error('FATAL:', e.message); process.exit(1); });
