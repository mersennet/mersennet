# Mersennet Documentation

Documentation site for [Mersennet](https://mersennet.com), built with
[Astro Starlight](https://starlight.astro.build). Served at
[docs.mersennet.com](https://docs.mersennet.com).

## Development

```bash
npm install
npm run dev        # local dev server at http://localhost:4321
npm run build      # static build to dist/
npm run preview    # serve the production build locally
```

## Layout

- `src/content/docs/` — all documentation pages (Markdown/MDX)
- `astro.config.mjs` — site config, sidebar, theme integrations
- `src/styles/custom.css` — Mersennet brand theme
- `src/components/` — interactive islands (e.g. Add-to-MetaMask)
- `public/` — static assets (logo, favicon, llms.txt)

Full-text search is powered by [Pagefind](https://pagefind.app) and built
automatically during `npm run build`.

The canonical home of this site is
[mersennet/docs](https://github.com/mersennet/docs); it is mirrored in the
[mersennet/mersennet](https://github.com/mersennet/mersennet) monorepo under
`docs-site/`.
