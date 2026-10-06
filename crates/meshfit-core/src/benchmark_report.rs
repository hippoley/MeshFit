use serde::{Deserialize, Serialize};

use crate::benchmark::BenchmarkBundle;

const MAX_RUN_OBJECTIVE_CV_FOR_PUBLICATION: f64 = 0.20;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ComparisonObjective {
    P95TtftMs,
    MeanDecodeTokensPerSecond,
    MeanTotalMs,
}

impl ComparisonObjective {
    fn lower_is_better(self) -> bool {
        !matches!(self, Self::MeanDecodeTokensPerSecond)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BenchmarkCandidate {
    pub name: String,
    pub strategy: String,
    #[serde(default)]
    pub hourly_cost_usd: Option<f64>,
    pub bundles: Vec<BenchmarkBundle>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BenchmarkComparisonRequest {
    pub benchmark_id: String,
    pub meshfit_candidate: String,
    pub objective: ComparisonObjective,
    pub candidates: Vec<BenchmarkCandidate>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CandidateBenchmarkSummary {
    pub name: String,
    pub strategy: String,
    pub bundle_count: usize,
    pub sample_count: usize,
    pub p50_ttft_ms: f64,
    pub p95_ttft_ms: f64,
    pub mean_total_ms: f64,
    #[serde(default)]
    pub ttft_stddev_ms: Option<f64>,
    #[serde(default)]
    pub ttft_cv: Option<f64>,
    #[serde(default)]
    pub mean_decode_tokens_per_second: Option<f64>,
    #[serde(default)]
    pub decode_tokens_per_second_stddev: Option<f64>,
    #[serde(default)]
    pub mean_wave_throughput_tokens_per_second: Option<f64>,
    #[serde(default)]
    pub peak_vram_gb: Option<f64>,
    #[serde(default)]
    pub peak_ram_gb: Option<f64>,
    #[serde(default)]
    pub hourly_cost_usd: Option<f64>,
    #[serde(default)]
    pub estimated_cost_per_million_output_tokens_usd: Option<f64>,
    pub objective_value: f64,
    #[serde(default)]
    pub run_objective_mean: Option<f64>,
    #[serde(default)]
    pub run_objective_ci95_lower: Option<f64>,
    #[serde(default)]
    pub run_objective_ci95_upper: Option<f64>,
    #[serde(default)]
    pub run_objective_cv: Option<f64>,
    pub regret_fraction: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BenchmarkComparisonReport {
    pub benchmark_id: String,
    pub objective: ComparisonObjective,
    pub publishable: bool,
    pub evidence_status: String,
    pub oracle_candidate: String,
    pub meshfit_candidate: String,
    pub oracle_objective_value: f64,
    pub meshfit_objective_value: f64,
    pub meshfit_regret_fraction: f64,
    pub best_baseline_candidate: String,
    pub best_baseline_objective_value: f64,
    pub best_baseline_regret_fraction: f64,
    pub meshfit_improvement_vs_best_baseline_fraction: f64,
    #[serde(default)]
    pub meshfit_improvement_ci95_lower_fraction: Option<f64>,
    #[serde(default)]
    pub meshfit_improvement_ci95_upper_fraction: Option<f64>,
    #[serde(default)]
    pub regret_reduction_vs_best_baseline_fraction: Option<f64>,
    pub candidates: Vec<CandidateBenchmarkSummary>,
}

impl BenchmarkComparisonReport {
    pub fn to_markdown(&self) -> String {
        let regret_reduction = self
            .regret_reduction_vs_best_baseline_fraction
            .map(|value| format!("{:.1}%", value * 100.0))
            .unwrap_or_else(|| "n/a".into());

        let improvement_label = if self.meshfit_improvement_vs_best_baseline_fraction >= 0.0 {
            format!(
                "{:.1}% better",
                self.meshfit_improvement_vs_best_baseline_fraction * 100.0
            )
        } else {
            format!(
                "{:.1}% worse",
                self.meshfit_improvement_vs_best_baseline_fraction.abs() * 100.0
            )
        };
        let improvement_interval = match (
            self.meshfit_improvement_ci95_lower_fraction,
            self.meshfit_improvement_ci95_upper_fraction,
        ) {
            (Some(lower), Some(upper)) => format!(
                " · **run-level 95% interval:** [{:.1}%, {:.1}%]",
                lower * 100.0,
                upper * 100.0
            ),
            _ => String::new(),
        };

        let mut out = String::new();
        out.push_str(&format!("## {}\n\n", self.benchmark_id));
        if self.publishable {
            out.push_str(
                "**Evidence status:** PUBLISHABLE · independent repeated runs verified\n\n",
            );
            out.push_str(&format!(
                "**MeshFit vs best baseline ({}): {}**{} · **placement regret:** {:.1}% · **baseline regret:** {:.1}% · **regret reduction:** {}\n\n",
                self.best_baseline_candidate,
                improvement_label,
                improvement_interval,
                self.meshfit_regret_fraction * 100.0,
                self.best_baseline_regret_fraction * 100.0,
                regret_reduction
            ));
        } else {
            out.push_str(&format!(
                "**Evidence status:** NOT PUBLISHABLE · {}\n\n",
                self.evidence_status
            ));
            out.push_str(&format!(
                "Provisional comparison only. MeshFit vs best baseline ({}): {}{}. Do not publish this delta as a MeshFit performance claim.\n\n",
                self.best_baseline_candidate, improvement_label, improvement_interval
            ));
        }
        out.push_str(
            "| Candidate | Strategy | Runs | Samples | p95 TTFT | Run CV | Decode | Throughput | Cost / 1M output tok | Regret |\n",
        );
        out.push_str("|---|---|---:|---:|---:|---:|---:|---:|---:|---:|\n");

        for candidate in &self.candidates {
            let decode = candidate
                .mean_decode_tokens_per_second
                .map(|value| format!("{value:.2} tok/s"))
                .unwrap_or_else(|| "n/a".into());
            let throughput = candidate
                .mean_wave_throughput_tokens_per_second
                .map(|value| format!("{value:.2} tok/s"))
                .unwrap_or_else(|| "n/a".into());
            let cost = candidate
                .estimated_cost_per_million_output_tokens_usd
                .map(|value| format!("USD {value:.3}"))
                .unwrap_or_else(|| "n/a".into());
            let run_cv = candidate
                .run_objective_cv
                .map(|value| format!("{:.1}%", value * 100.0))
                .unwrap_or_else(|| "n/a".into());
            out.push_str(&format!(
                "| {} | {} | {} | {} | {:.2} ms | {} | {} | {} | {} | {:.1}% |\n",
                candidate.name,
                candidate.strategy,
                candidate.bundle_count,
                candidate.sample_count,
                candidate.p95_ttft_ms,
                run_cv,
                decode,
                throughput,
                cost,
                candidate.regret_fraction * 100.0
            ));
        }

        out.push_str(&format!(
            "\nObserved oracle: **{}** ({:?} = {:.3}).\n",
            self.oracle_candidate, self.objective, self.oracle_objective_value
        ));
        out
    }
}

pub fn compare_benchmarks(
    request: &BenchmarkComparisonRequest,
) -> Result<BenchmarkComparisonReport, String> {
    if request.candidates.len() < 2 {
        return Err("benchmark comparison requires at least two candidates".into());
    }

    let mut names = std::collections::HashSet::new();
    for candidate in &request.candidates {
        if !names.insert(candidate.name.as_str()) {
            return Err(format!(
                "duplicate benchmark candidate name '{}'",
                candidate.name
            ));
        }
    }

    let meshfit_count = request
        .candidates
        .iter()
        .filter(|candidate| candidate.name == request.meshfit_candidate)
        .count();
    if meshfit_count != 1 {
        return Err(format!(
            "meshfit_candidate '{}' must match exactly one candidate",
            request.meshfit_candidate
        ));
    }

    validate_comparable_workloads(&request.candidates)?;
    let (publishable, evidence_status) =
        evidence_qualification(&request.candidates, request.objective);

    let mut summaries = request
        .candidates
        .iter()
        .map(|candidate| summarize_candidate(candidate, request.objective))
        .collect::<Result<Vec<_>, _>>()?;

    let oracle_idx = summaries
        .iter()
        .enumerate()
        .min_by(|(_, left), (_, right)| {
            let ordering = left.objective_value.total_cmp(&right.objective_value);
            if request.objective.lower_is_better() {
                ordering
            } else {
                ordering.reverse()
            }
        })
        .map(|(idx, _)| idx)
        .ok_or_else(|| "benchmark comparison contains no candidates".to_string())?;

    let oracle_value = summaries[oracle_idx].objective_value;
    if oracle_value <= 0.0 {
        return Err("oracle objective value must be greater than zero".into());
    }

    for summary in &mut summaries {
        summary.regret_fraction =
            regret_fraction(summary.objective_value, oracle_value, request.objective)?;
    }

    let meshfit = summaries
        .iter()
        .find(|summary| summary.name == request.meshfit_candidate)
        .ok_or_else(|| "meshfit candidate disappeared during aggregation".to_string())?;

    let meshfit_objective_value = meshfit.objective_value;
    let meshfit_regret_fraction = meshfit.regret_fraction;

    let best_baseline = summaries
        .iter()
        .filter(|summary| summary.name != request.meshfit_candidate)
        .min_by(|left, right| {
            let ordering = left.objective_value.total_cmp(&right.objective_value);
            if request.objective.lower_is_better() {
                ordering
            } else {
                ordering.reverse()
            }
        })
        .ok_or_else(|| "comparison has no baseline candidate".to_string())?;

    let best_baseline_candidate = best_baseline.name.clone();
    let best_baseline_objective_value = best_baseline.objective_value;
    let best_baseline_regret = best_baseline.regret_fraction;
    let meshfit_improvement_vs_best_baseline_fraction = relative_improvement(
        meshfit_objective_value,
        best_baseline_objective_value,
        request.objective,
    )?;
    let improvement_interval = conservative_improvement_interval(
        meshfit.run_objective_ci95_lower,
        meshfit.run_objective_ci95_upper,
        best_baseline.run_objective_ci95_lower,
        best_baseline.run_objective_ci95_upper,
        request.objective,
    );

    let regret_reduction = if best_baseline_regret > 0.0 {
        Some((best_baseline_regret - meshfit_regret_fraction) / best_baseline_regret)
    } else {
        None
    };

    Ok(BenchmarkComparisonReport {
        benchmark_id: request.benchmark_id.clone(),
        objective: request.objective,
        publishable,
        evidence_status,
        oracle_candidate: summaries[oracle_idx].name.clone(),
        meshfit_candidate: request.meshfit_candidate.clone(),
        oracle_objective_value: oracle_value,
        meshfit_objective_value,
        meshfit_regret_fraction,
        best_baseline_candidate,
        best_baseline_objective_value,
        best_baseline_regret_fraction: best_baseline_regret,
        meshfit_improvement_vs_best_baseline_fraction,
        meshfit_improvement_ci95_lower_fraction: improvement_interval.map(|(lower, _)| lower),
        meshfit_improvement_ci95_upper_fraction: improvement_interval.map(|(_, upper)| upper),
        regret_reduction_vs_best_baseline_fraction: regret_reduction,
        candidates: summaries,
    })
}

fn evidence_qualification(
    candidates: &[BenchmarkCandidate],
    objective: ComparisonObjective,
) -> (bool, String) {
    let mut all_ids = std::collections::HashSet::new();
    let mut compared_plan_ids = std::collections::HashSet::new();

    for candidate in candidates {
        let Some(first_bundle) = candidate.bundles.first() else {
            return (
                false,
                format!(
                    "candidate '{}' contains no benchmark bundles",
                    candidate.name
                ),
            );
        };
        let plan_id = first_bundle.request.executable.source_plan_id.as_str();

        if !compared_plan_ids.insert(plan_id) {
            return (
                false,
                format!(
                    "plan_id '{}' is reused across candidates; Benchmark 001 strategies must measure distinct placements",
                    plan_id
                ),
            );
        }

        let sample_count = candidate
            .bundles
            .iter()
            .map(|bundle| bundle.measurements.len())
            .sum::<usize>();

        if candidate.bundles.len() < 2 {
            return (
                false,
                format!(
                    "candidate '{}' has {} independent bundle(s); Benchmark 001 requires at least 2",
                    candidate.name,
                    candidate.bundles.len()
                ),
            );
        }

        if sample_count < 20 {
            return (
                false,
                format!(
                    "candidate '{}' has {} measured samples; p95 publication requires at least 20",
                    candidate.name, sample_count
                ),
            );
        }

        let run_values = candidate
            .bundles
            .iter()
            .filter_map(|bundle| bundle_objective_value(bundle, objective))
            .collect::<Vec<_>>();
        let Some(run_cv) = coefficient_of_variation(&run_values) else {
            return (
                false,
                format!(
                    "candidate '{}' does not have enough valid independent run-level objective values",
                    candidate.name
                ),
            );
        };
        if run_cv > MAX_RUN_OBJECTIVE_CV_FOR_PUBLICATION {
            return (
                false,
                format!(
                    "candidate '{}' is unstable across independent runs: objective CV {:.1}% exceeds the {:.1}% publication limit",
                    candidate.name,
                    run_cv * 100.0,
                    MAX_RUN_OBJECTIVE_CV_FOR_PUBLICATION * 100.0
                ),
            );
        }

        let mut candidate_ids = std::collections::HashSet::new();
        for bundle in &candidate.bundles {
            if bundle.request.executable.source_plan_id != plan_id {
                return (
                    false,
                    format!(
                        "candidate '{}' mixes plan_id '{}' with '{}'; all repeated bundles for one strategy must execute the same plan",
                        candidate.name,
                        plan_id,
                        bundle.request.executable.source_plan_id
                    ),
                );
            }
            if bundle.provenance.source != "meshfit-local-runner" {
                return (
                    false,
                    format!(
                        "benchmark_id '{}' uses provenance source '{}'; publishable Benchmark 001 evidence must come from meshfit-local-runner",
                        bundle.benchmark_id, bundle.provenance.source
                    ),
                );
            }
            if bundle.provenance.captured_at.is_none() {
                return (
                    false,
                    format!(
                        "benchmark_id '{}' has no capture timestamp",
                        bundle.benchmark_id
                    ),
                );
            }
            if bundle.provenance.commit.is_none() {
                return (
                    false,
                    format!(
                        "benchmark_id '{}' has no MeshFit source commit; publishable evidence must bind measurements to source",
                        bundle.benchmark_id
                    ),
                );
            }
            if !candidate_ids.insert(bundle.benchmark_id.as_str()) {
                return (
                    false,
                    format!(
                        "candidate '{}' repeats benchmark_id '{}'; duplicate bundles do not count as independent runs",
                        candidate.name, bundle.benchmark_id
                    ),
                );
            }
            if !all_ids.insert(bundle.benchmark_id.as_str()) {
                return (
                    false,
                    format!(
                        "benchmark_id '{}' is reused across candidates; each compared run must be independent",
                        bundle.benchmark_id
                    ),
                );
            }
        }
    }

    (
        true,
        "independent repeated benchmark bundles verified".into(),
    )
}

fn validate_comparable_workloads(candidates: &[BenchmarkCandidate]) -> Result<(), String> {
    let reference = candidates
        .first()
        .and_then(|candidate| candidate.bundles.first())
        .ok_or_else(|| "first comparison candidate contains no benchmark bundles".to_string())?;

    reference.validate()?;

    for candidate in candidates {
        if candidate.bundles.is_empty() {
            return Err(format!(
                "candidate '{}' contains no benchmark bundles",
                candidate.name
            ));
        }

        for bundle in &candidate.bundles {
            bundle.validate()?;
            if bundle.request.identity.model != reference.request.identity.model {
                return Err(format!(
                    "candidate '{}' uses a different model artifact identity",
                    candidate.name
                ));
            }
            if bundle.request.context_tokens != reference.request.context_tokens
                || bundle.request.concurrency != reference.request.concurrency
                || bundle.request.config != reference.request.config
            {
                return Err(format!(
                    "candidate '{}' is not comparable: context, concurrency, or BenchmarkConfig differ",
                    candidate.name
                ));
            }
        }
    }

    Ok(())
}

fn summarize_candidate(
    candidate: &BenchmarkCandidate,
    objective: ComparisonObjective,
) -> Result<CandidateBenchmarkSummary, String> {
    let measurements = candidate
        .bundles
        .iter()
        .flat_map(|bundle| bundle.measurements.iter())
        .collect::<Vec<_>>();

    if measurements.is_empty() {
        return Err(format!(
            "candidate '{}' has no measurements",
            candidate.name
        ));
    }

    let ttft = measurements
        .iter()
        .map(|measurement| measurement.ttft_ms)
        .collect::<Vec<_>>();
    let total = measurements
        .iter()
        .map(|measurement| measurement.total_ms)
        .collect::<Vec<_>>();
    let decode = measurements
        .iter()
        .filter_map(|measurement| measurement.decode_tokens_per_second())
        .collect::<Vec<_>>();

    let mean_decode = mean(&decode);
    let ttft_stddev_ms = standard_deviation(&ttft);
    let ttft_cv = coefficient_of_variation(&ttft);
    let decode_tokens_per_second_stddev = standard_deviation(&decode);
    let run_objective_values = candidate
        .bundles
        .iter()
        .filter_map(|bundle| bundle_objective_value(bundle, objective))
        .collect::<Vec<_>>();
    let run_objective_cv = coefficient_of_variation(&run_objective_values);
    let run_objective_mean = mean(&run_objective_values);
    let run_objective_interval = log_student_t_interval_95(&run_objective_values);
    let wave_throughput = candidate
        .bundles
        .iter()
        .flat_map(|bundle| bundle.waves.iter())
        .filter_map(|wave| wave.throughput_tokens_per_second())
        .collect::<Vec<_>>();
    let mean_wave_throughput = mean(&wave_throughput);
    let p50_ttft_ms = percentile(&ttft, 0.50)?;
    let p95_ttft_ms = percentile(&ttft, 0.95)?;
    let mean_total_ms = mean(&total).ok_or_else(|| "missing total duration".to_string())?;
    let peak_vram_gb = candidate
        .bundles
        .iter()
        .flat_map(|bundle| bundle.waves.iter())
        .filter_map(|wave| wave.peak_vram_gb)
        .max_by(f64::total_cmp);
    let peak_ram_gb = candidate
        .bundles
        .iter()
        .flat_map(|bundle| bundle.waves.iter())
        .filter_map(|wave| wave.peak_ram_gb)
        .max_by(f64::total_cmp);

    let objective_value = match objective {
        ComparisonObjective::P95TtftMs => p95_ttft_ms,
        ComparisonObjective::MeanDecodeTokensPerSecond => mean_decode.ok_or_else(|| {
            format!(
                "candidate '{}' has no decode token-rate observations",
                candidate.name
            )
        })?,
        ComparisonObjective::MeanTotalMs => mean_total_ms,
    };

    let estimated_cost = candidate.hourly_cost_usd.and_then(|hourly_cost| {
        let output_tokens = candidate
            .bundles
            .iter()
            .flat_map(|bundle| bundle.waves.iter())
            .filter_map(|wave| wave.output_tokens)
            .map(u64::from)
            .sum::<u64>();
        let total_seconds = candidate
            .bundles
            .iter()
            .flat_map(|bundle| bundle.waves.iter())
            .map(|wave| wave.total_ms / 1000.0)
            .sum::<f64>();

        if output_tokens == 0 || total_seconds <= 0.0 {
            None
        } else {
            let output_tps = output_tokens as f64 / total_seconds;
            Some(hourly_cost / (output_tps * 3600.0) * 1_000_000.0)
        }
    });

    Ok(CandidateBenchmarkSummary {
        name: candidate.name.clone(),
        strategy: candidate.strategy.clone(),
        bundle_count: candidate.bundles.len(),
        sample_count: measurements.len(),
        p50_ttft_ms,
        p95_ttft_ms,
        mean_total_ms,
        ttft_stddev_ms,
        ttft_cv,
        mean_decode_tokens_per_second: mean_decode,
        decode_tokens_per_second_stddev,
        mean_wave_throughput_tokens_per_second: mean_wave_throughput,
        peak_vram_gb,
        peak_ram_gb,
        hourly_cost_usd: candidate.hourly_cost_usd,
        estimated_cost_per_million_output_tokens_usd: estimated_cost,
        objective_value,
        run_objective_mean,
        run_objective_ci95_lower: run_objective_interval.map(|(lower, _)| lower),
        run_objective_ci95_upper: run_objective_interval.map(|(_, upper)| upper),
        run_objective_cv,
        regret_fraction: 0.0,
    })
}

fn bundle_objective_value(bundle: &BenchmarkBundle, objective: ComparisonObjective) -> Option<f64> {
    match objective {
        ComparisonObjective::P95TtftMs => {
            let values = bundle
                .measurements
                .iter()
                .map(|measurement| measurement.ttft_ms)
                .collect::<Vec<_>>();
            percentile(&values, 0.95).ok()
        }
        ComparisonObjective::MeanDecodeTokensPerSecond => {
            let values = bundle
                .measurements
                .iter()
                .filter_map(|measurement| measurement.decode_tokens_per_second())
                .collect::<Vec<_>>();
            mean(&values)
        }
        ComparisonObjective::MeanTotalMs => {
            let values = bundle
                .measurements
                .iter()
                .map(|measurement| measurement.total_ms)
                .collect::<Vec<_>>();
            mean(&values)
        }
    }
}

fn standard_deviation(values: &[f64]) -> Option<f64> {
    if values.len() < 2 {
        return None;
    }

    let mean = mean(values)?;
    let variance = values
        .iter()
        .map(|value| {
            let delta = value - mean;
            delta * delta
        })
        .sum::<f64>()
        / (values.len() - 1) as f64;
    Some(variance.sqrt())
}

fn coefficient_of_variation(values: &[f64]) -> Option<f64> {
    let mean = mean(values)?;
    if mean == 0.0 {
        return None;
    }

    Some(standard_deviation(values)? / mean.abs())
}

fn student_t_critical_95(degrees_of_freedom: usize) -> f64 {
    const T: [f64; 30] = [
        12.706, 4.303, 3.182, 2.776, 2.571, 2.447, 2.365, 2.306, 2.262, 2.228, 2.201, 2.179, 2.160,
        2.145, 2.131, 2.120, 2.110, 2.101, 2.093, 2.086, 2.080, 2.074, 2.069, 2.064, 2.060, 2.056,
        2.052, 2.048, 2.045, 2.042,
    ];

    if degrees_of_freedom == 0 {
        f64::INFINITY
    } else if degrees_of_freedom <= T.len() {
        T[degrees_of_freedom - 1]
    } else {
        2.042
    }
}

fn log_student_t_interval_95(values: &[f64]) -> Option<(f64, f64)> {
    if values.len() < 2
        || values
            .iter()
            .any(|value| *value <= 0.0 || !value.is_finite())
    {
        return None;
    }

    let logs = values.iter().map(|value| value.ln()).collect::<Vec<_>>();
    let log_mean = mean(&logs)?;
    let log_stddev = standard_deviation(&logs)?;
    let critical = student_t_critical_95(logs.len() - 1);
    let margin = critical * log_stddev / (logs.len() as f64).sqrt();

    Some(((log_mean - margin).exp(), (log_mean + margin).exp()))
}

fn conservative_improvement_interval(
    meshfit_lower: Option<f64>,
    meshfit_upper: Option<f64>,
    baseline_lower: Option<f64>,
    baseline_upper: Option<f64>,
    objective: ComparisonObjective,
) -> Option<(f64, f64)> {
    let (meshfit_lower, meshfit_upper, baseline_lower, baseline_upper) = (
        meshfit_lower?,
        meshfit_upper?,
        baseline_lower?,
        baseline_upper?,
    );

    if meshfit_lower <= 0.0
        || meshfit_upper <= 0.0
        || baseline_lower <= 0.0
        || baseline_upper <= 0.0
    {
        return None;
    }

    let (lower, upper) = if objective.lower_is_better() {
        (
            1.0 - meshfit_upper / baseline_lower,
            1.0 - meshfit_lower / baseline_upper,
        )
    } else {
        (
            meshfit_lower / baseline_upper - 1.0,
            meshfit_upper / baseline_lower - 1.0,
        )
    };

    Some((lower.min(upper), lower.max(upper)))
}

fn percentile(values: &[f64], quantile: f64) -> Result<f64, String> {
    if values.is_empty() {
        return Err("cannot compute percentile over zero samples".into());
    }
    if !(0.0..=1.0).contains(&quantile) {
        return Err("percentile quantile must be between zero and one".into());
    }

    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let rank = ((quantile * sorted.len() as f64).ceil() as usize)
        .saturating_sub(1)
        .min(sorted.len() - 1);
    Ok(sorted[rank])
}

fn mean(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        None
    } else {
        Some(values.iter().sum::<f64>() / values.len() as f64)
    }
}

fn relative_improvement(
    meshfit: f64,
    baseline: f64,
    objective: ComparisonObjective,
) -> Result<f64, String> {
    if meshfit <= 0.0 || baseline <= 0.0 {
        return Err("comparison objective values must be greater than zero".into());
    }

    Ok(if objective.lower_is_better() {
        (baseline - meshfit) / baseline
    } else {
        (meshfit - baseline) / baseline
    })
}

fn regret_fraction(value: f64, oracle: f64, objective: ComparisonObjective) -> Result<f64, String> {
    if value <= 0.0 || oracle <= 0.0 {
        return Err("objective values must be greater than zero".into());
    }

    let regret = if objective.lower_is_better() {
        (value - oracle) / oracle
    } else {
        (oracle - value) / oracle
    };

    Ok(regret.max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_candidate_is_not_publishable() {
        let candidate = BenchmarkCandidate {
            name: "meshfit".into(),
            strategy: "test".into(),
            hourly_cost_usd: None,
            bundles: vec![],
        };
        let (publishable, status) =
            evidence_qualification(&[candidate], ComparisonObjective::P95TtftMs);
        assert!(!publishable);
        assert!(status.contains("contains no benchmark bundles"));
    }

    #[test]
    fn markdown_report_is_readme_ready() {
        let report = BenchmarkComparisonReport {
            benchmark_id: "benchmark-001".into(),
            objective: ComparisonObjective::P95TtftMs,
            publishable: true,
            evidence_status: "independent repeated benchmark bundles verified".into(),
            oracle_candidate: "meshfit".into(),
            meshfit_candidate: "meshfit".into(),
            oracle_objective_value: 100.0,
            meshfit_objective_value: 100.0,
            meshfit_regret_fraction: 0.0,
            best_baseline_candidate: "heuristic".into(),
            best_baseline_objective_value: 125.0,
            best_baseline_regret_fraction: 0.25,
            meshfit_improvement_vs_best_baseline_fraction: 0.20,
            meshfit_improvement_ci95_lower_fraction: Some(0.10),
            meshfit_improvement_ci95_upper_fraction: Some(0.30),
            regret_reduction_vs_best_baseline_fraction: Some(1.0),
            candidates: vec![CandidateBenchmarkSummary {
                name: "meshfit".into(),
                strategy: "topology-aware".into(),
                bundle_count: 2,
                sample_count: 20,
                p50_ttft_ms: 90.0,
                p95_ttft_ms: 100.0,
                mean_total_ms: 500.0,
                ttft_stddev_ms: Some(4.0),
                ttft_cv: Some(0.04),
                mean_decode_tokens_per_second: Some(40.0),
                decode_tokens_per_second_stddev: Some(2.0),
                mean_wave_throughput_tokens_per_second: Some(120.0),
                peak_vram_gb: Some(20.0),
                peak_ram_gb: Some(10.0),
                hourly_cost_usd: Some(1.0),
                estimated_cost_per_million_output_tokens_usd: Some(6.944),
                objective_value: 100.0,
                run_objective_mean: Some(100.0),
                run_objective_ci95_lower: Some(90.0),
                run_objective_ci95_upper: Some(110.0),
                run_objective_cv: Some(0.05),
                regret_fraction: 0.0,
            }],
        };

        let markdown = report.to_markdown();
        assert!(markdown.contains("MeshFit vs best baseline (heuristic): 20.0% better"));
        assert!(markdown.contains("run-level 95% interval: [10.0%, 30.0%]"));
        assert!(markdown.contains("| meshfit | topology-aware | 2 | 20 |"));
        assert!(markdown.contains("Observed oracle: **meshfit**"));
    }

    #[test]
    fn nearest_rank_percentile_is_deterministic() {
        let values = vec![10.0, 20.0, 30.0, 40.0, 50.0];
        assert_eq!(percentile(&values, 0.50).unwrap(), 30.0);
        assert_eq!(percentile(&values, 0.95).unwrap(), 50.0);
    }

    #[test]
    fn coefficient_of_variation_tracks_repeatability() {
        let stable = vec![100.0, 102.0, 98.0];
        let unstable = vec![100.0, 150.0, 60.0];

        assert!(coefficient_of_variation(&stable).unwrap() < 0.03);
        assert!(coefficient_of_variation(&unstable).unwrap() > 0.20);
    }

    #[test]
    fn publication_stability_threshold_is_explicit() {
        assert_eq!(MAX_RUN_OBJECTIVE_CV_FOR_PUBLICATION, 0.20);
    }

    #[test]
    fn student_t_fallback_remains_conservative_beyond_table() {
        assert_eq!(student_t_critical_95(30), 2.042);
        assert_eq!(student_t_critical_95(31), 2.042);
        assert_eq!(student_t_critical_95(100), 2.042);
    }

    #[test]
    fn log_student_t_interval_is_positive_and_widens_for_two_runs() {
        let interval = log_student_t_interval_95(&[100.0, 110.0]).unwrap();
        assert!(interval.0 > 0.0);
        assert!(interval.0 < 100.0);
        assert!(interval.1 > 110.0);
    }

    #[test]
    fn conservative_improvement_interval_uses_worst_case_bounds() {
        let lower_better = conservative_improvement_interval(
            Some(80.0),
            Some(100.0),
            Some(100.0),
            Some(120.0),
            ComparisonObjective::P95TtftMs,
        )
        .unwrap();
        assert!((lower_better.0 - 0.0).abs() < 1e-12);
        assert!((lower_better.1 - (1.0 - 80.0 / 120.0)).abs() < 1e-12);

        let higher_better = conservative_improvement_interval(
            Some(100.0),
            Some(120.0),
            Some(80.0),
            Some(100.0),
            ComparisonObjective::MeanDecodeTokensPerSecond,
        )
        .unwrap();
        assert!((higher_better.0 - 0.0).abs() < 1e-12);
        assert!((higher_better.1 - 0.5).abs() < 1e-12);
    }

    #[test]
    fn relative_improvement_handles_lower_and_higher_is_better_metrics() {
        assert_eq!(
            relative_improvement(80.0, 100.0, ComparisonObjective::P95TtftMs).unwrap(),
            0.20
        );
        assert_eq!(
            relative_improvement(120.0, 100.0, ComparisonObjective::MeanDecodeTokensPerSecond)
                .unwrap(),
            0.20
        );
        assert_eq!(
            relative_improvement(120.0, 100.0, ComparisonObjective::P95TtftMs).unwrap(),
            -0.20
        );
    }

    #[test]
    fn regret_handles_lower_and_higher_is_better_metrics() {
        assert_eq!(
            regret_fraction(125.0, 100.0, ComparisonObjective::P95TtftMs).unwrap(),
            0.25
        );
        assert_eq!(
            regret_fraction(80.0, 100.0, ComparisonObjective::MeanDecodeTokensPerSecond).unwrap(),
            0.20
        );
    }
}
