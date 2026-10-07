# Reality Campaign 001 — compute proxy

Benchmark 001 names two baselines by compute rank: `single-best-node` and
`max-aggregate-compute`. Automatic discovery intentionally does not invent a
compute score, so real discovered accelerators currently carry
`relative_compute: 0.0` and the readiness gate refuses publication.

Use `tools/compute_proxy.py` only to supply that missing ordering signal.

## Contract

Run the same command on every CUDA accelerator that may participate in the
experiment:

```bash
python3 tools/compute_proxy.py \
  --device 0 \
  --dtype bf16 \
  --size 8192 \
  --warmup 5 \
  --repeats 20 \
  --max-cv 0.10 \
  --require-stable \
  > compute-proxy-gpu0.json
```

For multi-GPU hosts, run it once per physical GPU by changing `--device`.
Do not run multiple probes concurrently on the same host.

`--device` is a PyTorch logical CUDA index. If `CUDA_VISIBLE_DEVICES` is set,
that index may be remapped relative to `nvidia-smi`. The evidence JSON records
`CUDA_VISIBLE_DEVICES` and, when the PyTorch runtime exposes them, the device
UUID / PCI bus identifier. Use those fields to bind the proxy evidence to the
physical discovery record; never assume logical `cuda:0` means physical GPU 0.

The JSON emits raw timings, per-sample TFLOPS, median/mean TFLOPS, sample
standard deviation, CV, and a `stable` result. With `--require-stable`, the
command exits non-zero when CV exceeds `--max-cv`. Copy
`relative_compute` into the corresponding accelerator entry of the discovery
YAML **without changing any other discovered identity field**, and retain the
JSON next to the discovery/snapshot inputs.

For a local TP placement, MeshFit already sums the selected devices'
`relative_compute`, so the same per-device proxy naturally produces an
aggregate-compute baseline.

## Rules

- Use one precision across the whole campaign. The default is BF16.
- If any compared GPU cannot run that precision, choose another common precision
  and rerun **every** node. Never mix BF16 and FP16 scores.
- Keep matrix size, warmup count, repeat count, and `max_cv` identical.
- Use `--require-stable` for Reality Campaign evidence. Do not copy a score
  from an output where `stable: false`.
- Treat the score only as a baseline ordering proxy.
- Do not present the GEMM TFLOPS result as inference throughput or as a MeshFit
  performance result.
- Retain the output JSON as Reality Campaign evidence.

After filling non-zero scores, rerun:

```bash
meshfit benchmark-candidates cluster.yaml target.yaml <meshfit-plan-id> --require-distinct
meshfit benchmark-kit cluster.yaml target.yaml <meshfit-plan-id> <model-path> model-identity.yaml --require-ready --write-dir benchmark-001
```

The paid benchmark run should not start until both commands pass.


## End-to-end real-hardware campaign

The software control plane is no longer the bottleneck. The next publishable
result must come from real heterogeneous machines, real network measurements,
real runtime execution, repeated raw bundles, and the existing finalization
gate.

Use one campaign ID and keep every generated file under one evidence directory.
Do not edit generated BenchmarkBundle files by hand.

### 1. Discover every real host

On each machine, give MeshFit a stable logical node ID and capture discovery:

```bash
export MESHFIT_NODE_ID=node-a
meshfit discover > node-a.raw.yaml
cp node-a.raw.yaml node-a.yaml
```

Repeat for every host. Treat `node-*.raw.yaml` as immutable machine-derived
evidence. Insert `relative_compute` only into the working `node-*.yaml`
copies used to build the coordinator snapshot. Never edit the raw discovery
artifacts in place.

The hardware identity, accelerator inventory, local topology, driver metadata,
and runtime discovery fields in the working copies must remain byte-for-byte
derived from their raw counterparts; only the explicitly documented
`relative_compute` field may be added or changed.

### 2. Measure the compute-ordering proxy

Run the fixed proxy once per candidate GPU using exactly the same dtype, matrix
size, warmup count, and repeat count across the campaign:

```bash
python3 tools/compute_proxy.py \
  --device 0 \
  --dtype bf16 \
  --size 8192 \
  --warmup 5 \
  --repeats 20 \
  --max-cv 0.10 \
  --require-stable \
  > node-a-gpu0-compute-proxy.json
```

Copy only the resulting `relative_compute` value into the matching accelerator
entry in that host's working discovery YAML (for example `node-a.yaml`).
Retain the corresponding immutable `node-a.raw.yaml` and every proxy JSON
beside the snapshot inputs. These values rank compute baselines; they are not
inference throughput measurements.

### 3. Measure every cross-host link used by candidate plans

For every host pair that may participate in a distributed placement, capture a
real peer probe from the experiment environment. Preserve the probe YAML,
including source, timestamp, RTT, both directional bandwidth measurements, and
the conservative effective bandwidth.

`meshfit probe --bandwidth` measures both client-to-peer and peer-to-client
throughput. Because the current planner models a node link as an undirected
edge, `bandwidth_gbps` is the slower of the two measured directions. If either
direction cannot be measured, effective bandwidth is omitted so readiness fails
closed instead of treating one direction as representative of both.

Before measuring, confirm the participating hosts agree on UTC closely enough
for the freshness contract. Peer probe timestamps are generated on the source
host and later evaluated against the executing host's clock; evidence more than
60 seconds in the future is rejected. On each host, record:

```bash
date -u +%s
```

Compare the values while the hosts are otherwise idle. If the observed skew is
material (use 30 seconds as the campaign stop threshold), repair host/provider
clock synchronization before taking any peer measurements. Do not weaken the
future-timestamp gate to accommodate a bad clock.

Before measuring, prepare every participating host to receive peer probes over
the experiment's private network. `meshfit probe --bandwidth` uses ping for RTT
and iperf3 for throughput; iperf3 therefore needs a server on the peer.

On each host, bind the server to that host's private experiment address and keep
its PID so it can be stopped cleanly after evidence capture:

```bash
export MESHFIT_PRIVATE_IP=<this-host-private-address>

command -v iperf3
test ! -e /tmp/meshfit-iperf3.pid
iperf3 -s -D -B "$MESHFIT_PRIVATE_IP" -I /tmp/meshfit-iperf3.pid
test -s /tmp/meshfit-iperf3.pid
kill -0 "$(cat /tmp/meshfit-iperf3.pid)"
```

Allow ICMP and TCP/5201 only between the experiment hosts. Do not expose the
iperf3 listener to the public Internet. Verify the private path before recording
evidence:

```bash
ping -c 1 <peer-private-address>
```

Because one MeshFit bandwidth probe now measures both forward and reverse
throughput and the planner treats the resulting node link as undirected, capture
one probe for each unordered host pair:

```bash
meshfit probe <node-b-private-address> --bandwidth > node-a-node-b-probe.yaml
meshfit probe <node-c-private-address> --bandwidth > node-a-node-c-probe.yaml
# run from node-b:
meshfit probe <node-c-private-address> --bandwidth > node-b-node-c-probe.yaml
```

After all peer evidence has been captured on a host, stop only the server started
for this campaign:

```bash
kill "$(cat /tmp/meshfit-iperf3.pid)"
rm -f /tmp/meshfit-iperf3.pid
```

Do not substitute configured NIC speed for measured bandwidth. If a candidate
requires a peer link that has no measured provenance, the readiness gate should
remain closed.

### 4. Build the coordinator snapshot

For a three-host campaign, use the manifest-based snapshot path. The positional
`meshfit snapshot` command accepts exactly two discovery files plus at most one
probe; it is not the multi-host assembly interface.

Create `snapshot-manifest.yaml` beside the discovery/probe evidence:

```yaml
discovery_files:
  - node-a.yaml
  - node-b.yaml
  - node-c.yaml
probes:
  - from_node: node-a
    to_node: node-b
    probe_file: node-a-node-b-probe.yaml
    kind: ethernet
  - from_node: node-a
    to_node: node-c
    probe_file: node-a-node-c-probe.yaml
    kind: ethernet
  - from_node: node-b
    to_node: node-c
    probe_file: node-b-node-c-probe.yaml
    kind: ethernet
```

Then materialize exactly one coordinator snapshot:

```bash
meshfit snapshot-manifest snapshot-manifest.yaml > cluster.yaml

meshfit benchmark-candidates \
  cluster.yaml \
  target.yaml \
  <meshfit-plan-id> \
  --require-distinct
```

Keep `snapshot-manifest.yaml` with the campaign inputs so the node/probe binding
is explicit and replayable. The campaign should not proceed unless the selected
MeshFit plan and baseline plans are genuinely distinct and all compute-ranked
baselines have non-zero observed proxy scores.

### 5. Inspect and freeze the real model identity

Inspect the exact immutable model artifact that will be used for the run:

```bash
meshfit inspect-model \
  /models/model.gguf \
  <model-id> \
  gguf \
  <quantization> \
  <revision> \
  > model-identity.yaml
```

The same model bytes may live at a different filesystem path on another host,
but their SHA-256 must match the materialized identity.

### 6. Materialize Benchmark 001 once

Create the kit on the coordinator. Pick an unused service port explicitly so the
runtime port becomes part of the frozen execution identity:

```bash
meshfit benchmark-kit \
  cluster.yaml \
  target.yaml \
  <meshfit-plan-id> \
  /models/model.gguf \
  model-identity.yaml \
  --listen-port 18181 \
  --max-peer-probe-age-seconds 1800 \
  --require-ready \
  --write-dir benchmark-001
```

Keep this directory immutable except for generated runtime artifacts and declared
`results/` slots. Do not regenerate the kit independently on each host.

Before distributing the kit, freeze the control-plane files that every real host
must see identically:

```bash
(
  cd benchmark-001
  command -v sha256sum
  sha256sum \
    kit.yaml \
    comparison.yaml \
    inputs/snapshot.yaml \
    inputs/target.yaml \
    inputs/model-identity.yaml \
    inputs/prompt.txt \
    > CONTROL.sha256
)
```

Do not include `artifacts/` or `results/` in this manifest: those are
intentionally generated during execution. Retain `CONTROL.sha256` with the
coordinator kit and copy it unchanged with the kit to every benchmark host.

### 7. Gate every real machine before launch

Place the same materialized kit on each benchmark host, preserving its contents.
Before MeshFit reads any control-plane input on that host, verify the coordinator
digest:

```bash
(
  cd benchmark-001
  sha256sum -c CONTROL.sha256
)
```

Any mismatch is a transport/control-plane failure: replace the host copy from the
coordinator's frozen kit. Do not regenerate or hand-edit the mismatching file on
the benchmark host.

Then set the stable logical ID and, when necessary, the local path to the same
model bytes:

```bash
export MESHFIT_NODE_ID=node-a
export MESHFIT_MODEL_PATH=/data/models/model.gguf

meshfit benchmark-host-check benchmark-001 \
  --current-host \
  --require-ready
```

This gate must validate the actual machine, not just the declared node name. It
checks the snapshot hardware identity, local accelerator topology where required,
runtime availability, model SHA-256, listen port, and the relevant peer evidence.
Reality Campaign 001 freezes a 1800-second peer-probe freshness contract in
`kit.yaml`; if any required cross-host measurement is older than that at
preflight time, re-run the affected peer probe and rebuild the frozen snapshot
and kit rather than executing against stale topology evidence.

For TensorParallel candidates, local topology is part of the execution identity.
A machine with the same GPU inventory but different NVLink/PCIe structure is not
the same benchmark host.

### 8. Dry-run the complete host assignment

Before any GPU runtime starts:

```bash
meshfit benchmark-run-host benchmark-001 \
  --current-host \
  --resume \
  --dry-run
```

Read the candidate list, pending runs, effective model path, service port, and
preflight issues. Do not begin the paid run while `ready` is false.

### 9. Execute repeated runs

Run only the host assignment already frozen in the kit:

```bash
meshfit benchmark-run-host benchmark-001 \
  --current-host \
  --resume
```

Do not run unrelated GPU workloads on the machine during the measurement window.
Keep campaign power/performance configuration and runtime versions stable across
all repeated runs. Do not replace a failed run with a hand-edited bundle.

### 10. Return evidence to the coordinator

If all machines share the same writable experiment directory, no extra transport
step is needed.

If they do not share storage, do **not** copy arbitrary `results/` files into
the coordinator kit. On each benchmark host, export only the result slots that
the frozen kit assigned to that host:

```bash
meshfit benchmark-export-host benchmark-001 --current-host > node-a-evidence.yaml
```

Transfer the generated host-evidence package with your normal authenticated
file-transfer mechanism. Transport is not a trust source.

On the coordinator, import the package into the coordinator's original frozen
kit:

```bash
meshfit benchmark-import-host benchmark-001 node-a-evidence.yaml
```

The coordinator kit is the only slot whitelist. Import verifies the package
SHA-256 and re-attests the submitted evidence against the frozen hardware,
model, topology, runtime, listen port, plan, prompt, context, concurrency,
`max_tokens`, warmup count, measured request count, and timeout contract
**before writing any result file**.

The import is package-atomic: every submitted slot must validate before any slot
is written. Re-importing byte-equivalent evidence is idempotent. A different
bundle in an already-populated slot is rejected rather than overwritten.
Non-kit slots, path traversal, tampered packages, and request-contract drift must
all fail closed.

### 11. Require completeness before looking for a headline

On the coordinator:

```bash
meshfit benchmark-status benchmark-001 --require-complete
```

A missing, malformed, stale, foreign-hardware, wrong-model, wrong-runtime,
wrong-port, wrong-plan, or wrong-topology bundle must keep the campaign
incomplete.

### 12. Finalize through the publication gate

Only after completeness succeeds:

```bash
meshfit benchmark-finalize \
  benchmark-001 \
  --markdown \
  --require-publishable
```

Then emit the proof receipt:

```bash
meshfit benchmark-proof \
  benchmark-001 \
  --require-publishable
```

The first public MeshFit number must come from this finalized evidence, not from
the GEMM proxy, planner estimates, fixture bundles, CI data, or a manually chosen
best run.

## Stop conditions

Abort the campaign and fix reality instead of relaxing a gate if any of the
following occurs:

- a real accelerator still has zero or missing `relative_compute`;
- a required peer link lacks measured provenance;
- participating host clocks differ by more than the campaign's 30-second pre-probe threshold;
- a benchmark host fails `CONTROL.sha256` verification;
- hardware or local topology no longer matches the frozen snapshot;
- the model SHA-256 differs between hosts;
- the selected runtime is unavailable on its assigned host;
- the service port is occupied;
- candidate plans collapse to the same effective placement;
- any persisted bundle fails re-attestation;
- repeated evidence is incomplete or non-publishable.

The next engineering decision should be driven by whichever of these real checks
fails on the actual machines. If none fail, stop changing the framework and run
the experiment.


## Provider appendix: Lambda On-Demand Cloud

Use this appendix only when Reality Campaign 001 is run on Lambda On-Demand
Cloud (ODC). The provider gate is intentionally outside MeshFit's planner: live
capacity, firewall state, instance lifecycle, and billing are external facts and
must be checked immediately before spend.

### A. Require authenticated live capacity before launch

Do not infer live capacity from the public pricing page. Query Lambda's
authenticated instance-types endpoint and retain the response as campaign
evidence:

```bash
test -n "${LAMBDA_API_KEY:?set LAMBDA_API_KEY outside shell history}"

curl --fail --silent --show-error \
  --request GET \
  --url 'https://cloud.lambda.ai/api/v1/instance-types' \
  --header 'accept: application/json' \
  --header "Authorization: Bearer ${LAMBDA_API_KEY}" \
  > lambda-instance-types.json
```

Inspect the exact provider type names instead of hard-coding names from a stale
document:

```bash
jq -r '
  .data
  | to_entries[]
  | [
      .key,
      .value.instance_type.gpu_description,
      (.value.instance_type.specs.gpus | tostring),
      .value.instance_type.architecture,
      (.value.instance_type.price_cents_per_hour | tostring),
      (.value.regions_with_capacity_available | map(.name) | join(","))
    ]
  | @tsv
' lambda-instance-types.json
```

For the current primary campaign shape, choose the actual returned type keys for:

- node-a: 2 x NVIDIA A6000 48 GB;
- node-b: 1 x NVIDIA H100 PCIe 80 GB;
- node-c: 1 x NVIDIA B200 180 GB.

Then compute the live same-region intersection from the captured response:

```bash
export MESHFIT_NODE_A_TYPE=<provider-type-key-for-2x-a6000>
export MESHFIT_NODE_B_TYPE=<provider-type-key-for-1x-h100-pcie>
export MESHFIT_NODE_C_TYPE=<provider-type-key-for-1x-b200>

jq -r \
  --arg a "$MESHFIT_NODE_A_TYPE" \
  --arg b "$MESHFIT_NODE_B_TYPE" \
  --arg c "$MESHFIT_NODE_C_TYPE" '
    [.data[$a].regions_with_capacity_available[].name] as $a_regions
    | [.data[$b].regions_with_capacity_available[].name] as $b_regions
    | [.data[$c].regions_with_capacity_available[].name] as $c_regions
    | [
        $a_regions[]
        | . as $region
        | select(
            ($b_regions | index($region)) != null
            and ($c_regions | index($region)) != null
          )
      ]
    | unique[]
  ' lambda-instance-types.json
```

The campaign must not launch unless this prints at least one region. Capacity is
first-come and can change between this query and launch, so a later
provider/capacity error is a normal external gate, not permission to substitute a
different hardware shape silently.

### B. Freeze the provider cost ceiling before spend

Record the current hourly prices from the same authenticated response. Also
cross-check them against the provider's public pricing page before launch.

For the primary shape, the campaign tracker currently uses this public-price
reference:

```text
2 x A6000 48 GB = $2.18/hour
1 x H100 PCIe 80 GB = $3.29/hour
1 x B200 180 GB = $6.99/hour
reference concurrent burn = $12.46/hour before tax/storage
first-run GPU budget ceiling = $50
```

That reference implies roughly four hours of concurrent instance time before the
GPU-only ceiling is reached. Do not treat the prices as constants. Compute the
actual campaign burn rate from `lambda-instance-types.json`, write it into the
campaign notes before launch, and derive the maximum allowed wall-clock time
from the frozen $50 first-run ceiling. If the live prices, taxes/storage
assumptions, or required wall-clock time would cross that ceiling, stop before
provisioning rather than hoping to terminate in time.

### C. Launch exactly the frozen provider shape

Create or select a Lambda firewall ruleset in the chosen region before launch.
At this point the peer instance addresses do not exist yet, so the pre-launch
ruleset should preserve only the SSH access you actually need; do not pre-open
TCP/5201 broadly just to make launch convenient. Attach this same-region ruleset
to the campaign instances, then tighten/update it after the provider assigns the
real peer addresses. Export the chosen region, SSH key name, and ruleset ID:

```bash
export MESHFIT_LAMBDA_REGION=<region-from-the-live-intersection>
export MESHFIT_LAMBDA_SSH_KEY_NAME=<existing-lambda-ssh-key-name>
export MESHFIT_LAMBDA_FIREWALL_RULESET_ID=<same-region-ruleset-id>
```

Launch each distinct shape separately and retain every provider response. The
Lambda launch endpoint is rate-limited, so keep at least 12 seconds between
these calls:

```bash
launch_meshfit_lambda() {
  local instance_type="$1"
  local instance_name="$2"
  local receipt="$3"

  jq -cn \
    --arg region "$MESHFIT_LAMBDA_REGION" \
    --arg type "$instance_type" \
    --arg ssh_key "$MESHFIT_LAMBDA_SSH_KEY_NAME" \
    --arg ruleset "$MESHFIT_LAMBDA_FIREWALL_RULESET_ID" \
    --arg name "$instance_name" '
      {
        region_name: $region,
        instance_type_name: $type,
        ssh_key_names: [$ssh_key],
        file_system_names: [],
        name: $name,
        firewall_rulesets: [{id: $ruleset}],
        tags: [{key: "meshfit-campaign", value: "reality-001"}]
      }
    ' \
  | curl --fail --silent --show-error \
      --request POST \
      --url 'https://cloud.lambda.ai/api/v1/instance-operations/launch' \
      --header 'accept: application/json' \
      --header 'content-type: application/json' \
      --header "Authorization: Bearer ${LAMBDA_API_KEY}" \
      --data-binary @- \
      > "$receipt"
}

launch_meshfit_lambda \
  "$MESHFIT_NODE_A_TYPE" meshfit-reality-001-a lambda-launch-node-a.json
sleep 12
launch_meshfit_lambda \
  "$MESHFIT_NODE_B_TYPE" meshfit-reality-001-b lambda-launch-node-b.json
sleep 12
launch_meshfit_lambda \
  "$MESHFIT_NODE_C_TYPE" meshfit-reality-001-c lambda-launch-node-c.json
```

Extract and freeze the returned instance IDs immediately:

```bash
export MESHFIT_NODE_A_INSTANCE_ID="$(
  jq -er '.data.instance_ids[0]' lambda-launch-node-a.json
)"
export MESHFIT_NODE_B_INSTANCE_ID="$(
  jq -er '.data.instance_ids[0]' lambda-launch-node-b.json
)"
export MESHFIT_NODE_C_INSTANCE_ID="$(
  jq -er '.data.instance_ids[0]' lambda-launch-node-c.json
)"

export MESHFIT_LAMBDA_INSTANCE_IDS_JSON="$(
  jq -cn \
    --arg a "$MESHFIT_NODE_A_INSTANCE_ID" \
    --arg b "$MESHFIT_NODE_B_INSTANCE_ID" \
    --arg c "$MESHFIT_NODE_C_INSTANCE_ID" \
    '[$a, $b, $c]'
)"
```

If any launch fails with insufficient capacity, do not silently replace the
failed shape. Terminate any instances that did launch, retain the failed launch
receipt/error, refresh the authenticated capacity intersection, and make a new
campaign decision.

### D. Make the intended network path explicit

Lambda ODC does not open arbitrary inbound TCP ports by default. Its provider
firewall rules use public IPv4/CIDR source networks, so do not treat a Lambda
ruleset as a private-network ACL for the returned `private_ip` values.

Keep the attached provider ruleset focused on the public exposure surface needed
for administration (for example, narrowly scoped SSH). Do **not** open
TCP/5201 on the public interface merely to make the peer probe pass.

The ODC API exposes both public and private instance addresses, but the campaign
must not assume that a returned private address proves private peer
reachability. After all three instances are active, capture their provider
metadata, bind the iperf3 server to the host's private address as described in
the main runbook, and test the exact private path that will be benchmarked:

```bash
curl --fail --silent --show-error \
  --request GET \
  --url 'https://cloud.lambda.ai/api/v1/instances' \
  --header 'accept: application/json' \
  --header "Authorization: Bearer ${LAMBDA_API_KEY}" \
  > lambda-running-instances.json

# On each host, using the peer private IP recorded above:
ping -c 1 <peer-private-ip>
timeout 5 bash -c 'cat < /dev/null > /dev/tcp/<peer-private-ip>/5201'
```

Start the bound iperf3 servers described in the main runbook before the
TCP/5201 check. Binding to the private address keeps this measurement service off
the public interface.

If the private path is not reachable, do not add a public TCP/5201 rule and
silently switch the experiment to public Internet addresses: that would change
the network tier being measured. Stop, record the provider/network blocker, and
choose a provider or topology whose intended experiment path can be demonstrated.

### E. Keep provider identity beside MeshFit identity

For each launched host, retain at least:

- Lambda instance ID;
- Lambda region;
- provider instance-type key and GPU description;
- public IP and private IP returned by the API;
- MeshFit logical node ID;
- immutable `meshfit discover` artifact;
- firewall ruleset ID used for the campaign.

Provider metadata is not a substitute for MeshFit's hardware attestation. It is
the external receipt that ties the paid resource to the experiment window.

### F. Terminate through Lambda, not through the guest OS

As soon as host evidence has been exported and copied off the paid instances,
terminate every campaign instance through the Lambda control plane:

```bash
export MESHFIT_LAMBDA_INSTANCE_IDS_JSON='[
  "<node-a-instance-id>",
  "<node-b-instance-id>",
  "<node-c-instance-id>"
]'

curl --fail --silent --show-error \
  --request POST \
  --url 'https://cloud.lambda.ai/api/v1/instance-operations/terminate' \
  --header 'accept: application/json' \
  --header 'content-type: application/json' \
  --header "Authorization: Bearer ${LAMBDA_API_KEY}" \
  --data "$(jq -cn \
    --argjson ids "$MESHFIT_LAMBDA_INSTANCE_IDS_JSON" \
    '{instance_ids: $ids}')" \
  > lambda-terminate-receipt.json
```

Retain `lambda-terminate-receipt.json` with the campaign evidence. Do **not**
use `shutdown`, `poweroff`, or `systemctl poweroff` as a billing stop:
guest-OS shutdown is not instance termination.

After termination, query the provider again and confirm that none of the
campaign instance IDs remains active. Also remove any campaign-only firewall
ruleset after the instances are gone.

### G. Provider stop conditions

Abort the paid campaign before benchmark execution if any of these is true:

- the authenticated same-region capacity intersection is empty;
- the provider launches a different instance type than the frozen campaign
  shape;
- any instance reports an unexpected architecture;
- the intended private experiment path cannot be demonstrated;
- TCP/5201 cannot be restricted to experiment peers;
- the observed hourly burn rate violates the campaign budget ceiling;
- instance termination cannot be confirmed through the provider control plane.

These conditions are external Reality failures. They should produce evidence
and a new execution decision, not a relaxed MeshFit benchmark gate.
