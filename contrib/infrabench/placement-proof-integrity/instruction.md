A heterogeneous-infrastructure benchmark campaign was transferred between hosts.

The handoff in `/app/campaign` no longer passes its independent audit check. Restore the campaign to a **verifiable evidence state**.

Run:

```bash
python3 /app/tools/verify_proof.py /app/campaign
```

until it succeeds.

Constraints:

- do **not** change the frozen placement decision in `frozen/plan.yaml`;
- do **not** change `benchmark-proof.yaml`;
- do **not** change `CONTROL.sha256`;
- do **not** change the trusted transfer package in `transfers/run-01.yaml`;
- do **not** change the benchmark claim or measured values;
- do not replace evidence files with symlinks or paths outside `/app/campaign`;
- repair the evidence state rather than disabling or modifying the verifier.

The final state must be independently auditable from the frozen control inputs, proof receipt, and raw benchmark bundle.
