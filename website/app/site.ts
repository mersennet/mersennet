export const SITE = {
  name: 'Mersennet',
  domain: 'mersennet.com',
  url: 'https://mersennet.com',
  tagline: 'The private, verifiable network',
  description:
    'Mersennet is a zero-knowledge Layer 1: account-level privacy across the EVM and a native on-chain order book, leverage secured by ZK risk checks instead of open liquidations, and state proven end to end with SP1.',
  chainId: 7919,
  symbol: 'PRIM',
  blockTime: '~1s',
  consensus: 'BFT PoS',
  rpcUrl: '46.225.30.187:8545',
} as const;

export const LINKS = {
  docs: 'https://docs.mersennet.com',
  explorer: 'http://46.225.30.187',
  faucet: 'http://46.225.30.187:4003',
  github: 'https://github.com/mersennet/mersennet',
  whitepaper: 'https://docs.mersennet.com/whitepaper',
  quickStart: 'https://docs.mersennet.com/developers/quick-start/hardhat',
  networkInfo: 'https://docs.mersennet.com/getting-started/network-info',
  rpc: 'https://docs.mersennet.com/developers/rpc/methods',
  sdk: 'https://docs.mersennet.com/developers/sdks/javascript',
} as const;

export const MERSENNE_PRIMES = ['3', '7', '31', '127', '8191', '131071', '524287'] as const;
