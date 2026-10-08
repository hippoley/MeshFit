# MeshFit Proof Explorer

A small, dependency-free reviewer interface for the public
`meshfit.benchmark-proof/v1` contract.

The explorer is deliberately not a benchmark marketing dashboard. It makes the
decision → frozen inputs → execution identity → raw evidence → claim gate →
independent verification chain inspectable.

## Local use

Open `index.html` in a browser.

Load a `benchmark-proof.yaml` or JSON file. Files are read locally in the
browser and are not uploaded.

The first version fully renders JSON receipts. For YAML receipts it extracts the
stable top-level publication fields and preserves the complete raw proof for
inspection. Rich YAML parsing is intentionally not implemented with an external
CDN dependency.

## Trust boundary

The explorer never declares a claim true. The authoritative check remains:

```bash
meshfit benchmark-verify-proof benchmark-proof.yaml \
  --root <materialized-kit> \
  --require-publishable
```

The built-in demo is a **contract demo**, not real Benchmark 001 evidence. It is
explicitly marked non-publishable.

## Publication

`.github/workflows/proof-explorer-pages.yml` publishes this directory through
GitHub Pages after it is merged to `main`, assuming Pages is enabled for the
repository.
