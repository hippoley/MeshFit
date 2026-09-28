# Evidence Model

MeshFit treats performance numbers as claims that require provenance.

## Rule 1 — no naked numbers

A number such as:

~~~text
decode = 69 tok/s
~~~

is not a MeshFit prediction by itself.

A valid prediction must answer:

- which exact hardware and driver;
- which model artifact and quantization;
- which runtime version and flags;
- which topology;
- which placement;
- which context and concurrency;
- how many observations;
- where those observations came from.

## ExecutionIdentity

v0.2.1 replaces hand-written labels such as:

~~~text
2xh100-nvlink
~~~

with structured identity:

~~~text
HardwareIdentity
+ ModelArtifactIdentity
+ RuntimeIdentity
+ TopologyIdentity
+ PlacementKind
        ↓
stable SHA-256 execution fingerprint
~~~

The fingerprint is normalized so device order, runtime-flag order, and topology-link order do not create false identity changes.

Important:

- device model / total memory / driver are identity;
- current free VRAM is dynamic state and is **not** identity;
- runtime version changes identity;
- quantization changes identity;
- model artifact hash changes identity;
- topology changes identity.

This prevents benchmark evidence from silently crossing execution boundaries.

## BenchmarkRecord

A benchmark record binds observations to:

~~~text
ExecutionIdentity
context
concurrency
metrics
provenance
~~~

## Exact matching policy

The current predictor only uses exact matches across:

- execution fingerprint
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

- execution fingerprint
- mean
- min
- max
- sample count
- confidence
- evidence references
- explanation

The current confidence score reflects sample count only.

It is not yet a statistical confidence interval.

## Synthetic examples

Files under `examples/` may contain synthetic values for schema and CLI testing.

Synthetic evidence must be clearly labeled and must never be presented as measured benchmark truth.

## Next steps

### Repeated-run statistics

Add:

- warmup vs measured runs
- p50 / p95
- standard deviation
- outlier policy

### Real capture

Capture identity directly from:

- discovered devices
- runtime binary/version
- driver
- model artifact
- runtime flags
- measured topology

### Calibrated transfer

Only after exact-match evidence and real benchmark capture work:

- same accelerator family
- same runtime family
- nearby context
- nearby concurrency
- topology similarity

Transferred predictions must reduce confidence and expose transfer distance.

## Principle

> The planner gets better only when reality disagrees with it in a traceable way.
