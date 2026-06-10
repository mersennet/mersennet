import styles from '../page.module.css';
import { Reveal } from './Reveal';

const STEPS = [
  {
    title: 'Prove locally',
    text: 'Your wallet builds a zero-knowledge proof in-browser with the WASM Noir prover. Keys never leave the device.',
  },
  {
    title: 'Submit shielded',
    text: 'Transactions carry nullifiers and output commitments — no addresses, balances, or order details in the clear.',
  },
  {
    title: 'Verify in ZK',
    text: 'The node checks the proof and the nullifier set. Solvency and margin are enforced without exposing positions.',
  },
  {
    title: 'Prove the block',
    text: 'Each block is proven with SP1 and Groth16-wrapped for Ethereum — verifiable from a succinct proof.',
  },
];

export function HowItWorks() {
  return (
    <section className={styles.shell} id="how">
      <div className={styles.section}>
        <Reveal as="header" className={styles.secHead}>
          <span className={styles.secIndex}>[ 04 ] // Protocol</span>
          <h2 className={styles.secTitle}>Private by construction. Verifiable by anyone.</h2>
        </Reveal>
        <div className={styles.steps}>
          {STEPS.map((s, i) => (
            <Reveal as="div" key={s.title} delay={i * 70} className={styles.step}>
              <div className={styles.stepNum}>{String(i + 1).padStart(2, '0')}</div>
              <h3 className={styles.stepTitle}>{s.title}</h3>
              <p className={styles.stepText}>{s.text}</p>
            </Reveal>
          ))}
        </div>
      </div>
    </section>
  );
}
