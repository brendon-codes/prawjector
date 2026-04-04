# Architecture

## Overview

`docs/` is a single-page static marketing site for the `prawjector` CLI in this repository. It is generated with Astro and published to GitHub Pages. The page has section navigation, product documentation, and an embedded demo video.

## Stack

- Bun is the package manager and script runner. `package.json` pins `bun@1.4.4`. `engines.node` pins Node.js `24.x`.
- Astro `7.4.7` is the static site generator, with `output: "static"`.
- TypeScript `6.0.3` with `@astrojs/check` `0.9.12` provides type checking for `.astro` and `.ts` files. TypeScript 7 is not used because `@astrojs/check` requires TypeScript `^5 || ^6`.
- Biome `2.6.5` handles linting and formatting, including `.astro` files through `html.experimentalFullSupportEnabled`. Markdown files are excluded.
- Tailwind CSS `4.3.4` is loaded through `@tailwindcss/vite`.
- Fonts are self-hosted at build time with the Astro Fonts API using the Google provider.
- No UI framework runtime is shipped. The only client script is the hero video play overlay.

## Folder Structure

- `astro.config.ts`
  Site URL, base path, font definitions, and Vite plugins.
- `src/pages/index.astro`
  The only route. Renders the homepage.
- `src/layouts/base_layout.astro`
  Document shell, head metadata, fonts, global CSS, and footer.
- `src/components`
  Presentational Astro components such as the header, hero stage, section wrapper, command panels, and logo.
- `src/scripts/hero_video_play_overlay.ts`
  Client script bundled by Astro that shows the play overlay while the hero video is paused.
- `src/data/site_content.ts`
  Static site copy and section content.
- `src/lib`
  Non-visual helpers and metadata: `site_metadata.ts`, `join_class_names.ts`, `split_inline_code.ts`, `with_base_path.ts`.
- `src/styles/globals.css`
  Global styles.
- `public/media/usage`
  Demo MP4 and poster image, copied verbatim into the build output.

## Import Conventions

- Source imports use the `#/*` alias defined in `tsconfig.json`.
- Imports include file extensions.
- Imports are kept at the top of each module.

## Rendering Model

- Astro renders everything to static HTML at build time.
- Content is assembled from `src/data/site_content.ts`.
- There are no API routes, server endpoints, or runtime data fetching.
- Inline backtick spans in content strings are rendered as `<code class="inline_code">` by `inline_text.astro`.

## Styling

- Global styling lives in `src/styles/globals.css`, which starts with `@import "tailwindcss";`.
- The visual system relies on CSS custom properties for colors, spacing, motion, and typography.
- Font CSS variables `--font_body`, `--font_mono`, and `--font_display` come from the Astro Fonts API.
- The design uses hard edges, thin borders, dark surfaces, and restrained accent color.

## Content And Media Strategy

- Marketing copy is centralized in `src/data/site_content.ts`.
- Public asset URLs are built with `with_base_path()` so the site keeps working if `base` changes.
- The hero video is silent, so it is marked `muted` and has no captions track.
- CLI usage copy should be checked against `../src/main.rs`, `../src/config.rs`, and rendered clap help before changing command, argument, or picker-control text.

## SEO Strategy

- `src/lib/site_metadata.ts` holds title, description, Open Graph, Twitter card, robots, and theme values.
- `base_layout.astro` renders those tags plus a canonical URL derived from `site` and `base` in `astro.config.ts`.

## Accessibility Strategy

- The page includes a skip link to the main content.
- The layout uses semantic landmarks: `header`, `nav`, `main`, `footer`, and `section`.
- The hero video has visible controls, a poster image, and an `aria-label`.
- Global CSS defines visible `:focus-visible` treatment and a `prefers-reduced-motion: reduce` branch.

## Quality Gates

- `bun run lint` runs `biome check .`.
- `bun run typecheck` runs `astro check`.
- `bun run check` runs lint and typecheck together.
- `bun run build` writes the static site to `dist/`.
- `bun audit` reports dependency vulnerabilities.

## Deployment

- `.github/workflows/pages.yml` at the repository root runs on pushes and pull requests to `main` that touch `docs/`, plus manual dispatch.
- The `check` job runs `bun ci`, lint, and typecheck.
- The `build` job uses `withastro/action` with `path: docs` to build and upload the Pages artifact.
- The `deploy` job runs `actions/deploy-pages` for non-pull-request events.
- The site is served at the default project Pages URL `https://brendon-codes.github.io/prawjector/`. `astro.config.ts` sets `site` to `https://brendon-codes.github.io` and `base` to `/prawjector`. No custom domain is configured.
