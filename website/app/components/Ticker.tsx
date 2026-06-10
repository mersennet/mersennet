import styles from '../page.module.css';

const ITEMS = [
  'Zero-knowledge',
  'Shielded accounts',
  'Native order book',
  'ZK risk checks',
  'SP1 proven state',
  'Selective disclosure',
  '2³¹−1 = 2147483647',
  'Groth16 bridge',
];

export function Ticker() {
  return (
    <div className={styles.marquee} aria-hidden="true">
      <div className={styles.marqueeTrack}>
        <span>
          {ITEMS.map((t, i) => (
            <span key={`a${i}`}>
              <span className={styles.marqueeStar}>✦</span> {t}
            </span>
          ))}
        </span>
        <span>
          {ITEMS.map((t, i) => (
            <span key={`b${i}`}>
              <span className={styles.marqueeStar}>✦</span> {t}
            </span>
          ))}
        </span>
      </div>
    </div>
  );
}
