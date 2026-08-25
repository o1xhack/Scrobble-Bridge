#!/bin/sh
set -eu

soak_seconds=${SCROBBLE_SOAK_SECONDS:-604800}
soak_interval=${SCROBBLE_SOAK_INTERVAL_SECONDS:-300}
soak_output=${SCROBBLE_SOAK_OUTPUT:-target/qa/docker-soak.jsonl}
soak_arm_container=${SCROBBLE_SOAK_ARM_CONTAINER:-scrobble-bridge-qa-arm64}
soak_amd_container=${SCROBBLE_SOAK_AMD_CONTAINER:-scrobble-bridge-qa-amd64}
soak_arm_port=${SCROBBLE_SOAK_ARM_PORT:-18788}
soak_amd_port=${SCROBBLE_SOAK_AMD_PORT:-28789}

mkdir -p "$(dirname "$soak_output")"
soak_started=$(date +%s)
soak_deadline=$((soak_started + soak_seconds))

check_container() {
  check_name=$1
  check_port=$2
  check_expected_arch=$3
  check_timestamp=$(date -u '+%Y-%m-%dT%H:%M:%SZ')
  check_health=$(docker inspect --format '{{if .State.Health}}{{.State.Health.Status}}{{else}}none{{end}}' "$check_name")
  check_arch=$(docker exec "$check_name" uname -m)
  check_token=$(docker exec "$check_name" sh -c 'cat /data/secrets/admin.token')
  check_live=$(curl -sS -o /dev/null -w '%{http_code}' "http://127.0.0.1:$check_port/health/live")
  check_ready=$(curl -sS -o /dev/null -w '%{http_code}' "http://127.0.0.1:$check_port/health/ready")
  check_status=$(curl -sS -o /dev/null -w '%{http_code}' \
    -H "Authorization: Bearer $check_token" \
    "http://127.0.0.1:$check_port/api/v1/status")

  if [ "$check_health" != healthy ] \
    || [ "$check_arch" != "$check_expected_arch" ] \
    || [ "$check_live" != 200 ] \
    || { [ "$check_ready" != 200 ] && [ "$check_ready" != 503 ]; } \
    || [ "$check_status" != 200 ]; then
    check_result=failed
  else
    check_result=passed
  fi

  printf '{"timestamp":"%s","container":"%s","architecture":"%s","health":"%s","live":%s,"ready":%s,"status":%s,"result":"%s"}\n' \
    "$check_timestamp" "$check_name" "$check_arch" "$check_health" \
    "$check_live" "$check_ready" "$check_status" "$check_result" >>"$soak_output"

  [ "$check_result" = passed ]
}

while :; do
  check_container "$soak_arm_container" "$soak_arm_port" aarch64
  check_container "$soak_amd_container" "$soak_amd_port" x86_64
  soak_now=$(date +%s)
  if [ "$soak_now" -ge "$soak_deadline" ]; then
    printf '{"timestamp":"%s","result":"completed","duration_seconds":%s}\n' \
      "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" "$soak_seconds" >>"$soak_output"
    break
  fi
  sleep "$soak_interval"
done
