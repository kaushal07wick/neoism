---
sidebar_position: 2
title: Documentation
---

# Documentation

The docs site is built with Docusaurus and lives in `docs/`.

## Run Locally

```bash
cd docs
npm install
npm start
```

## Build

```bash
cd docs
npm run build
```

## Writing Rules

- Write for Neoism as its own product.
- Do not mass-replace product names without checking whether the behavior still exists.
- Prefer current source references over stale docs.
- Mark unstable behavior clearly.
- Keep user docs, agent docs, developer docs, and reference docs separate.

## Removed Legacy Content

The old inherited blog and product-specific legacy pages were removed from this docs site. If historical context is needed later, add a short migration note rather than restoring old marketing posts.