# Prawjector Site

Static marketing site for Prawjector, built with [Astro](https://astro.build) and published to GitHub Pages at <https://brendon-codes.github.io/prawjector/>.

## Prerequisites

- Bun `1.4.4` as declared in `package.json`
- Node.js `24.x`

## Install

```bash
bun install
```

## Local Development

```bash
bun run dev
```

The Astro dev server runs at `http://localhost:4321/prawjector/`.

## Checks

```bash
bun run lint
bun run typecheck
bun run check
bun run build
bun run preview
bun run format
```

## Deployment

`.github/workflows/pages.yml` at the repository root lints, typechecks, and builds this directory on pushes and pull requests that touch `docs/`. Pushes to `main` deploy the built `dist/` output to GitHub Pages.

The repository Pages source is set to GitHub Actions. The site uses the default project Pages domain, so `astro.config.ts` sets `base` to `/prawjector`.

## Architecture

See [ARCHITECTURE.md](./ARCHITECTURE.md).
