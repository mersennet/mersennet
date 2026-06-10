import styles from '../page.module.css';
import { LINKS, SITE } from '../site';
import { BrandMark } from './Icon';

const COLUMNS = [
  {
    title: 'Build',
    links: [
      { label: 'Quick Start', href: LINKS.quickStart },
      { label: 'RPC Reference', href: LINKS.rpc },
      { label: 'JavaScript SDK', href: LINKS.sdk },
      { label: 'Docs', href: LINKS.docs },
    ],
  },
  {
    title: 'Network',
    links: [
      { label: 'Block Explorer', href: LINKS.explorer },
      { label: 'Trade', href: LINKS.trade },
      { label: 'Faucet', href: LINKS.faucet },
      { label: 'Network Info', href: LINKS.networkInfo },
    ],
  },
  {
    title: 'Community',
    links: [
      { label: 'GitHub', href: LINKS.github },
      { label: 'Whitepaper', href: LINKS.whitepaper },
    ],
  },
];

export function Footer() {
  return (
    <footer className={styles.footer}>
      <div className={styles.footerInner}>
        <div>
          <a className={styles.brand} href="/" aria-label={`${SITE.name} home`}>
            <span className={styles.brandMark}>
              <BrandMark size={22} flat />
            </span>
            {SITE.name}
          </a>
          <p className={styles.footerBrandText}>
            A zero-knowledge Layer 1 for account-level privacy on the EVM and a native
            on-chain order book.
          </p>
        </div>
        {COLUMNS.map((col) => (
          <div key={col.title}>
            <div className={styles.footerColTitle}>{col.title}</div>
            {col.links.map((l) => (
              <a
                key={l.label}
                className={styles.footerLink}
                href={l.href}
                {...(l.href.startsWith('http')
                  ? { target: '_blank', rel: 'noopener noreferrer' }
                  : {})}
              >
                {l.label}
              </a>
            ))}
          </div>
        ))}
      </div>
      <div className={styles.footerBottom}>
        <span>© {new Date().getFullYear()} {SITE.name} · Prime Numbers Labs</span>
        <span>Chain {SITE.chainId} · {SITE.symbol}</span>
      </div>
    </footer>
  );
}
