# Alaco website and documentation

This is the standalone website branch for [Alaco](https://github.com/ManiProjs/alaco). It intentionally has independent history from the compiler source branch.

- **Landing page:** Vite (`index.html`, `src/`)
- **Documentation:** VitePress (`docs-site/`)
- **Style:** white surfaces, no gradients, DM Sans for body text, Syne for headings

## Local development

Requires Node.js and npm.

```sh
npm install
npm run dev
```

Run the documentation site in a separate process:

```sh
npm run docs:dev
```

## Production build

The commands build the landing page first, then place VitePress output alongside it under `dist/docs-site/`:

```sh
npm run build
npm run docs:build
npm run preview
```

The configured GitHub Pages base paths assume the repository is published at `https://maniprojs.github.io/alaco/`. Update `base` in `vite.config.js` and `docs-site/.vitepress/config.mts` if the repository name or hosting path changes.

## Deploy

The GitHub Actions workflow in `.github/workflows/pages.yml` builds both sites and publishes `dist/` to GitHub Pages. In repository settings, select **GitHub Actions** as the Pages build and deployment source.

Documentation describes the compiler implementation as it exists today, including target differences and known limitations.
