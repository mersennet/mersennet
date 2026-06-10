// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';
import remarkMath from 'remark-math';
import rehypeKatex from 'rehype-katex';

export default defineConfig({
  site: 'https://docs.mersennet.com',
  markdown: {
    remarkPlugins: [remarkMath],
    rehypePlugins: [rehypeKatex],
  },
  integrations: [
    starlight({
      title: 'Mersennet',
      description:
        'Build private DeFi on a zero-knowledge EVM Layer 1 with shielded accounts, a native on-chain order book, and SP1-proven state.',
      logo: {
        src: './src/assets/logo.svg',
        alt: 'Mersennet',
      },
      favicon: '/favicon.svg',
      social: [
        { icon: 'github', label: 'GitHub', href: 'https://github.com/mersennet/mersennet' },
      ],
      editLink: {
        baseUrl: 'https://github.com/mersennet/docs/edit/main/',
      },
      customCss: ['./src/styles/custom.css', 'katex/dist/katex.min.css'],
      sidebar: [
        {
          label: 'Getting Started',
          items: [
            { slug: 'getting-started/overview' },
            { slug: 'getting-started/network-info' },
            { slug: 'getting-started/wallet-setup' },
            { slug: 'getting-started/faucet' },
            { slug: 'getting-started/first-transaction' },
          ],
        },
        {
          label: 'Developers',
          items: [
            {
              label: 'Quick Start',
              items: [
                { slug: 'developers/quick-start/hardhat' },
                { slug: 'developers/quick-start/foundry' },
              ],
            },
            {
              label: 'Smart Contracts',
              items: [
                { slug: 'developers/contracts/erc20-guide' },
                { slug: 'developers/contracts/nft-guide' },
                { slug: 'developers/contracts/defi-integration' },
              ],
            },
            {
              label: 'JSON-RPC API',
              items: [
                { slug: 'developers/rpc/overview' },
                { slug: 'developers/rpc/methods' },
              ],
            },
            {
              label: 'SDKs',
              items: [
                { slug: 'developers/sdks/javascript' },
                { slug: 'developers/sdks/python' },
              ],
            },
            {
              label: 'Privacy & ZK',
              items: [
                { slug: 'developers/privacy/shielded-sdk' },
                { slug: 'developers/privacy/shielded-rpc' },
              ],
            },
          ],
        },
        {
          label: 'Privacy',
          items: [
            { slug: 'privacy/overview' },
            { slug: 'privacy/shielded-accounts' },
            { slug: 'privacy/zk-risk-checks' },
            { slug: 'privacy/selective-disclosure' },
            { slug: 'privacy/note-scanning' },
            { slug: 'privacy/state-proofs' },
            { slug: 'privacy/migration' },
          ],
        },
        {
          label: 'Validators',
          items: [
            { slug: 'validators/overview' },
            { slug: 'validators/run-a-node' },
            { slug: 'validators/staking' },
            { slug: 'validators/monitoring' },
          ],
        },
        {
          label: 'Architecture',
          items: [
            { slug: 'architecture/consensus' },
            { slug: 'architecture/node-architecture' },
            { slug: 'architecture/evm-compatibility' },
            { slug: 'architecture/prime-orders' },
            { slug: 'architecture/tokenomics' },
          ],
        },
        {
          label: 'Ecosystem',
          items: [
            { slug: 'ecosystem/primeswap' },
            { slug: 'ecosystem/primeswap-v3' },
            { slug: 'ecosystem/primetrade' },
            { slug: 'ecosystem/primefi' },
            { slug: 'ecosystem/primeport' },
            { slug: 'ecosystem/wallet' },
            { slug: 'ecosystem/directory' },
          ],
        },
        {
          label: 'Resources',
          items: [
            { slug: 'whitepaper' },
            { slug: 'resources/faq' },
            { slug: 'resources/contracts' },
            { slug: 'resources/brand-assets' },
            { slug: 'resources/changelog' },
          ],
        },
      ],
    }),
  ],
});
