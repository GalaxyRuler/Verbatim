# Verbatim Website

This directory contains the public website source contract for `verbatim.alkulaib.io`.

The generated site is produced by:

```bash
bun run build:website
```

The generator fetches the latest GitHub release, verifies the expected installer assets, and writes a bilingual static site to `website/dist`.

Generated files are not committed. GitHub Actions deploys the generated output after a release is published.

Routes:

- `/` and `/en/` English
- `/ar/` Arabic with RTL layout
- `/release.json` machine-readable latest release metadata

Deployment target:

- Cloudflare Pages project: `verbatim-site`
- Custom domain: `verbatim.alkulaib.io`
