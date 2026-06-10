import styles from '../page.module.css';
import { LINKS, SITE } from '../site';
import { ArrowRight } from './Icon';
import { Reveal } from './Reveal';

export function CtaBand() {
  return (
    <div className={styles.ctaWrap}>
      <Reveal as="div" className={`${styles.hud} ${styles.cta}`}>
        <h2 className={styles.ctaTitle}>Build on the private network.</h2>
        <p className={styles.ctaText}>
          Deploy a contract, claim testnet {SITE.symbol}, and ship private DeFi in minutes.
        </p>
        <div className={styles.ctaBtns}>
          <a className={`${styles.btn} ${styles.btnPrimary}`} href={LINKS.quickStart}>
            Start building
            <ArrowRight />
          </a>
          <a
            className={`${styles.btn} ${styles.btnGhost}`}
            href={LINKS.faucet}
            target="_blank"
            rel="noopener noreferrer"
          >
            Claim testnet {SITE.symbol}
          </a>
        </div>
      </Reveal>
    </div>
  );
}
