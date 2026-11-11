#!/bin/sh
set -eu
mkdir -p /logs/verifier
if [ -f /tests/connections.json ]; then
  cp /tests/connections.json /logs/verifier/connections.json
fi
set -- bench-verify \
  --scenario "$(cat /tests/scenario.txt)" \
  --contract /tests/expect.json \
  --task-source /tests/task-source.json \
  --task-context /tests/task-context.json \
  --evidence /exports/evidence.json \
  --runner-policy /tests/runner-policy.json \
  --runner-admission /exports/runner-admission.json \
  --realized-manifest /exports/realized-manifest.json \
  --inference-evidence /exports/inference-evidence.json \
  --output /logs/verifier \
  --allow-unattested-runner \
  --withhold-invalid-reward
if [ -f /app/answer.md ]; then
  set -- "$@" --answer /app/answer.md
fi
exec "$@"
