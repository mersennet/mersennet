---
slug: /resources/brand-assets
sidebar_position: 2
title: "Brand Assets"
---

# Brand Assets

Brand guidelines for Prime Chain — colors, typography, logo usage, and downloadable assets for developers, partners, and community members.

## Logo

The Prime Chain logo is a **double-helix** symbol representing interconnected value chains. It uses a violet-to-cyan gradient that defines the entire brand identity.

### Guidelines

- Use the official Prime Chain double-helix logo for all Prime Chain–related materials.
- Maintain clear space around the logo (minimum: the height of the central ellipse).
- Do not stretch, rotate, or alter the logo proportions.
- Use the gradient version on dark backgrounds; use a white solid version on light backgrounds.
- Avoid placing the logo on busy or low-contrast backgrounds.

### Don'ts

- Do not change the gradient colors outside the approved palette.
- Do not add effects (shadows, outlines) that obscure the logo.
- Do not use low-resolution or pixelated versions.
- Do not separate the helix strands from the central ellipse.

## Color Palette

### Primary Brand Colors

| Name | Hex | RGB | Usage |
|------|-----|-----|-------|
| **Violet** | `#4901FF` | 73, 1, 255 | Primary brand color, gradients, CTAs |
| **Violet Light** | `#6d2fff` | 109, 47, 255 | Gradient midpoint, hover states |
| **Cyan** | `#00FFF9` | 0, 255, 249 | Accent color, links, highlights, status indicators |

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

The signature Prime Chain gradient flows from **Violet** through **Violet Light** to **Cyan**:

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
All logo files are available in the [docs-site repository](https://github.com/PrimeNumbersLabs/prime-chain/tree/main/docs-site/static/img).
:::

## Integration Guide

When building dApps or documentation for Prime Chain:

1. Use the **violet-to-cyan gradient** for primary actions and hero elements.
2. Use **Cyan (`#00FFF9`)** for links, active states, and accent highlights.
3. Use **Sora** for UI text and **JetBrains Mono** for code.
4. Prefer dark backgrounds (`#0b0b12` base) for a consistent Prime Chain look.
5. Import fonts via Google Fonts:

```html
<link href="https://fonts.googleapis.com/css2?family=Sora:wght@400;500;600;700;800&family=JetBrains+Mono:wght@400;500;600&display=swap" rel="stylesheet">
```

## Contact

For custom brand requests, partnerships, or asset access, reach out via the [Prime Chain GitHub](https://github.com/PrimeNumbersLabs/prime-chain) or community channels.
