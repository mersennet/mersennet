import styles from '../page.module.css';
import { Reveal } from './Reveal';

const ROWS: { k: string; v: string; ok?: boolean }[] = [
  { k: 'blockHeight', v: '500' },
  { k: 'prevStateRoot', v: '0x9f3a…b21c' },
  { k: 'newStateRoot', v: '0x4d77…0e8a' },
  { k: 'nullifierRoot', v: '0xc015…7f1d' },
  { k: 'proofType', v: 'SP1 → Groth16' },
  { k: 'verify()', v: 'ok: true', ok: true },
];

export function Verifiability() {
  return (
    <section className={`${styles.shell}`} id="verifiable">
      <div className={`${styles.section} ${styles.sectionDark}`}>
        <Reveal as="header" className={styles.secHead}>
          <span className={styles.secIndex}>[ 04 ] // Verifiable state</span>
          <h2 className={styles.secTitle}>Don&apos;t trust the node. Verify the proof.</h2>
        </Reveal>

        <div className={styles.proof}>
          <Reveal as="div" className={`${styles.hud} ${styles.proofReadout}`}>
            {ROWS.map((r) => (
              <div key={r.k} className={styles.proofRow}>
                <span className={styles.proofKey}>{r.k}</span>
                <span className={r.ok ? styles.proofValOk : styles.proofVal}>{r.v}</span>
              </div>
            ))}
          </Reveal>
          <Reveal as="div" delay={100}>
            <p className={styles.secLead} style={{ marginBottom: '1rem' }}>
              Every block&apos;s state transition is proven with{' '}
              <strong style={{ color: 'var(--text)' }}>SP1</strong> and wrapped into a{' '}
              <strong style={{ color: 'var(--text)' }}>Groth16</strong> proof an Ethereum
              contract can verify. Light clients accept Mersennet state roots from a
              succinct proof — no full node, no trusted RPC, no re-execution.
            </p>
            <p className={styles.secLead}>
              The same nullifier set that protects shielded accounts is proven correct in
              every block, so privacy and safety hold together — publicly checkable, fully
              private.
            </p>
          </Reveal>
        </div>
      </div>
    </section>
  );
}
