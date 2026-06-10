import styles from '../page.module.css';
import { Reveal } from './Reveal';

const PILLARS = [
  {
    stat: '$18.5T+',
    statLabel: 'addressable market by 2030',
    title: 'Markets pay for transparency.',
    text: 'On a public chain every position, every order, every liquidation level is broadcast to your counterparties. Institutions tokenizing billions in assets have nowhere to trade them without leaking their book. That is not a feature — it is the single largest unpriced cost in on-chain finance.',
  },
  {
    stat: '2 of 3',
    statLabel: 'properties is where everyone else stops',
    title: 'The field is split. We are not.',
    text: 'Transparent perp chains trade fast but leak everything. Privacy chains hide everything but cannot run real DeFi. General EVMs run everything slowly and in public. Private, verifiable, and a native order book in one chain is the empty quadrant — and it is the one institutions need.',
  },
  {
    stat: '0',
    statLabel: 'code changes to deploy here',
    title: 'Distribution is built in.',
    text: 'Mersennet is EVM-equivalent: every Solidity contract, every wallet, every tool works unchanged. Privacy is not a new platform to bet on — it is one import added to the stack developers already ship with. The switching cost is a chain ID.',
  },
];

export function Thesis() {
  return (
    <section className={styles.shell} id="thesis">
      <div className={styles.section}>
        <Reveal as="header" className={styles.secHead}>
          <span className={styles.secIndex}>[ 02 ] // The thesis</span>
          <h2 className={styles.secTitle}>Why this network wins.</h2>
        </Reveal>
        <div className={styles.thesisGrid}>
          {PILLARS.map((p, i) => (
            <Reveal as="article" key={p.title} delay={i * 90} className={`${styles.hud} ${styles.thesisCard}`}>
              <div className={styles.thesisStat}>{p.stat}</div>
              <div className={styles.thesisStatLabel}>{p.statLabel}</div>
              <h3 className={styles.thesisTitle}>{p.title}</h3>
              <p className={styles.thesisText}>{p.text}</p>
            </Reveal>
          ))}
        </div>
      </div>
    </section>
  );
}
