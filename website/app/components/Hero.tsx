import styles from '../page.module.css';
import { LINKS, SITE } from '../site';
import { ArrowRight } from './Icon';
import { GlyphSphere } from './GlyphSphere';

export function Hero() {
  return (
    <header className={styles.hero}>
      <div className={styles.heroCanvas}>
        <GlyphSphere />
      </div>

      <div className={styles.heroGrid}>
        <div className={styles.heroLeft}>
          <span className={styles.kicker}>Nº {SITE.chainId} · Zero-knowledge Layer 1</span>
          <h1 className={styles.title}>
            Private,
            <br />
            verifiable
            <br />
            <span className={styles.titleDim}>money.</span>
            <span className={styles.cursor} aria-hidden="true" />
          </h1>
          <p className={styles.lede}>
            {SITE.name} brings account-level privacy to the EVM and a native on-chain order
            book — leverage secured by zero-knowledge risk checks, state proven end to end
            with SP1.
          </p>
          <div className={styles.heroCtas}>
            <a className={`${styles.btn} ${styles.btnPrimary}`} href={LINKS.docs}>
              Enter the network
              <ArrowRight />
            </a>
            <a className={`${styles.btn} ${styles.btnGhost}`} href={LINKS.whitepaper}>
              Whitepaper
            </a>
          </div>
          <div className={styles.heroSpecs}>
            <div className={styles.heroSpec}>
              <span className={styles.heroSpecVal}>{SITE.blockTime}</span>
              <span className={styles.heroSpecLabel}>Block time</span>
            </div>
            <div className={styles.heroSpec}>
              <span className={styles.heroSpecVal}>EVM</span>
              <span className={styles.heroSpecLabel}>Equivalent</span>
            </div>
            <div className={styles.heroSpec}>
              <span className={styles.heroSpecVal}>ZK</span>
              <span className={styles.heroSpecLabel}>Proven state</span>
            </div>
          </div>
        </div>

        <div className={styles.heroRight}>
          <div className={`${styles.hud} ${styles.panel}`}>
            <div className={styles.panelHead}>
              <span>// Network</span>
              <span>ONLINE</span>
            </div>
            <div className={styles.panelRow}>
              <span>Chain ID</span>
              <b>{SITE.chainId}</b>
            </div>
            <div className={styles.panelRow}>
              <span>Native token</span>
              <b>{SITE.symbol}</b>
            </div>
            <div className={styles.panelRow}>
              <span>Consensus</span>
              <b>{SITE.consensus} · {SITE.blockTime}</b>
            </div>
            <div className={styles.panelRow}>
              <span>Proof system</span>
              <b>SP1 → Groth16</b>
            </div>
            <div className={styles.panelRow}>
              <span>RPC</span>
              <b>{SITE.rpcUrl}</b>
            </div>
          </div>

          <div className={`${styles.hud} ${styles.panel}`}>
            <div className={styles.panelHead}>
              <span>// Get started</span>
              <span>↳</span>
            </div>
            <p className={styles.panelText}>
              Claim testnet {SITE.symbol}, deploy a contract, and ship private DeFi in
              minutes.
            </p>
            <a
              className={`${styles.btn} ${styles.btnSm}`}
              href={LINKS.faucet}
              target="_blank"
              rel="noopener noreferrer"
            >
              Open faucet →
            </a>
          </div>
        </div>
      </div>
    </header>
  );
}
