# Mersennet Website

The network home for **Mersennet** — the landing site for the whole network, served at [mersennet.com](https://mersennet.com).

A standalone [Next.js](https://nextjs.org) (App Router) + TypeScript + Tailwind CSS app, statically exported so it can be hosted on any static host (Vercel, Netlify, Cloudflare Pages, S3/CDN, Nginx).

> Documentation lives separately in [`../docs-site`](../docs-site) (Docusaurus) and is served at `docs.mersennet.com`.

## Develop

```bash
npm install
npm run dev      # http://localhost:3000
```

## Build

```bash
npm run build    # static export written to ./out
```

Because `next.config.mjs` sets `output: 'export'`, `npm run build` produces a fully static site in `out/`. Serve it locally with any static server, e.g.:

```bash
npx serve out
```

## Structure

```
app/
  layout.tsx                 # fonts (Sora / Instrument Serif / JetBrains Mono) + SEO metadata
  page.tsx                   # landing composition
  page.module.css            # cosmic design system
  globals.css                # reset + base
  site.ts                    # brand facts + external links (single source of truth)
  sitemap.ts / robots.ts     # generated sitemap.xml + robots.txt
  components/                # Nav, Hero, StatBand, Pillars, SelectiveDisclosure, Ecosystem, CtaBand, Footer, Icon, Backdrop
public/
  favicon.svg                # icon
  logo.svg                   # brand mark
  og.svg                     # OpenGraph / social card (1200x630)
```

## Editing content

- Brand facts (chain ID, RPC, symbol) and all outbound links: [`app/site.ts`](app/site.ts).
- Section copy lives in the matching component under [`app/components`](app/components).

## Notes

- The OpenGraph image is provided as `public/og.svg`. Some social platforms prefer PNG; export `og.svg` to a 1200x630 PNG and point `openGraph.images` / `twitter.images` in `app/layout.tsx` at it if you want maximum preview compatibility.
- External links (explorer, faucet) currently point at the testnet host IP. Update them in `app/site.ts` once DNS is in place.
