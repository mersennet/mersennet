'use client';

import { useEffect, useRef } from 'react';

type Glyph = {
  x: number;
  y: number;
  z: number;
  ch: string;
  base: number;
  accent: boolean;
  flick: number;
};

type Blob = {
  x: number;
  y: number;
  z: number;
  color: string;
  size: number;
};

const CHARS = '01234567891379∑∏√π∞≡⊕⊗×÷ΛΦ◇▷◁';
const BLOB_COLORS = [
  '255,82,64', // red
  '255,154,60', // amber
  '91,140,255', // blue
  '176,124,255', // violet
  '125,255,155', // phosphor green
  '255,255,255', // white
];

// Signature hero: a slowly rotating sphere woven from prime digits and math
// glyphs, lit by drifting nebula blobs. Depth controls size + opacity.
export function GlyphSphere() {
  const ref = useRef<HTMLCanvasElement | null>(null);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const context = el.getContext('2d');
    if (!context) return;
    const canvas: HTMLCanvasElement = el;
    const ctx: CanvasRenderingContext2D = context;

    const reduce = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    let dpr = Math.min(window.devicePixelRatio || 1, 2);
    let w = 0;
    let h = 0;
    let R = 0;
    let glyphs: Glyph[] = [];
    let blobs: Blob[] = [];
    let raf = 0;
    let ay = 0;
    let ax = -0.35;
    const pointer = { tx: 0, ty: 0, x: 0, y: 0 };

    function build() {
      const n = Math.min(640, Math.max(280, Math.floor((w * h) / 2600)));
      glyphs = [];
      const gold = Math.PI * (3 - Math.sqrt(5));
      for (let i = 0; i < n; i++) {
        const y = 1 - (i / (n - 1)) * 2;
        const r = Math.sqrt(1 - y * y);
        const theta = gold * i;
        glyphs.push({
          x: Math.cos(theta) * r,
          y,
          z: Math.sin(theta) * r,
          ch: CHARS[(Math.random() * CHARS.length) | 0],
          base: 0.45 + Math.random() * 0.55,
          accent: Math.random() < 0.12,
          flick: Math.random() * Math.PI * 2,
        });
      }
      blobs = BLOB_COLORS.map((color, i) => {
        const y = 1 - (i / (BLOB_COLORS.length - 1)) * 1.4 - -0.2;
        const r = Math.sqrt(Math.max(0, 1 - y * y));
        const theta = i * 1.7;
        return {
          x: Math.cos(theta) * r,
          y: Math.max(-0.9, Math.min(0.9, y - 0.2)),
          z: Math.sin(theta) * r,
          color,
          size: 0.7 + Math.random() * 0.7,
        };
      });
    }

    function resize() {
      const rect = canvas.getBoundingClientRect();
      w = rect.width;
      h = rect.height;
      dpr = Math.min(window.devicePixelRatio || 1, 2);
      canvas.width = Math.max(1, Math.floor(w * dpr));
      canvas.height = Math.max(1, Math.floor(h * dpr));
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      R = Math.min(w, h) * 0.46;
      build();
    }

    function frame(t: number) {
      ctx.clearRect(0, 0, w, h);
      pointer.x += (pointer.tx - pointer.x) * 0.05;
      pointer.y += (pointer.ty - pointer.y) * 0.05;
      if (!reduce) ay += 0.0016;
      const tilt = ax + pointer.y * 0.4;
      const cosY = Math.cos(ay + pointer.x * 0.6);
      const sinY = Math.sin(ay + pointer.x * 0.6);
      const cosX = Math.cos(tilt);
      const sinX = Math.sin(tilt);
      const cx = w / 2;
      const cy = h / 2;
      const fov = 2.6;

      type Draw = { kind: 'g' | 'b'; sx: number; sy: number; z: number; p: number; ref: number };
      const draws: Draw[] = [];

      for (let i = 0; i < glyphs.length; i++) {
        const g = glyphs[i];
        const x1 = g.x * cosY - g.z * sinY;
        const z1 = g.x * sinY + g.z * cosY;
        const y1 = g.y * cosX - z1 * sinX;
        const z2 = g.y * sinX + z1 * cosX;
        const p = fov / (fov - z2);
        draws.push({ kind: 'g', sx: cx + x1 * R * p, sy: cy + y1 * R * p, z: z2, p, ref: i });
      }
      for (let i = 0; i < blobs.length; i++) {
        const b = blobs[i];
        const x1 = b.x * cosY - b.z * sinY;
        const z1 = b.x * sinY + b.z * cosY;
        const y1 = b.y * cosX - z1 * sinX;
        const z2 = b.y * sinX + z1 * cosX;
        const p = fov / (fov - z2);
        draws.push({ kind: 'b', sx: cx + x1 * R * p, sy: cy + y1 * R * p, z: z2, p, ref: i });
      }

      draws.sort((a, b) => a.z - b.z);

      ctx.textAlign = 'center';
      ctx.textBaseline = 'middle';

      for (const d of draws) {
        const depth = (d.z + 1) / 2; // 0 back .. 1 front
        if (d.kind === 'b') {
          const b = blobs[d.ref];
          const rad = R * 0.42 * b.size * d.p;
          const intensity = 0.18 + depth * 0.5;
          const grd = ctx.createRadialGradient(d.sx, d.sy, 0, d.sx, d.sy, rad);
          grd.addColorStop(0, `rgba(${b.color},${(intensity).toFixed(3)})`);
          grd.addColorStop(0.5, `rgba(${b.color},${(intensity * 0.3).toFixed(3)})`);
          grd.addColorStop(1, `rgba(${b.color},0)`);
          ctx.fillStyle = grd;
          ctx.beginPath();
          ctx.arc(d.sx, d.sy, rad, 0, Math.PI * 2);
          ctx.fill();
        } else {
          const g = glyphs[d.ref];
          const flick = reduce ? 1 : 0.7 + 0.3 * Math.sin(t * 0.004 + g.flick);
          const alpha = Math.max(0.05, (0.1 + depth * 0.9) * g.base * flick);
          const size = (9 + depth * 11) * d.p;
          ctx.font = `${size.toFixed(1)}px var(--font-mono), monospace`;
          if (g.accent) {
            ctx.fillStyle = `rgba(125,255,155,${alpha.toFixed(3)})`;
            ctx.shadowColor = 'rgba(125,255,155,0.6)';
            ctx.shadowBlur = depth * 6;
          } else {
            ctx.fillStyle = `rgba(214,221,235,${alpha.toFixed(3)})`;
            ctx.shadowBlur = 0;
          }
          ctx.fillText(g.ch, d.sx, d.sy);
          ctx.shadowBlur = 0;
        }
      }

      if (!reduce && t % 6 < 1) {
        const g = glyphs[(Math.random() * glyphs.length) | 0];
        if (g) g.ch = CHARS[(Math.random() * CHARS.length) | 0];
      }

      if (!reduce) raf = requestAnimationFrame(frame);
    }

    function onPointer(e: PointerEvent) {
      pointer.tx = (e.clientX / window.innerWidth - 0.5) * 2;
      pointer.ty = (e.clientY / window.innerHeight - 0.5) * 2;
    }

    resize();
    if (reduce) {
      frame(0);
    } else {
      raf = requestAnimationFrame(frame);
      window.addEventListener('pointermove', onPointer, { passive: true });
    }
    window.addEventListener('resize', resize);

    return () => {
      cancelAnimationFrame(raf);
      window.removeEventListener('resize', resize);
      window.removeEventListener('pointermove', onPointer);
    };
  }, []);

  return <canvas ref={ref} aria-hidden="true" />;
}
