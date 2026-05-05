#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

PID_DIR="${CATALYST_LOCAL_HELPER_PID_DIR:-$ROOT_DIR/.continuum/local-helper-pids}"
HELPER_LABEL_KEY="io.catalyst-continuum.local-helper"
HELPER_LABEL_VALUE="true"
DISPOSABLE_LABEL_KEY="io.catalyst-continuum.disposable"
DISPOSABLE_LABEL_VALUE="true"

log_phase() {
  printf '[cleanup-local-dev] %s\n' "$1"
}

pid_command() {
  ps -o command= -p "$1" 2>/dev/null || true
}

pid_is_running() {
  ps -p "$1" >/dev/null 2>&1
}

pid_is_repo_local_helper() {
  local pid="$1"
  local command=""
  command="$(pid_command "$pid")"
  case "$command" in
    *operator-ui-smoke.sh*|*run-operator-ui.sh*|*solo-demo.sh*|*catalyst-continuum-orchestrator*serve*)
      return 0
      ;;
  esac
  return 1
}

cleanup_pidfile() {
  local pid_file="$1"
  local pid=""

  if [ ! -f "$pid_file" ]; then
    return
  fi

  pid="$(tr -d '[:space:]' <"$pid_file" || true)"
  if ! printf '%s\n' "$pid" | grep -Eq '^[0-9]+$'; then
    rm -f "$pid_file"
    return
  fi

  if ! pid_is_running "$pid"; then
    rm -f "$pid_file"
    return
  fi

  if ! pid_is_repo_local_helper "$pid"; then
    log_phase "skipping pid $pid from $(basename "$pid_file") because it is outside repo-local helper scope"
    rm -f "$pid_file"
    return
  fi

  log_phase "stopping helper pid $pid from $(basename "$pid_file")"
  kill "$pid" >/dev/null 2>&1 || true
  for _ in $(seq 1 20); do
    if ! pid_is_running "$pid"; then
      break
    fi
    sleep 0.1
  done

  if pid_is_running "$pid"; then
    log_phase "forcing helper pid $pid to stop"
    kill -9 "$pid" >/dev/null 2>&1 || true
  fi

  rm -f "$pid_file"
}

cleanup_pidfiles() {
  local pid_file=""
  if [ ! -d "$PID_DIR" ]; then
    return
  fi

  for pid_file in "$PID_DIR"/*.pid; do
    if [ ! -e "$pid_file" ]; then
      break
    fi
    cleanup_pidfile "$pid_file"
  done

  rmdir "$PID_DIR" >/dev/null 2>&1 || true
}

cleanup_labeled_containers() {
  local container_ids=""
  local container_id=""
  local container_name=""

  container_ids="$(
    {
      docker ps -aq --filter "label=${HELPER_LABEL_KEY}=${HELPER_LABEL_VALUE}" 2>/dev/null || true
      docker ps -aq --filter "label=${DISPOSABLE_LABEL_KEY}=${DISPOSABLE_LABEL_VALUE}" 2>/dev/null || true
    } | sort -u
  )"

  if [ -z "$container_ids" ]; then
    return
  fi

  while IFS= read -r container_id; do
    if [ -z "$container_id" ]; then
      continue
    fi
    container_name="$(
      docker inspect --format '{{.Name}}' "$container_id" 2>/dev/null | sed 's#^/##'
    )"
    log_phase "removing disposable Catalyst container ${container_name:-$container_id}"
    docker rm -f "$container_id" >/dev/null 2>&1 || true
  done <<EOF
$container_ids
EOF
}

cleanup_pidfiles
cleanup_labeled_containers
log_phase "repo-local helper cleanup completed"
