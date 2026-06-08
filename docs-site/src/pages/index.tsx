import React, {type ReactNode} from 'react';
import Layout from '@theme/Layout';
import GlyphSphere from '@site/src/components/GlyphSphere';
import styles from './index.module.css';

function Icon({path}: {path: ReactNode}): ReactNode {
  return (
    <svg
      width="22"
      height="22"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.7"
      strokeLinecap="round"
      strokeLinejoin="round">
      {path}
    </svg>
  );
}

const MARQUEE = [
  'Zero-knowledge',
  'Shielded accounts',
  'Native order book',
  'ZK risk checks',
  'SP1 proven state',
  'Selective disclosure',
  '2³¹−1 = 2147483647',
  'Groth16 bridge',
];

const STATS = [
  {num: '7919', label: 'Chain ID · prime'},
  {num: '~1s', label: 'Block time'},
  {num: 'EVM', label: 'Shanghai-equivalent'},
  {num: 'SP1', label: 'Proven state'},
];

const PILLARS = [
  {
    title: 'Account-level privacy',
    text: 'Shielded accounts conceal balances, positions, and order flow across the EVM and the native order book — full account privacy, not just mixed transfers.',
    icon: (
      <Icon
        path={
          <>
            <rect x="3" y="11" width="18" height="11" rx="2" />
            <path d="M7 11V7a5 5 0 0 1 10 0v4" />
          </>
        }
      />
    ),
  },
  {
    title: 'Risk checks in zero knowledge',
    text: 'Leverage without open liquidations. Solvency and margin are proven with ZK proofs instead of public liquidation auctions — positions stay private, the chain stays safe.',
    icon: (
      <Icon
        path={
          <>
            <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" />
            <path d="m9 12 2 2 4-4" />
          </>
        }
      />
    ),
  },
  {
    title: 'Native on-chain order book',
    text: 'A central limit order book exposed to Solidity through a precompile (0x0100). Atomic, composable matching beyond AMMs — now with shielded order placement.',
    icon: (
      <Icon
        path={
          <>
            <line x1="3" y1="6" x2="21" y2="6" />
            <line x1="3" y1="12" x2="15" y2="12" />
            <line x1="3" y1="18" x2="18" y2="18" />
          </>
        }
      />
    ),
  },
  {
    title: 'Verifiable state',
    text: 'State transitions are proven with SP1 and verified on-chain via a Groth16 bridge. Anyone can verify the chain from a succinct proof — no trusted full node required.',
    icon: (
      <Icon
        path={
          <>
            <polyline points="16 18 22 12 16 6" />
            <polyline points="8 6 2 12 8 18" />
          </>
        }
      />
    ),
  },
];

const ECOSYSTEM = [
  {name: 'PrimeSwap V2', status: 'Live', desc: 'AMM DEX with constant-product pools.'},
  {name: 'PrimeSwap V3', status: 'Live', desc: 'Concentrated liquidity, Uniswap-V3 style.'},
  {name: 'PrimeTrade', status: 'Live', desc: 'Order-book trading terminal on the native CLOB.'},
  {name: 'PrimeOrders', status: 'Live', desc: 'Native order book via precompile 0x0100.'},
  {name: 'Shielded Pool', status: 'Live', desc: 'Deposit, transfer, and trade privately.'},
  {name: 'PrimeFi', status: 'Soon', desc: 'Private lending & borrowing markets.'},
];

export default function Home(): ReactNode {
  return (
    <Layout
      title="Mersennet — The private, verifiable network"
      description="Mersennet is a zero-knowledge Layer 1: account-level privacy across the EVM and a native on-chain order book, with leverage secured by ZK risk checks and state proven end to end with SP1.">
      <main className={styles.page}>
        {/* Hero */}
        <header className={styles.hero}>
          <div className={styles.heroCanvas}>
            <GlyphSphere />
          </div>
          <div className={styles.heroGrid}>
            <div className={styles.heroLeft}>
              <span className={styles.kicker}>Nº 7919 · Zero-knowledge Layer 1</span>
              <h1 className={styles.title}>
                Private,
                <br />
                verifiable
                <br />
                <span className={styles.titleDim}>money.</span>
                <span className={styles.cursor} aria-hidden="true" />
              </h1>
              <p className={styles.lede}>
                The Mersennet developer hub — guides, RPC and SDK references, the
                whitepaper, and everything you need to build private DeFi.
              </p>
              <div className={styles.heroCtas}>
                <a className={`${styles.btn} ${styles.btnPrimary}`} href="/overview">
                  Read the docs
                  <Icon path={<path d="M5 12h14M12 5l7 7-7 7" />} />
                </a>
                <a className={`${styles.btn} ${styles.btnGhost}`} href="/whitepaper">
                  Whitepaper
                </a>
              </div>
              <div className={styles.heroSpecs}>
                <div>
                  <span className={styles.heroSpecVal}>~1s</span>
                  <span className={styles.heroSpecLabel}>Block time</span>
                </div>
                <div>
                  <span className={styles.heroSpecVal}>EVM</span>
                  <span className={styles.heroSpecLabel}>Equivalent</span>
                </div>
                <div>
                  <span className={styles.heroSpecVal}>ZK</span>
                  <span className={styles.heroSpecLabel}>Proven state</span>
                </div>
              </div>
            </div>

            <div className={styles.heroRight}>
              <div className={styles.hud}>
                <div className={styles.panelHead}>
                  <span>// Network</span>
                  <span>ONLINE</span>
                </div>
                <div className={styles.panelRow}>
                  <span>Chain ID</span>
                  <b>7919</b>
                </div>
                <div className={styles.panelRow}>
                  <span>Native token</span>
                  <b>PRIM</b>
                </div>
                <div className={styles.panelRow}>
                  <span>Consensus</span>
                  <b>BFT PoS · ~1s</b>
                </div>
                <div className={styles.panelRow}>
                  <span>Proof system</span>
                  <b>SP1 → Groth16</b>
                </div>
                <div className={styles.panelRow}>
                  <span>RPC</span>
                  <b>46.225.30.187:8545</b>
                </div>
              </div>

              <div className={styles.hud}>
                <div className={styles.panelHead}>
                  <span>// Get started</span>
                  <span>↳</span>
                </div>
                <p className={styles.panelText}>
                  Add Mersennet to your wallet, claim testnet PRIM, and deploy your first
                  contract.
                </p>
                <a className={`${styles.btn} ${styles.btnSm}`} href="/getting-started/network-info">
                  Network info →
                </a>
              </div>
            </div>
          </div>
        </header>

        {/* Marquee */}
        <div className={styles.marquee} aria-hidden="true">
          <div className={styles.marqueeTrack}>
            <span>
              {MARQUEE.map((t, i) => (
                <span key={`a${i}`}>
                  <span className={styles.marqueeStar}>✦</span> {t}
                </span>
              ))}
            </span>
            <span>
              {MARQUEE.map((t, i) => (
                <span key={`b${i}`}>
                  <span className={styles.marqueeStar}>✦</span> {t}
                </span>
              ))}
            </span>
          </div>
        </div>

        {/* Stat band */}
        <div className={styles.shell} style={{paddingTop: '3.5rem'}}>
          <div className={styles.stats}>
            {STATS.map((s) => (
              <div key={s.label} className={styles.stat}>
                <div className={styles.statNum}>{s.num}</div>
                <div className={styles.statLabel}>{s.label}</div>
              </div>
            ))}
          </div>
        </div>

        {/* Pillars */}
        <section className={styles.shell}>
          <div className={styles.section}>
            <span className={styles.secIndex}>[ 01 ] // The network</span>
            <h2 className={styles.secTitle}>
              A chain where privacy and verifiability are the defaults.
            </h2>
            <p className={styles.secLead}>
              Most chains make you choose between transparency and confidentiality.
              Mersennet uses zero-knowledge proofs to deliver both.
            </p>
            <div className={styles.pillars}>
              {PILLARS.map((p, i) => (
                <article key={p.title} className={styles.pillar}>
                  <div className={styles.pillarTop}>
                    <span className={styles.pillarNum}>P.{String(i + 1).padStart(2, '0')}</span>
                    <span className={styles.pillarIcon}>{p.icon}</span>
                  </div>
                  <h3 className={styles.pillarTitle}>{p.title}</h3>
                  <p className={styles.pillarText}>{p.text}</p>
                </article>
              ))}
            </div>
          </div>
        </section>

        {/* Selective disclosure */}
        <section className={styles.shell}>
          <div className={styles.section}>
            <div className={styles.split}>
              <div>
                <span className={styles.secIndex}>[ 02 ] // Selective disclosure</span>
                <h2 className={styles.secTitle}>Private by default. Auditable on your terms.</h2>
                <p className={styles.secLead}>
                  Grant a viewing key to an auditor, exchange, or counterparty and reveal
                  exactly what you choose — without exposing the rest of your account.
                </p>
                <ul className={styles.bullets}>
                  <li>
                    <span className={styles.check}>[✓]</span>
                    <span>
                      <strong>Grant-gated reads.</strong> Reconstruct balances, positions,
                      and orders only for holders of a valid disclosure grant.
                    </span>
                  </li>
                  <li>
                    <span className={styles.check}>[✓]</span>
                    <span>
                      <strong>Client-side proving.</strong> Generate proofs in the browser
                      with the WASM Noir prover — keys never leave the wallet.
                    </span>
                  </li>
                  <li>
                    <span className={styles.check}>[✓]</span>
                    <span>
                      <strong>Note scanning.</strong> Wallets rebuild private state by
                      scanning notes and tracking nullifiers — no central indexer.
                    </span>
                  </li>
                </ul>
              </div>

              <div className={`${styles.hud} ${styles.codeCard}`} style={{padding: 0}}>
                <div className={styles.codeBar}>
                  <span className={`${styles.dot} ${styles.dotR}`} />
                  <span className={`${styles.dot} ${styles.dotY}`} />
                  <span className={`${styles.dot} ${styles.dotG}`} />
                  <span className={styles.codeFile}>disclosure.ts</span>
                </div>
                <pre className={styles.code}>
                  <code>
                    <span className={styles.cCom}>{'// grant a scoped viewing key'}</span>
                    {'\n'}
                    <span className={styles.cKey}>const</span> grant{' '}
                    <span className={styles.cKey}>=</span>{' '}
                    <span className={styles.cKey}>await</span> wallet.
                    <span className={styles.cFn}>createGrant</span>({'{'}
                    {'\n  '}scope: [<span className={styles.cStr}>'balances'</span>,{' '}
                    <span className={styles.cStr}>'positions'</span>],
                    {'\n  '}grantee: auditorPubKey,
                    {'\n  '}expiresAt: <span className={styles.cStr}>'2026-12-31'</span>,
                    {'\n'}
                    {'}'});
                    {'\n\n'}
                    <span className={styles.cCom}>{'// grantee reads only what was shared'}</span>
                    {'\n'}
                    <span className={styles.cKey}>const</span> view{' '}
                    <span className={styles.cKey}>=</span>{' '}
                    <span className={styles.cKey}>await</span> rpc.
                    <span className={styles.cFn}>viewBalances</span>(grant.id);
                  </code>
                </pre>
              </div>
            </div>
          </div>
        </section>

        {/* Ecosystem */}
        <section className={styles.shell}>
          <div className={styles.section}>
            <span className={styles.secIndex}>[ 03 ] // Ecosystem</span>
            <h2 className={styles.secTitle}>Everything you need, already on testnet.</h2>
            <p className={styles.secLead}>
              Swaps, order-book trading, a shielded pool, and tooling are live on the
              Mersennet testnet today.
            </p>
            <div className={styles.eco}>
              {ECOSYSTEM.map((e) => (
                <div key={e.name} className={styles.ecoCard}>
                  <div className={styles.ecoHead}>
                    <span className={styles.ecoName}>{e.name}</span>
                    <span
                      className={`${styles.badge} ${
                        e.status === 'Live' ? styles.badgeLive : styles.badgeSoon
                      }`}>
                      {e.status}
                    </span>
                  </div>
                  <p className={styles.ecoDesc}>{e.desc}</p>
                </div>
              ))}
            </div>
          </div>
        </section>

        {/* CTA */}
        <div className={styles.shell}>
          <div className={`${styles.hud} ${styles.ctaBand}`}>
            <h2 className={styles.ctaTitle}>Start building on Mersennet.</h2>
            <p className={styles.ctaText}>
              Deploy a contract, claim testnet PRIM, and ship private DeFi in minutes.
            </p>
            <div className={styles.ctas}>
              <a
                className={`${styles.btn} ${styles.btnPrimary}`}
                href="/developers/quick-start/hardhat">
                Start building
                <Icon path={<path d="M5 12h14M12 5l7 7-7 7" />} />
              </a>
              <a className={`${styles.btn} ${styles.btnGhost}`} href="/getting-started/faucet">
                Claim testnet PRIM
              </a>
            </div>
          </div>
        </div>
      </main>
    </Layout>
  );
}
