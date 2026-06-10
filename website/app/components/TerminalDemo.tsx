'use client';

import { useEffect, useRef, useState } from 'react';
import styles from '../page.module.css';

type Line = { text: string; kind: 'cmd' | 'out' | 'ok' | 'dim' };

// One full shielded session, accurate to the protocol: shield → private
// order → SP1 block proof → owner-side scan.
const SCRIPT: Line[] = [
  { kind: 'cmd', text: 'prim shield --amount 250 PRIM' },
  { kind: 'dim', text: '→ proving output.nr in-browser (wasm) … 388 ms' },
  { kind: 'ok', text: '✓ note committed 0x9f3a…c21e · merkle depth 32 · sender hidden' },
  { kind: 'cmd', text: 'prim order place --market PRIM-USDC --buy --size 40 --price 1.043' },
  { kind: 'dim', text: '→ proving order_place.nr — margin ≥ 5% shown in zero knowledge' },
  { kind: 'ok', text: '✓ matched 40 @ 1.043 · book precompile 0x0100 · identity hidden' },
  { kind: 'cmd', text: 'prim block prove --height 18421' },
  { kind: 'dim', text: '→ SP1 state-transition … replaying FBA tick … wrapping Groth16' },
  { kind: 'ok', text: '✓ state root 0x4e90…b7d1 anchored on Ethereum — no trusted RPC' },
  { kind: 'cmd', text: 'prim scan --viewing-key vk1q8z…ke2v' },
  { kind: 'ok', text: '✓ 3 notes · 1,250 PRIM — visible only with your key' },
];

const CHAR_MS = 26;
const LINE_PAUSE_MS = 420;
const LOOP_PAUSE_MS = 4200;

export function TerminalDemo() {
  const [lines, setLines] = useState<Line[]>([]);
  const [typed, setTyped] = useState('');
  const rootRef = useRef<HTMLDivElement | null>(null);
  const bodyRef = useRef<HTMLDivElement | null>(null);
  const running = useRef(false);
  const visible = useRef(false);

  useEffect(() => {
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
      setLines(SCRIPT);
      return;
    }

    const el = rootRef.current;
    if (!el) return;
    let cancelled = false;
    const sleep = (ms: number) =>
      new Promise<void>((resolve) => setTimeout(resolve, ms));
    const waitVisible = async () => {
      while (!visible.current && !cancelled) await sleep(200);
    };

    async function run() {
      if (running.current) return;
      running.current = true;
      while (!cancelled) {
        setLines([]);
        for (const line of SCRIPT) {
          await waitVisible();
          if (cancelled) return;
          if (line.kind === 'cmd') {
            // Type commands character by character; output appears whole.
            for (let i = 1; i <= line.text.length; i++) {
              await waitVisible();
              if (cancelled) return;
              setTyped(line.text.slice(0, i));
              await sleep(CHAR_MS);
            }
            setTyped('');
          }
          setLines((prev) => [...prev, line]);
          bodyRef.current?.scrollTo({ top: 1e6 });
          await sleep(line.kind === 'cmd' ? 120 : LINE_PAUSE_MS);
        }
        await sleep(LOOP_PAUSE_MS);
      }
    }

    const io = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          visible.current = entry.isIntersecting;
          if (entry.isIntersecting) void run();
        }
      },
      { threshold: 0.25 },
    );
    io.observe(el);
    return () => {
      cancelled = true;
      io.disconnect();
    };
  }, []);

  return (
    <div className={`${styles.hud} ${styles.term}`} ref={rootRef}>
      <div className={styles.termHead}>
        <span className={styles.termDots} aria-hidden="true">
          <i /><i /><i />
        </span>
        <span>mersennet — shielded session</span>
        <span className={styles.termLive}>● REC</span>
      </div>
      <div className={styles.termBody} ref={bodyRef} aria-live="off">
        {lines.map((l, i) => (
          <div key={i} className={styles.termLine} data-kind={l.kind}>
            {l.kind === 'cmd' ? <span className={styles.termPrompt}>$ </span> : null}
            {l.text}
          </div>
        ))}
        {typed ? (
          <div className={styles.termLine} data-kind="cmd">
            <span className={styles.termPrompt}>$ </span>
            {typed}
            <span className={styles.termCaret} aria-hidden="true" />
          </div>
        ) : (
          <div className={styles.termLine} data-kind="cmd">
            <span className={styles.termPrompt}>$ </span>
            <span className={styles.termCaret} aria-hidden="true" />
          </div>
        )}
      </div>
    </div>
  );
}
