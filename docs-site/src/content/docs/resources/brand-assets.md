---
title: "Brand Assets"
---

Brand guidelines for Mersennet — colors, typography, logo usage, and downloadable assets for developers, partners, and community members.

## Logo

The Mersennet mark is the letter **M drawn as five vertical bars**, anchored to a
common top line. Five binary ones — `11111₂` = 31 = 2⁵−1 — form a Mersenne
prime: the name is written into the mark in binary. The mark is set in
phosphor green (`#7dff9b`) on black.

<img src="/logo.svg" alt="Mersennet mark" width="96" height="96" />

Download: [logo.svg](/logo.svg) · [favicon.svg](/favicon.svg) · [social card](/mersennet-social.svg)

### Guidelines

- The five bars are always **top-anchored and symmetric** (heights 5·2·3·2·5). Never bottom-anchor them — that reads as an audio equalizer, not the M.
- Use phosphor green `#7dff9b` on dark backgrounds and deep green `#0c8f43` on light backgrounds.
- Maintain clear space around the mark equal to one bar width.
- Do not stretch, rotate, re-space, or re-proportion the bars.
- For monochrome contexts the mark may be set in pure white or pure black.

### Don'ts

- Do not change the number of bars — five is the point (11111₂ = 31).
- Do not apply gradients, shadows, or outlines.
- Do not place the mark on busy or low-contrast backgrounds.
- Do not round the bars into circles or taper them.

## Color Palette

### Primary Brand Colors

| Name | Hex | RGB | Usage |
|------|-----|-----|-------|
| **Phosphor Green** | `#7dff9b` | 125, 255, 155 | Primary brand color, logo, CTAs, accents on dark |
| **Deep Green** | `#0c8f43` | 12, 143, 67 | Logo and accents on light backgrounds |
| **Teal** | `#40e0b4` | 64, 224, 180 | Secondary accent (charts, glows) |

### Background Colors (Dark Theme)

| Name | Hex | Usage |
|------|-----|-------|
| **Base** | `#0b0b12` | Page background, navbar, footer |
| **Surface** | `#111122` | Content area background |
| **Elevated** | `#14142a` | Cards, panels, elevated surfaces |

### Supporting Colors

| Name | Hex | Usage |
|------|-----|-------|
| **Success** | `#3fe57f` | Success states, confirmations |
| **Warning** | `#f59e0b` | Warnings, "Coming Soon" badges |
| **Danger** | `#ef4444` | Errors, destructive actions |
| **Text Primary** | `#e8edf5` | Primary text on dark backgrounds |
| **Text Secondary** | `#94a3b8` | Subtitles, descriptions, muted text |

### Brand Gradient

The signature Mersennet gradient flows from **Violet** through **Violet Light** to **Cyan**:

```css
background: linear-gradient(135deg, #4901FF 0%, #6d2fff 50%, #00FFF9 100%);
```

Used for: hero titles, primary CTA buttons, top bars, logo fills, and accent borders.

## Typography

### UI / Headings

- **Font:** Sora
- **Source:** [Google Fonts](https://fonts.google.com/specimen/Sora)
- **Usage:** Headings, navigation, body text, buttons
- **Weights:** 400 (regular), 500 (medium), 600 (semibold), 700 (bold), 800 (extra-bold)

### Code / Monospace

- **Font:** JetBrains Mono
- **Source:** [Google Fonts](https://fonts.google.com/specimen/JetBrains+Mono)
- **Usage:** Code blocks, addresses, chain IDs, technical content
- **Weight:** 400 (regular), 500 (medium), 600 (semibold)

### CSS Variables

```css
:root {
  --prime-accent: #00FFF9;
  --prime-accent-dim: rgba(0, 255, 249, 0.15);
  --prime-accent-subtle: rgba(0, 255, 249, 0.08);
  --prime-violet: #4901FF;
  --prime-violet-light: #6d2fff;
  --prime-bg-base: #0b0b12;
  --prime-bg-surface: #111122;
  --prime-bg-elevated: #14142a;
  --prime-font-ui: 'Sora', system-ui, -apple-system, sans-serif;
  --prime-font-mono: 'JetBrains Mono', 'Fira Code', monospace;
}
```

## Downloadable Assets

| Asset | Format | Description |
|-------|--------|-------------|
| Logo (gradient) | SVG | Double-helix logo with violet→cyan gradient |
| Favicon | SVG | Browser tab icon |
| Social Card | PNG | Open Graph / Twitter share image (1200×630) |

:::tip
All logo files are available in the [docs-site repository](https://github.com/mersennet/mersennet/tree/main/docs-site/static/img).
:::

## Integration Guide

When building dApps or documentation for Mersennet:

1. Use the **violet-to-cyan gradient** for primary actions and hero elements.
2. Use **Cyan (`#00FFF9`)** for links, active states, and accent highlights.
3. Use **Sora** for UI text and **JetBrains Mono** for code.
4. Prefer dark backgrounds (`#0b0b12` base) for a consistent Mersennet look.
5. Import fonts via Google Fonts:

```html
<link href="https://fonts.googleapis.com/css2?family=Sora:wght@400;500;600;700;800&family=JetBrains+Mono:wght@400;500;600&display=swap" rel="stylesheet">
```

## Contact

For custom brand requests, partnerships, or asset access, reach out via the [Mersennet GitHub](https://github.com/mersennet/mersennet) or community channels.
