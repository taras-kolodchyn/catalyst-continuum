#!/usr/bin/env bash

WAIT_LAST_CONTAINER_STATUS="not yet probed"
WAIT_LAST_HTTP_RESULT="not yet probed"

docker_container_runtime_status() {
  docker inspect \
    --format='{{if .State.Health}}{{.State.Health.Status}}{{else}}{{.State.Status}}{{end}}' \
    "$1" 2>/dev/null || true
}

wait_for_docker_container_status() {
  local name="$1"
  local container_name="$2"
  local attempts="$3"
  shift 3

  if [ "$#" -eq 0 ]; then
    echo "wait_for_docker_container_status requires at least one accepted status" >&2
    return 1
  fi

  local status=""
  local accepted_status
  WAIT_LAST_CONTAINER_STATUS="not yet probed"

  for _ in $(seq 1 "$attempts"); do
    status="$(docker_container_runtime_status "$container_name")"
    if [ -z "$status" ]; then
      status="unknown"
    fi
    WAIT_LAST_CONTAINER_STATUS="$status"
    for accepted_status in "$@"; do
      if [ "$status" = "$accepted_status" ]; then
        return 0
      fi
    done
    sleep 1
  done

  echo \
    "$name did not reach an accepted status after ${attempts}s (last status: ${WAIT_LAST_CONTAINER_STATUS}; accepted: $*)" >&2
  return 1
}

probe_http_capture() {
  local url="$1"
  local output_file="$2"
  shift 2

  local http_code=""
  local curl_exit=0
  WAIT_LAST_HTTP_RESULT="not yet probed"
  : >"$output_file"

  if http_code="$(curl -sS -o "$output_file" -w '%{http_code}' "$@" "$url" 2>/dev/null)"; then
    WAIT_LAST_HTTP_RESULT="HTTP ${http_code}"
    case "$http_code" in
      2*|3*)
        return 0
        ;;
    esac
  else
    curl_exit=$?
    WAIT_LAST_HTTP_RESULT="curl exit ${curl_exit} (http ${http_code:-000})"
  fi

  return 1
}

wait_for_http_capture() {
  local name="$1"
  local url="$2"
  local output_file="$3"
  local attempts="$4"
  shift 4

  for _ in $(seq 1 "$attempts"); do
    if probe_http_capture "$url" "$output_file" "$@"; then
      return 0
    fi
    sleep 1
  done

  echo \
    "$name did not become ready at $url after ${attempts}s (last probe: ${WAIT_LAST_HTTP_RESULT})" >&2
  return 1
}

is_transient_docker_run_error() {
  local output="$1"
  printf '%s' "$output" | grep -Eiq \
    'Client\.Timeout|request canceled|TLS handshake timeout|i/o timeout|context deadline exceeded|connection reset by peer|unexpected EOF|temporary failure|network is unreachable|no such host|connection timed out'
}

run_with_transient_docker_retry() {
  local label="$1"
  shift

  local attempt=1
  local max_attempts="${CATALYST_DOCKER_TRANSIENT_MAX_ATTEMPTS:-3}"
  local stdout_file=""
  local stderr_file=""
  local combined_output=""

  stdout_file="$(mktemp)"
  stderr_file="$(mktemp)"

  while [ "$attempt" -le "$max_attempts" ]; do
    if "$@" >"$stdout_file" 2>"$stderr_file"; then
      cat "$stdout_file"
      rm -f "$stdout_file" "$stderr_file"
      return 0
    fi

    combined_output="$(
      {
        cat "$stdout_file"
        cat "$stderr_file"
      } 2>/dev/null
    )"

    if [ "$attempt" -lt "$max_attempts" ] && is_transient_docker_run_error "$combined_output"; then
      printf \
        '[docker-retry] retrying %s after transient docker failure (attempt %s/%s)\n' \
        "$label" \
        "$((attempt + 1))" \
        "$max_attempts" >&2
      attempt=$((attempt + 1))
      sleep "$attempt"
      continue
    fi

    cat "$stdout_file"
    cat "$stderr_file" >&2
    rm -f "$stdout_file" "$stderr_file"
    return 1
  done

  rm -f "$stdout_file" "$stderr_file"
  return 1
}
