import styles from '../page.module.css';
import { Reveal } from './Reveal';

const PHASES = [
  {
    tag: 'SHIPPED',
    title: 'The chain is built',
    live: true,
    items: [
      '~44K lines of Rust · 300+ tests green',
      'Public testnet live — RPC, faucet, explorer',
      'Parallel EVM + native CLOB + shielded accounts',
      'SDKs in TypeScript, Python, Go',
    ],
  },
  {
    tag: 'IN FLIGHT',
    title: 'Proofs go production',
    live: false,
    items: [
      'Groth16 verifying key for the Ethereum bridge',
      'Third-party security audit',
      'Real-prover lane (SP1 + Barretenberg) in CI',
    ],
  },
  {
    tag: 'NEXT',
    title: 'Mainnet & MRSN',
    live: false,
    items: [
      'Genesis ceremony · 7+ independent validators',
      'MRSN token generation — 1B fixed supply',
      'Privacy hard fork activated by governance',
    ],
  },
];

export function Momentum() {
  return (
    <section className={styles.shell} id="momentum">
      <div className={styles.section}>
        <Reveal as="header" className={styles.secHead}>
          <span className={styles.secIndex}>[ 08 ] // Momentum</span>
          <h2 className={styles.secTitle}>Most of the risk is already retired.</h2>
          <p className={styles.secLead}>
            This is not a whitepaper project. The execution engine, the order book, the
            shielded state machine, and the proof pipeline exist today as working,
            tested, open code you can run.
          </p>
        </Reveal>
        <div className={styles.phases}>
          {PHASES.map((p, i) => (
            <Reveal as="article" key={p.tag} delay={i * 90} className={`${styles.hud} ${styles.phase}`}>
              <div className={styles.phaseHead}>
                <span className={p.live ? styles.phaseTagLive : styles.phaseTag}>{p.tag}</span>
                {p.live ? <span className={styles.phaseDot} aria-hidden="true" /> : null}
              </div>
              <h3 className={styles.phaseTitle}>{p.title}</h3>
              <ul className={styles.phaseList}>
                {p.items.map((it) => (
                  <li key={it}>{it}</li>
                ))}
              </ul>
            </Reveal>
          ))}
        </div>
      </div>
    </section>
  );
}
