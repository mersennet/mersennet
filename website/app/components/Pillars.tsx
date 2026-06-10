import type { ReactNode } from 'react';
import styles from '../page.module.css';
import { Icon } from './Icon';
import { Reveal } from './Reveal';
import { SITE } from '../site';

const PILLARS: { title: string; text: string; icon: ReactNode }[] = [
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
    text: 'A central limit order book exposed to Solidity through a precompile. Atomic, composable matching beyond AMMs — now with shielded order placement.',
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

export function Pillars() {
  return (
    <section className={styles.shell} id="network">
      <div className={styles.section}>
        <Reveal as="header" className={styles.secHead}>
          <span className={styles.secIndex}>[ 01 ] // The network</span>
          <h2 className={styles.secTitle}>
            A chain where privacy and verifiability are the defaults.
          </h2>
          <p className={styles.secLead}>
            Most chains make you choose between transparency and confidentiality.{' '}
            {SITE.name} uses zero-knowledge proofs to deliver both.
          </p>
        </Reveal>
        <div className={styles.pillars}>
          {PILLARS.map((p, i) => (
            <Reveal as="article" key={p.title} delay={i * 70} className={styles.pillar}>
              <div className={styles.pillarTop}>
                <span className={styles.pillarNum}>P.{String(i + 1).padStart(2, '0')}</span>
                <span className={styles.pillarIcon}>{p.icon}</span>
              </div>
              <h3 className={styles.pillarTitle}>{p.title}</h3>
              <p className={styles.pillarText}>{p.text}</p>
            </Reveal>
          ))}
        </div>
      </div>
    </section>
  );
}
