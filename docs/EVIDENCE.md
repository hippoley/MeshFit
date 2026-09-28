# Evidence Model

MeshFit treats performance numbers as claims that require provenance.

## Rule 1 — no naked numbers

A number such as:

~~~text
decode = 69 tok/s
~~~

is not a MeshFit prediction by itself.

A valid prediction must answer:

- which hardware;
- which model artifact;
- which runtime;
- which placement;
- which context;
- which concurrency;
- how many observations;
- where those observations came from.

## BenchmarkRecord

A benchmark record binds observations to a concrete execution context:

~~~text
hardware fingerprint
model
runtime
placement
context
concurrency
metrics
provenance
~~~

## v0.2.0 matching policy

The first evidence-backed predictor is intentionally strict.

It only uses **exact matches** across:

- hardware fingerprint
- model id
- runtime
- placement kind
- context length
- concurrency

If no benchmark record matches exactly, MeshFit returns:

~~~text
status: unavailable
confidence: 0
~~~

It does not interpolate and it does not invent a fallback estimate.

## Prediction

An available prediction reports:

- mean
- min
- max
- sample count
- confidence
- evidence references
- explanation

The confidence score in v0.2.0 reflects only sample count.

It is not yet a statistical confidence interval and must not be interpreted as one.

## Synthetic examples

Files under `examples/` may contain synthetic values for testing schemas and CLI behavior.

Synthetic evidence must be clearly labeled and must never be presented as measured benchmark truth.

## Next evidence steps

### v0.2.1 — artifact identity

Add:

- model artifact hash
- runtime version
- driver version
- device identity
- quantization
- runtime flags

### v0.2.2 — repeated-run statistics

Add:

- warmup vs measured runs
- p50 / p95
- standard deviation
- outlier policy

### v0.2.3 — calibrated transfer

Only after exact-match evidence works:

- same GPU family
- same runtime family
- nearby context
- nearby concurrency
- topology similarity

Transferred predictions must reduce confidence and expose the transfer distance.

## Principle

> The planner gets better only when reality disagrees with it in a traceable way.
