FROM ubuntu:24.04

RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends python3 python3-yaml \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY environment/campaign /app/campaign
COPY environment/tools /app/tools

RUN chmod 0555 /app/tools/verify_proof.py \
    && chmod 0444 /app/campaign/CONTROL.sha256 \
       /app/campaign/benchmark-proof.yaml \
       /app/campaign/frozen/plan.yaml \
       /app/campaign/transfers/run-01.yaml
