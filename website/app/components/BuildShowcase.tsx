import styles from '../page.module.css';
import { Reveal } from './Reveal';
import { TerminalDemo } from './TerminalDemo';
import { CodeTabs } from './CodeTabs';
import { LINKS } from '../site';

export function BuildShowcase() {
  return (
    <section className={styles.shell} id="build">
      <div className={styles.section}>
        <Reveal as="header" className={styles.secHead}>
          <span className={styles.secIndex}>[ 03 ] // Build</span>
          <h2 className={styles.secTitle}>Your stack already works here.</h2>
          <p className={styles.secLead}>
            Solidity, Hardhat, Foundry, ethers — unchanged. Privacy is one import away:
            an order book at precompile <code className={styles.inlineCode}>0x0100</code>,
            shielded accounts behind <code className={styles.inlineCode}>0x0200</code>,
            and an SDK that proves in the browser.
          </p>
        </Reveal>

        <div className={styles.buildGrid}>
          <Reveal as="div">
            <TerminalDemo />
          </Reveal>
          <Reveal as="div" delay={120}>
            <CodeTabs />
            <div className={styles.buildLinks}>
              <a href={LINKS.quickStart}>Quick start ↗</a>
              <a href={LINKS.sdk}>SDK reference ↗</a>
              <a href={LINKS.rpc}>RPC methods ↗</a>
            </div>
          </Reveal>
        </div>
      </div>
    </section>
  );
}
