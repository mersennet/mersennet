import type {SidebarsConfig} from '@docusaurus/plugin-content-docs';

const sidebars: SidebarsConfig = {
  gettingStarted: [
    {
      type: 'category',
      label: 'Getting Started',
      collapsed: false,
      items: [
        'getting-started/overview',
        'getting-started/network-info',
        'getting-started/wallet-setup',
        'getting-started/faucet',
        'getting-started/first-transaction',
      ],
    },
  ],
  developers: [
    {
      type: 'category',
      label: 'Quick Start',
      collapsed: false,
      items: [
        'developers/quick-start/hardhat',
        'developers/quick-start/foundry',
      ],
    },
    {
      type: 'category',
      label: 'Smart Contracts',
      items: [
        'developers/contracts/erc20-guide',
        'developers/contracts/nft-guide',
        'developers/contracts/defi-integration',
      ],
    },
    {
      type: 'category',
      label: 'JSON-RPC API',
      items: [
        'developers/rpc/overview',
        'developers/rpc/methods',
      ],
    },
    {
      type: 'category',
      label: 'SDKs',
      items: [
        'developers/sdks/javascript',
        'developers/sdks/python',
      ],
    },
    {
      type: 'category',
      label: 'Privacy & ZK',
      items: [
        'developers/privacy/shielded-sdk',
        'developers/privacy/shielded-rpc',
      ],
    },
  ],
  privacy: [
    {
      type: 'category',
      label: 'Privacy',
      collapsed: false,
      items: [
        'privacy/overview',
        'privacy/shielded-accounts',
        'privacy/zk-risk-checks',
        'privacy/selective-disclosure',
        'privacy/note-scanning',
        'privacy/state-proofs',
        'privacy/migration',
      ],
    },
  ],
  validators: [
    {
      type: 'category',
      label: 'Validators',
      collapsed: false,
      items: [
        'validators/overview',
        'validators/run-a-node',
        'validators/staking',
        'validators/monitoring',
      ],
    },
  ],
  architecture: [
    {
      type: 'category',
      label: 'Architecture',
      collapsed: false,
      items: [
        'architecture/consensus',
        'architecture/node-architecture',
        'architecture/evm-compatibility',
        'architecture/prime-orders',
        'architecture/tokenomics',
      ],
    },
  ],
  ecosystem: [
    {
      type: 'category',
      label: 'Ecosystem',
      collapsed: false,
      items: [
        'ecosystem/primeswap',
        'ecosystem/primeswap-v3',
        'ecosystem/primetrade',
        'ecosystem/primefi',
        'ecosystem/primeport',
        'ecosystem/wallet',
        'ecosystem/directory',
      ],
    },
    {
      type: 'category',
      label: 'Resources',
      items: [
        'resources/contracts',
        'resources/brand-assets',
        'resources/faq',
        'resources/changelog',
      ],
    },
    {
      type: 'doc',
      id: 'whitepaper',
      label: 'Whitepaper',
    },
  ],
};

export default sidebars;
