import styles from '../page.module.css';
import { Reveal } from './Reveal';

const STATS = [
  { num: '7919', label: 'Chain ID · prime' },
  { num: '~1s', label: 'Block time' },
  { num: 'EVM', label: 'Shanghai-equivalent' },
  { num: 'SP1', label: 'Proven state' },
];

export function StatBand() {
  return (
    <div className={styles.shell}>
      <Reveal as="div" className={styles.stats}>
        {STATS.map((s) => (
          <div key={s.label} className={styles.stat}>
            <div className={styles.statNum}>{s.num}</div>
            <div className={styles.statLabel}>{s.label}</div>
          </div>
        ))}
      </Reveal>
    </div>
  );
}
