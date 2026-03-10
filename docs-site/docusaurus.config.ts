import {themes as prismThemes} from 'prism-react-renderer';
import type {Config} from '@docusaurus/types';
import type * as Preset from '@docusaurus/preset-classic';

const config: Config = {
  title: 'Prime Chain',
  tagline: 'High-performance EVM blockchain with native order matching',
  favicon: 'img/favicon.svg',

  future: {
    v4: true,
  },

  markdown: {
    format: 'detect',
  },

  url: 'https://docs.primechain.network',
  baseUrl: '/',

  organizationName: 'PrimeNumbersLabs',
  projectName: 'prime-chain',

  onBrokenLinks: 'warn',

  i18n: {
    defaultLocale: 'en',
    locales: ['en'],
  },

  themes: [
    [
      '@easyops-cn/docusaurus-search-local',
      {
        hashed: true,
        indexBlog: false,
        docsRouteBasePath: '/',
        highlightSearchTermsOnTargetPage: true,
        searchBarShortcutHint: true,
      },
    ],
  ],

  presets: [
    [
      'classic',
      {
        docs: {
          sidebarPath: './sidebars.ts',
          routeBasePath: '/',
          editUrl: 'https://github.com/PrimeNumbersLabs/prime-chain/tree/main/docs-site/',
          showLastUpdateTime: true,
        },
        blog: false,
        theme: {
          customCss: './src/css/custom.css',
        },
      } satisfies Preset.Options,
    ],
  ],

  themeConfig: {
    image: 'img/prime-chain-social.png',
    metadata: [
      { name: 'keywords', content: 'blockchain, EVM, Layer 1, DeFi, CLOB, PrimeOrders, PRIM' },
      { name: 'twitter:card', content: 'summary_large_image' },
    ],
    colorMode: {
      defaultMode: 'light',
      disableSwitch: false,
      respectPrefersColorScheme: true,
    },
    announcementBar: {
      id: 'testnet_live',
      content: '🟢 <b>Prime Chain Testnet is live</b> — <a href="/getting-started/faucet">Get testnet PRIM</a> · <a href="http://46.225.30.187" target="_blank">Block Explorer</a> · Chain ID: 7919',
      backgroundColor: '#F3EDFF',
      textColor: '#3A334D',
      isCloseable: true,
    },
    navbar: {
      title: 'Prime Chain',
      logo: {
        alt: 'Prime Chain',
        src: 'img/logo.svg',
        width: 32,
        height: 22,
      },
      items: [
        {
          type: 'docSidebar',
          sidebarId: 'gettingStarted',
          position: 'left',
          label: 'Get Started',
        },
        {
          type: 'docSidebar',
          sidebarId: 'developers',
          position: 'left',
          label: 'Developers',
        },
        {
          type: 'docSidebar',
          sidebarId: 'validators',
          position: 'left',
          label: 'Validators',
        },
        {
          type: 'docSidebar',
          sidebarId: 'architecture',
          position: 'left',
          label: 'Architecture',
        },
        {
          type: 'docSidebar',
          sidebarId: 'ecosystem',
          position: 'left',
          label: 'Ecosystem',
        },
        {
          href: 'http://46.225.30.187',
          label: 'Explorer',
          position: 'right',
        },
        {
          href: 'http://46.225.30.187:4003',
          label: 'Faucet',
          position: 'right',
        },
        {
          href: 'https://github.com/PrimeNumbersLabs/prime-chain',
          label: 'GitHub',
          position: 'right',
        },
      ],
    },
    footer: {
      style: 'dark',
      links: [
        {
          title: 'Build',
          items: [
            { label: 'Quick Start', to: '/developers/quick-start/hardhat' },
            { label: 'ERC-20 Guide', to: '/developers/contracts/erc20-guide' },
            { label: 'RPC Reference', to: '/developers/rpc/methods' },
            { label: 'JavaScript SDK', to: '/developers/sdks/javascript' },
          ],
        },
        {
          title: 'Network',
          items: [
            { label: 'Block Explorer', href: 'http://46.225.30.187' },
            { label: 'Faucet', href: 'http://46.225.30.187:4003' },
            { label: 'Grafana', href: 'http://46.225.30.187:3000' },
            { label: 'Network Info', to: '/getting-started/network-info' },
          ],
        },
        {
          title: 'Ecosystem',
          items: [
            { label: 'PrimeSwap V2', to: '/ecosystem/primeswap' },
            { label: 'PrimeSwap V3', to: '/ecosystem/primeswap-v3' },
            { label: 'PrimeTrade', to: '/ecosystem/primetrade' },
            { label: 'PrimeFi Lending', to: '/ecosystem/primefi' },
            { label: 'Ecosystem Directory', to: '/ecosystem/directory' },
          ],
        },
        {
          title: 'Community',
          items: [
            { label: 'GitHub', href: 'https://github.com/PrimeNumbersLabs/prime-chain' },
            { label: 'Whitepaper', to: '/whitepaper' },
            { label: 'FAQ', to: '/resources/faq' },
          ],
        },
      ],
      copyright: `© ${new Date().getFullYear()} Prime Numbers Labs. All rights reserved.`,
    },
    prism: {
      theme: prismThemes.github,
      darkTheme: prismThemes.dracula,
      additionalLanguages: ['solidity', 'toml', 'rust', 'bash', 'json'],
    },
  } satisfies Preset.ThemeConfig,
};

export default config;
