import styles from '../page.module.css';
import { Reveal } from './Reveal';
import { SITE } from '../site';

const ECOSYSTEM = [
  { name: 'PrimeSwap V2', status: 'Live', desc: 'AMM DEX with constant-product pools.' },
  { name: 'PrimeSwap V3', status: 'Live', desc: 'Concentrated liquidity, Uniswap-V3 style.' },
  { name: 'PrimeTrade', status: 'Live', desc: 'Order-book trading terminal on the native CLOB.' },
  { name: 'PrimeOrders', status: 'Live', desc: 'Native order book via precompile 0x0100.' },
  { name: 'Shielded Pool', status: 'Live', desc: 'Deposit, transfer, and trade privately.' },
  { name: 'PrimeFi', status: 'Soon', desc: 'Private lending & borrowing markets.' },
];

export function Ecosystem() {
  return (
    <section className={styles.shell} id="ecosystem">
      <div className={styles.section}>
        <Reveal as="header" className={styles.secHead}>
          <span className={styles.secIndex}>[ 09 ] // Ecosystem</span>
          <h2 className={styles.secTitle}>Everything you need, already on testnet.</h2>
          <p className={styles.secLead}>
            Swaps, order-book trading, a shielded pool, and tooling are live on the{' '}
            {SITE.name} testnet today.
          </p>
        </Reveal>
        <div className={styles.eco}>
          {ECOSYSTEM.map((e, i) => (
            <Reveal as="div" key={e.name} delay={i * 50} className={styles.ecoCard}>
              <div className={styles.ecoHead}>
                <span className={styles.ecoName}>{e.name}</span>
                <span
                  className={`${styles.badge} ${
                    e.status === 'Live' ? styles.badgeLive : styles.badgeSoon
                  }`}
                >
                  {e.status}
                </span>
              </div>
              <p className={styles.ecoDesc}>{e.desc}</p>
            </Reveal>
          ))}
        </div>
      </div>
    </section>
  );
}
