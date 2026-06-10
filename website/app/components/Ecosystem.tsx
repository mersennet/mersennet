import styles from '../page.module.css';
import { Reveal } from './Reveal';
import { SITE, LINKS } from '../site';

const ECOSYSTEM = [
  {
    name: 'Mersennet Trade',
    status: 'Live',
    desc: 'Order-book trading terminal on the native CLOB.',
    href: LINKS.trade,
  },
  {
    name: 'Mersennet Explorer',
    status: 'Live',
    desc: 'Privacy-aware block explorer with ZK proof verification.',
    href: LINKS.explorer,
  },
  {
    name: 'Mersennet Faucet',
    status: 'Live',
    desc: 'Testnet PRIM for development and testing.',
    href: LINKS.faucet,
  },
  { name: 'PrimeOrders', status: 'Live', desc: 'Native order book via precompile 0x0100.' },
  { name: 'Shielded Pool', status: 'Live', desc: 'Deposit, transfer, and trade privately.' },
  { name: 'Private Lending', status: 'Soon', desc: 'Private lending & borrowing markets.' },
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
                {'href' in e && e.href ? (
                  <a
                    className={styles.ecoName}
                    href={e.href}
                    target="_blank"
                    rel="noopener noreferrer"
                  >
                    {e.name}
                  </a>
                ) : (
                  <span className={styles.ecoName}>{e.name}</span>
                )}
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
