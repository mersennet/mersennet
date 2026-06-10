import styles from '../page.module.css';
import { Reveal } from './Reveal';

// Bar widths use a sqrt scale so order-of-magnitude gaps stay readable.
const tpsMax = Math.sqrt(72181);
const TPS = [
  { name: 'Mersennet', value: '72,181', w: 100, us: true },
  { name: 'Monad (claimed)', value: '10,000', w: (Math.sqrt(10000) / tpsMax) * 100 },
  { name: 'Solana', value: '~4,000', w: (Math.sqrt(4000) / tpsMax) * 100 },
  { name: 'Ethereum L1', value: '~15', w: (Math.sqrt(15) / tpsMax) * 100 },
];
const clobMax = Math.sqrt(2484170);
const CLOB = [
  { name: 'Mersennet', value: '2,484,170', w: 100, us: true },
  { name: 'Hyperliquid', value: '~200,000', w: (Math.sqrt(200000) / clobMax) * 100 },
  { name: 'dYdX v4', value: '~100,000', w: (Math.sqrt(100000) / clobMax) * 100 },
];

const MOAT_COLS = ['Mersennet', 'Hyperliquid', 'dYdX v4', 'Aleo / Zcash', 'EVM L2s'];
const MOAT_ROWS: { cap: string; marks: (boolean | null)[] }[] = [
  { cap: 'Private accounts & positions', marks: [true, false, false, true, false] },
  { cap: 'Native on-chain order book', marks: [true, true, true, false, false] },
  { cap: 'Atomic EVM ↔ CLOB in one tx', marks: [true, false, false, false, false] },
  { cap: 'ZK-proven state (light clients)', marks: [true, false, false, true, null] },
  { cap: 'Selective disclosure for auditors', marks: [true, false, false, null, false] },
  { cap: 'Unchanged Solidity tooling', marks: [true, null, false, false, true] },
];

function Mark({ v }: { v: boolean | null }) {
  if (v === true) return <span className={styles.moatYes}>✓</span>;
  if (v === false) return <span className={styles.moatNo}>—</span>;
  return <span className={styles.moatPart}>◐</span>;
}

export function Edge() {
  return (
    <section className={styles.shell} id="edge">
      <div className={`${styles.section} ${styles.sectionDark}`}>
        <Reveal as="header" className={styles.secHead}>
          <span className={styles.secIndex}>[ 03 ] // The edge</span>
          <h2 className={styles.secTitle}>Measured, not promised.</h2>
        </Reveal>

        <div className={styles.edgeCharts}>
          <Reveal as="div" className={`${styles.hud} ${styles.chart}`}>
            <div className={styles.chartTitle}>EVM throughput · transactions / second</div>
            {TPS.map((r) => (
              <div key={r.name} className={styles.chartRow}>
                <span className={styles.chartName}>{r.name}</span>
                <span className={styles.chartTrack}>
                  <span
                    className={`${styles.chartBar} ${r.us ? styles.chartBarUs : ''}`}
                    style={{ width: `${r.w}%` }}
                  />
                </span>
                <span className={styles.chartVal}>{r.value}</span>
              </div>
            ))}
          </Reveal>
          <Reveal as="div" delay={100} className={`${styles.hud} ${styles.chart}`}>
            <div className={styles.chartTitle}>Order-book throughput · operations / second</div>
            {CLOB.map((r) => (
              <div key={r.name} className={styles.chartRow}>
                <span className={styles.chartName}>{r.name}</span>
                <span className={styles.chartTrack}>
                  <span
                    className={`${styles.chartBar} ${r.us ? styles.chartBarUs : ''}`}
                    style={{ width: `${r.w}%` }}
                  />
                </span>
                <span className={styles.chartVal}>{r.value}</span>
              </div>
            ))}
          </Reveal>
        </div>

        <Reveal as="div" delay={150} className={styles.moatWrap}>
          <table className={styles.moat}>
            <thead>
              <tr>
                <th>Capability</th>
                {MOAT_COLS.map((c, i) => (
                  <th key={c} className={i === 0 ? styles.moatUsCol : ''}>{c}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              {MOAT_ROWS.map((r) => (
                <tr key={r.cap}>
                  <td>{r.cap}</td>
                  {r.marks.map((m, i) => (
                    <td key={i} className={i === 0 ? styles.moatUsCol : ''}>
                      <Mark v={m} />
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </Reveal>

        <Reveal as="div" delay={200}>
          <p className={styles.finePrint}>
            Mersennet figures measured on reference hardware against synthetic workloads
            (open-source benchmarks in the repo). Competitor figures are public claims or
            observed mainnet throughput; bar lengths are square-root scaled.
          </p>
        </Reveal>
      </div>
    </section>
  );
}
