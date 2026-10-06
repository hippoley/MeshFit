# Exact-evidence prediction intervals

MeshFit distinguishes three different things:

1. **empirical range** — the minimum and maximum values already observed;
2. **uncertainty of the sample** — sample standard deviation and coefficient of variation;
3. **prediction interval** — a statistical interval for one future observation under the same exact execution identity.

These are not interchangeable.

## Scope

The first prediction interval implementation applies only to exact-match evidence:

- identical execution fingerprint,
- identical context length,
- identical concurrency,
- identical metric.

It does not transfer evidence across hardware, runtime versions, model artifacts, quantization, or topology.

## Method

With at least two exact-match observations, MeshFit emits a 95% Student-t next-observation interval:

```text
mean ± t(0.975, n - 1) × sample_stddev × sqrt(1 + 1/n)
```

The result is reported as:

- `confidence_level: 0.95`
- `lower`
- `upper`
- `method: student_t_next_observation`
- `sample_count`
- `lower_truncated_at_zero`

All current performance/resource metrics are non-negative. If the statistical lower bound crosses zero, MeshFit reports zero and sets `lower_truncated_at_zero: true`.

## Small samples

MeshFit does not hide small-sample uncertainty.

- one sample: no standard deviation, no CV, no prediction interval;
- two samples: the Student-t critical value is large, so the interval is deliberately wide;
- more repeated evidence narrows the interval only when the observed variation supports it.

The existing `confidence` field remains an evidence-volume heuristic. It is not the statistical confidence level of the prediction interval.

## Reality boundary

This interval describes repeatability under an exact observed execution identity. It is not a cross-hardware predictor and is not a substitute for real repeated Benchmark 001 runs.
