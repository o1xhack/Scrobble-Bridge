#!/bin/sh
set -eu

soak_output=${1:-target/qa/docker-soak-7day.jsonl}
expected_seconds=${SCROBBLE_SOAK_SECONDS:-604800}

if [ ! -s "$soak_output" ]; then
  printf '{"status":"failed","reason":"missing_or_empty_log"}\n'
  exit 1
fi

summary=$(jq -cse --argjson expected "$expected_seconds" '
  def checks: map(select(.container != null));
  def completions: map(select(.result == "completed"));
  {
    status:
      (if (checks | any(.result == "failed")) then "failed"
       elif (completions | any(.duration_seconds >= $expected))
         and ((checks | map(.architecture) | unique) == ["aarch64", "x86_64"])
         and (checks | length) >= 2
       then "passed"
       else "in_progress"
       end),
    expected_seconds: $expected,
    checks: (checks | length),
    failures: (checks | map(select(.result != "passed")) | length),
    architectures: (checks | map(.architecture) | unique),
    first_timestamp: (checks | first | .timestamp),
    last_timestamp: (checks | last | .timestamp),
    completed_duration_seconds: ([completions[].duration_seconds] | max // null)
  }
' "$soak_output")

printf '%s\n' "$summary"

case $(printf '%s' "$summary" | jq -r '.status') in
  passed) exit 0 ;;
  in_progress) exit 2 ;;
  *) exit 1 ;;
esac
