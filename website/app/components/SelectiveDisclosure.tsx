import styles from '../page.module.css';
import { Reveal } from './Reveal';

const BULLETS = [
  {
    title: 'Grant-gated reads.',
    text: 'Reconstruct balances, positions, and orders only for holders of a valid disclosure grant.',
  },
  {
    title: 'Client-side proving.',
    text: 'Generate proofs in the browser with the WASM Noir prover — keys never leave the wallet.',
  },
  {
    title: 'Note scanning.',
    text: 'Wallets rebuild private state by scanning notes and tracking nullifiers — no central indexer of your funds.',
  },
];

export function SelectiveDisclosure() {
  return (
    <section className={styles.shell} id="privacy">
      <div className={styles.section}>
        <Reveal as="header" className={styles.secHead}>
          <span className={styles.secIndex}>[ 03 ] // Selective disclosure</span>
          <h2 className={styles.secTitle}>Private by default. Auditable on your terms.</h2>
          <p className={styles.secLead}>
            Grant a viewing key to an auditor, exchange, or counterparty and reveal exactly
            what you choose — a balance, a position, a single order — without exposing the
            rest of your account.
          </p>
        </Reveal>

        <div className={styles.split}>
          <Reveal as="div">
            <ul className={styles.bullets}>
              {BULLETS.map((b) => (
                <li key={b.title}>
                  <span className={styles.check}>[✓]</span>
                  <span>
                    <strong>{b.title}</strong> {b.text}
                  </span>
                </li>
              ))}
            </ul>
          </Reveal>

          <Reveal as="div" delay={100} className={`${styles.hud} ${styles.codeCard}`}>
            <div className={styles.codeBar}>
              <span>FIG.03</span>
              <b>disclosure.ts</b>
            </div>
            <pre className={styles.code}>
              <code>
                <span className={styles.cCom}>{'// grant a scoped viewing key'}</span>
                {'\n'}
                <span className={styles.cKey}>const</span> grant{' '}
                <span className={styles.cKey}>=</span> <span className={styles.cKey}>await</span>{' '}
                wallet.
                <span className={styles.cFn}>createGrant</span>({'{'}
                {'\n  '}scope: [<span className={styles.cStr}>{"'balances'"}</span>,{' '}
                <span className={styles.cStr}>{"'positions'"}</span>],
                {'\n  '}grantee: auditorPubKey,
                {'\n  '}expiresAt: <span className={styles.cStr}>{"'2026-12-31'"}</span>,
                {'\n'}
                {'}'});
                {'\n\n'}
                <span className={styles.cCom}>
                  {'// grantee reconstructs only what was shared'}
                </span>
                {'\n'}
                <span className={styles.cKey}>const</span> view{' '}
                <span className={styles.cKey}>=</span> <span className={styles.cKey}>await</span>{' '}
                rpc.
                <span className={styles.cFn}>viewBalances</span>(grant.id);
              </code>
            </pre>
          </Reveal>
        </div>
      </div>
    </section>
  );
}
