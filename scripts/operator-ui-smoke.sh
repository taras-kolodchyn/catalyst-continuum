#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

# shellcheck disable=SC1091
source "$ROOT_DIR/versions.env"
# shellcheck disable=SC1091
source "$ROOT_DIR/scripts/lib/readiness.sh"

usage() {
  cat <<'EOF'
Usage: ./scripts/operator-ui-smoke.sh [options]

Run a browser-level smoke test for the built-in operator UI. The script:
  - starts a disposable pinned Postgres container
  - seeds a full MVP run through the existing CLI smoke flow
  - starts the host-run operator UI against that state
  - clicks the run ledger, Flow/Agents tabs, report cards, and manual refresh
  - verifies WebSocket live updates, no hard reloads, stable agent-panel focus,
    no iframe refresh, and no browser errors

Options:
  --scenario NAME       CI smoke scenario used to seed UI data (default: mvp-cli-tool)
  --http-port PORT      Fixed operator UI port (default: random loopback port)
  --postgres-port PORT  Fixed disposable Postgres port (default: random loopback port)
  --output-root PATH    Directory for logs, screenshots, and smoke artifacts
                         (default: .continuum/operator-ui-smoke)
  --repository-targets-file PATH
                        Optional repository target allowlist to expose in the UI
  --expect-remote-publication-blocked
                        Assert the selected run keeps PR export local-only while
                        publish and draft PR stay disabled by repository policy
  --skip-build          Reuse the existing debug binary
  --help                Show this help text
EOF
}

resolve_cargo_target_root() {
  if [ -n "${CARGO_TARGET_DIR:-}" ]; then
    case "$CARGO_TARGET_DIR" in
      /*)
        printf '%s\n' "$CARGO_TARGET_DIR"
        ;;
      *)
        printf '%s/%s\n' "$ROOT_DIR" "$CARGO_TARGET_DIR"
        ;;
    esac
  else
    printf '%s/target\n' "$ROOT_DIR"
  fi
}

allocate_loopback_port() {
  python3 - <<'PY'
import socket

with socket.socket() as sock:
    sock.bind(("127.0.0.1", 0))
    print(sock.getsockname()[1])
PY
}

log_phase() {
  printf '[operator-ui-smoke] %s\n' "$1"
}

require_command() {
  local command_name="$1"
  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "$command_name is required for operator UI smoke" >&2
    exit 1
  fi
}

playwright_installed_version() {
  node -e '
const packageJson = process.argv[1];
try {
  console.log(require(packageJson).version);
} catch (_) {
  process.exit(1);
}
' "$PLAYWRIGHT_RUNNER_DIR/node_modules/playwright/package.json" 2>/dev/null || true
}

ensure_playwright_runner() {
  local actual_version=""

  mkdir -p "$PLAYWRIGHT_RUNNER_DIR"
  actual_version="$(playwright_installed_version)"
  if [ "$actual_version" = "$PLAYWRIGHT_NPM_VERSION" ]; then
    return
  fi

  cat >"$PLAYWRIGHT_RUNNER_DIR/package.json" <<EOF
{
  "name": "catalyst-continuum-operator-ui-smoke-runner",
  "version": "1.0.0",
  "private": true,
  "type": "commonjs",
  "dependencies": {
    "playwright": "${PLAYWRIGHT_NPM_VERSION}"
  }
}
EOF

  npm install \
    --prefix "$PLAYWRIGHT_RUNNER_DIR" \
    --save-exact \
    --no-audit \
    --no-fund \
    "playwright@${PLAYWRIGHT_NPM_VERSION}" >/dev/null
}

postgres_is_healthy() {
  [ "$(docker inspect --format='{{.State.Health.Status}}' "$POSTGRES_CONTAINER_NAME")" = "healthy" ]
}

wait_for_postgres() {
  for _ in $(seq 1 45); do
    if postgres_is_healthy; then
      return 0
    fi
    sleep 1
  done

  docker logs "$POSTGRES_CONTAINER_NAME" >&2 || true
  echo "operator UI smoke Postgres did not become healthy" >&2
  return 1
}

wait_for_ui_ready() {
  local last_result="not yet probed"

  for _ in $(seq 1 45); do
    if probe_http_capture "http://127.0.0.1:${HTTP_PORT}/readyz" "$READYZ_FILE"; then
      return 0
    fi
    last_result="$WAIT_LAST_HTTP_RESULT"

    if ! kill -0 "$UI_PID" >/dev/null 2>&1; then
      cat "$UI_LOG_FILE" >&2 || true
      echo "operator UI exited before readiness (last probe: ${last_result})" >&2
      return 1
    fi

    sleep 1
  done

  cat "$UI_LOG_FILE" >&2 || true
  echo "operator UI did not become ready after 45s (last probe: ${last_result})" >&2
  return 1
}

verify_dashboard_repository_targets() {
  local dashboard_file="$OUTPUT_DIR/dashboard.json"

  if ! curl -fsS "http://127.0.0.1:${HTTP_PORT}/ui/dashboard" >"$dashboard_file"; then
    echo "failed to fetch operator UI dashboard snapshot" >&2
    return 1
  fi

  if [ -z "$REPOSITORY_TARGETS_FILE" ]; then
    return 0
  fi

  python3 - "$dashboard_file" <<'PY'
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as handle:
    payload = json.load(handle)

repository_targets = payload["config"]["data"]["repository_targets"]
if not repository_targets.get("enforcement_enabled"):
    raise SystemExit("repository target enforcement was expected in /ui/dashboard, but it is disabled")

targets = repository_targets.get("targets") or []
if not targets:
    raise SystemExit("repository target enforcement is enabled, but /ui/dashboard returned no targets")

print(f"repository_target_count={len(targets)}")
print(f"first_repository_target={targets[0]['target_id']}")
PY
}

write_browser_check() {
  cat >"$BROWSER_CHECK_FILE" <<'NODE'
const fs = require("fs");
const path = require("path");

const { chromium } = require(process.env.PLAYWRIGHT_MODULE);

const outDir = process.env.OPERATOR_UI_SMOKE_OUTPUT_DIR;
const uiUrl = process.env.OPERATOR_UI_SMOKE_URL;
const expectRepositoryBootstrapGuidance =
  process.env.OPERATOR_UI_EXPECT_REPOSITORY_BOOTSTRAP_GUIDANCE !== "0";
const expectRepositoryTargetPolicy =
  process.env.OPERATOR_UI_EXPECT_REPOSITORY_TARGET_POLICY === "1";
const expectRemotePublicationBlocked =
  process.env.OPERATOR_UI_EXPECT_REMOTE_PUBLICATION_BLOCKED === "1";
const summaryPath = path.join(outDir, "operator-ui-browser-summary.json");
const screenshotPath = path.join(outDir, "operator-ui-browser.png");

async function main() {
  const browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({ viewport: { width: 1440, height: 1100 } });
  const page = await context.newPage();
  const consoleMessages = [];
  const pageErrors = [];
  const requestFailures = [];
  let loadEvents = 0;
  let mainFrameNavigations = 0;
  const focusChecks = [];

  page.on("console", (message) => {
    if (["error", "warning"].includes(message.type())) {
      consoleMessages.push({ type: message.type(), text: message.text() });
    }
  });
  page.on("pageerror", (error) => pageErrors.push(error.message));
  page.on("requestfailed", (request) => {
    const failure = request.failure();
    requestFailures.push({ url: request.url(), error: failure && failure.errorText });
  });
  page.on("load", () => {
    loadEvents += 1;
  });
  page.on("framenavigated", (frame) => {
    if (frame === page.mainFrame()) {
      mainFrameNavigations += 1;
    }
  });

  const response = await page.goto(uiUrl, { waitUntil: "domcontentloaded" });
  await page.waitForLoadState("load");
  await page.waitForSelector("#runsList [data-run-id]", { timeout: 15000 });
  await page.waitForFunction(() => {
    const element = document.querySelector("#lastRefresh");
    return element && element.textContent.includes("WebSocket stream active");
  }, { timeout: 15000 });

  const loadEventsAfterInitial = loadEvents;
  const mainFrameNavigationsAfterInitial = mainFrameNavigations;

  await page.click("#runsList [data-run-id]");
  await page.waitForSelector("#runDetailShell");
  await page.click('[data-mission-tab="agents"]');
  await page.waitForSelector("#missionAgentsPanel");
  await page.waitForSelector("#agentLaneGrid .agent-lane", { timeout: 10000 });

  const agentFilters = await page.locator("[data-agent-filter]").count();
  if (agentFilters > 1) {
    const expectedFocusKey = await page
      .locator("[data-agent-filter]")
      .nth(1)
      .getAttribute("data-ui-stable-key");
    await page.locator("[data-agent-filter]").nth(1).click();
    await page.waitForTimeout(250);
    focusChecks.push({
      surface: "agent-filter",
      expected: expectedFocusKey,
      actual: await stableFocusKey(page),
    });
  }

  const reportCards = await page.locator("[data-agent-report-artifact-id]").count();
  if (reportCards > 0) {
    const expectedFocusKey = await page
      .locator("[data-agent-report-artifact-id]")
      .first()
      .getAttribute("data-ui-stable-key");
    await page.locator("[data-agent-report-artifact-id]").first().click();
    await page.waitForTimeout(250);
    focusChecks.push({
      surface: "agent-report",
      expected: expectedFocusKey,
      actual: await stableFocusKey(page),
    });
  }

  const logCards = await page.locator("[data-agent-log-artifact-id]").count();
  if (logCards > 0) {
    const expectedFocusKey = await page
      .locator("[data-agent-log-artifact-id]")
      .first()
      .getAttribute("data-ui-stable-key");
    await page.locator("[data-agent-log-artifact-id]").first().click();
    await page.waitForTimeout(250);
    focusChecks.push({
      surface: "agent-log",
      expected: expectedFocusKey,
      actual: await stableFocusKey(page),
    });
  }

  await page.click('[data-mission-tab="flow"]');
  await page.waitForSelector("#missionFlowPanel");
  await page.click('[data-mission-tab="agents"]');
  await page.waitForSelector("#missionAgentsPanel");
  await page.waitForFunction(() => {
    const refreshButton = document.querySelector("#refreshButton");
    return refreshButton && !refreshButton.disabled;
  }, { timeout: 10000 });
  await page.click("#refreshButton");
  await page.waitForFunction(() => {
    const element = document.querySelector("#lastRefresh");
    return element && element.textContent.includes("WebSocket stream active");
  }, { timeout: 10000 });
  await page.waitForTimeout(500);

  const summary = await page.evaluate(() => {
    const text = (selector) => document.querySelector(selector)?.textContent?.trim() || "";
    const count = (selector) => document.querySelectorAll(selector).length;
    const runActionButtons = (actionId) =>
      Array.from(document.querySelectorAll(`[data-run-action="${actionId}"]`));
    const buttonStats = (actionId) => {
      const buttons = runActionButtons(actionId);
      return {
        count: buttons.length,
        enabledCount: buttons.filter((button) => !button.disabled).length,
        disabledCount: buttons.filter((button) => button.disabled).length,
      };
    };

    return {
      title: document.title,
      heroHeading: document.querySelector("h1")?.textContent?.trim() || "",
      lastRefresh: text("#lastRefresh"),
      selectedRunLabel: text("#selectedRunLabel"),
      runCardCount: count("#runsList [data-run-id]"),
      taskRows: count("#taskTableWrap tbody tr"),
      artifactRows: count("#artifactTableWrap tbody tr"),
      eventItems: count("#eventTimeline article"),
      agentLaneCount: count("#agentLaneGrid .agent-lane"),
      agentFilterCount: count("[data-agent-filter]"),
      agentReportCardCount: count("[data-agent-report-artifact-id]"),
      iframeCount: count("iframe"),
      bodyTextIncludesDraftPr:
        document.body.textContent.includes("draft PR") ||
        document.body.textContent.includes("Draft PR"),
      bodyTextIncludesRepositoryBootstrap:
        document.body.textContent.includes("repository-targets-bootstrap"),
      bodyTextIncludesRepositoryTargetPolicyState:
        document.body.textContent.includes("Matching target") ||
        document.body.textContent.includes("No matching repository target") ||
        document.body.textContent.includes("Multiple matching targets"),
      bodyTextIncludesLocalOnlyPromotionState:
        document.body.textContent.includes("local promotion artifact") ||
        document.body.textContent.includes("local PR bundle"),
      runActionHint: text("#runActionHint"),
      exportPrButtons: buttonStats("export-pr"),
      publishPrButtons: buttonStats("publish-pr"),
      draftPrButtons: buttonStats("draft-pr"),
    };
  });

  await page.screenshot({ path: screenshotPath, fullPage: true });
  await browser.close();

  const result = {
    gotoStatus: response && response.status(),
    consoleMessages,
    pageErrors,
    requestFailures,
    loadEvents,
    loadEventsAfterInitial,
    unexpectedLoadEvents: loadEvents - loadEventsAfterInitial,
    mainFrameNavigations,
    mainFrameNavigationsAfterInitial,
    unexpectedMainFrameNavigations: mainFrameNavigations - mainFrameNavigationsAfterInitial,
    summary,
    focusChecks,
    screenshotPath,
    ok: false,
  };

  const problems = [];
  if (!response || response.status() !== 200) {
    problems.push(`unexpected status ${response && response.status()}`);
  }
  if (summary.title !== "Catalyst Continuum Control Surface") {
    problems.push(`unexpected title ${summary.title}`);
  }
  if (!summary.lastRefresh.includes("WebSocket stream active")) {
    problems.push(`websocket not active: ${summary.lastRefresh}`);
  }
  if (summary.runCardCount < 1) {
    problems.push("no run cards visible");
  }
  if (summary.taskRows < 1) {
    problems.push("no task rows visible");
  }
  if (summary.artifactRows < 1) {
    problems.push("no artifact rows visible");
  }
  if (summary.eventItems < 1) {
    problems.push("no event items visible");
  }
  if (summary.agentLaneCount < 1) {
    problems.push("no agent lanes visible");
  }
  if (summary.agentReportCardCount < 1) {
    problems.push("no agent report cards visible");
  }
  for (const check of focusChecks) {
    if (!check.expected || check.actual !== check.expected) {
      problems.push(
        `focus was not preserved for ${check.surface}: expected ${check.expected}, got ${check.actual}`
      );
    }
  }
  if (summary.iframeCount !== 0) {
    problems.push(`unexpected iframe count ${summary.iframeCount}`);
  }
  if (expectRepositoryBootstrapGuidance && !summary.bodyTextIncludesRepositoryBootstrap) {
    problems.push("repository-target bootstrap guidance is missing from the UI");
  }
  if (expectRepositoryTargetPolicy && !summary.bodyTextIncludesRepositoryTargetPolicyState) {
    problems.push("run-level repository-target policy state is missing from the UI");
  }
  if (expectRemotePublicationBlocked) {
    if (summary.exportPrButtons.count < 1 || summary.exportPrButtons.enabledCount < 1) {
      problems.push("expected export-pr to stay enabled for local-only promotion");
    }
    if (
      summary.publishPrButtons.count < 1 ||
      summary.publishPrButtons.enabledCount !== 0 ||
      summary.publishPrButtons.disabledCount !== summary.publishPrButtons.count
    ) {
      problems.push("expected publish-pr to stay disabled under repository-target policy");
    }
    if (
      summary.draftPrButtons.count < 1 ||
      summary.draftPrButtons.enabledCount !== 0 ||
      summary.draftPrButtons.disabledCount !== summary.draftPrButtons.count
    ) {
      problems.push("expected draft-pr to stay disabled under repository-target policy");
    }
    if (!summary.bodyTextIncludesLocalOnlyPromotionState) {
      problems.push("expected local-only promotion guidance in the UI");
    }
  }
  if (result.unexpectedLoadEvents !== 0) {
    problems.push(`unexpected load events ${result.unexpectedLoadEvents}`);
  }
  if (result.unexpectedMainFrameNavigations !== 0) {
    problems.push(`unexpected main frame navigations ${result.unexpectedMainFrameNavigations}`);
  }
  if (consoleMessages.length) {
    problems.push(`console warnings/errors ${consoleMessages.length}`);
  }
  if (pageErrors.length) {
    problems.push(`page errors ${pageErrors.length}`);
  }
  if (requestFailures.length) {
    problems.push(`request failures ${requestFailures.length}`);
  }

  result.ok = problems.length === 0;
  result.problems = problems;
  fs.writeFileSync(summaryPath, `${JSON.stringify(result, null, 2)}\n`);
  console.log(JSON.stringify(result, null, 2));
  if (!result.ok) {
    process.exit(1);
  }
}

async function stableFocusKey(page) {
  return page.evaluate(() => {
    const active = document.activeElement;
    if (!active || typeof active.closest !== "function") {
      return "";
    }
    const stableElement = active.closest("[data-ui-stable-key]");
    return stableElement?.dataset?.uiStableKey || "";
  });
}

main().catch((error) => {
  fs.writeFileSync(
    summaryPath,
    `${JSON.stringify({ ok: false, error: error.stack || error.message }, null, 2)}\n`
  );
  console.error(error);
  process.exit(1);
});
NODE
}

cleanup() {
  if [ -n "${UI_PID:-}" ]; then
    kill "$UI_PID" >/dev/null 2>&1 || true
    wait "$UI_PID" >/dev/null 2>&1 || true
  fi
  if [ "${STARTED_POSTGRES:-0}" -eq 1 ] && [ -n "${POSTGRES_CONTAINER_NAME:-}" ]; then
    docker rm -f "$POSTGRES_CONTAINER_NAME" >/dev/null 2>&1 || true
  fi
  if [ -n "${SCRIPT_PID_FILE:-}" ]; then
    rm -f "$SCRIPT_PID_FILE"
  fi
  if [ -n "${TEMP_DIR:-}" ]; then
    rm -rf "$TEMP_DIR"
  fi
}

trap cleanup EXIT

SCENARIO="${OPERATOR_UI_SMOKE_SCENARIO:-mvp-cli-tool}"
HTTP_PORT="${OPERATOR_UI_SMOKE_HTTP_PORT:-}"
POSTGRES_PORT="${OPERATOR_UI_SMOKE_POSTGRES_PORT:-}"
OUTPUT_ROOT="${OPERATOR_UI_SMOKE_OUTPUT_ROOT:-$ROOT_DIR/.continuum/operator-ui-smoke}"
REPOSITORY_TARGETS_FILE="${OPERATOR_UI_SMOKE_REPOSITORY_TARGETS_FILE:-}"
EXPECT_REMOTE_PUBLICATION_BLOCKED=0
SKIP_BUILD=0

while [ "$#" -gt 0 ]; do
  case "$1" in
    --scenario)
      SCENARIO="${2:?missing value for --scenario}"
      shift 2
      ;;
    --http-port)
      HTTP_PORT="${2:?missing value for --http-port}"
      shift 2
      ;;
    --postgres-port)
      POSTGRES_PORT="${2:?missing value for --postgres-port}"
      shift 2
      ;;
    --output-root)
      OUTPUT_ROOT="${2:?missing value for --output-root}"
      shift 2
      ;;
    --repository-targets-file)
      REPOSITORY_TARGETS_FILE="${2:?missing value for --repository-targets-file}"
      shift 2
      ;;
    --expect-remote-publication-blocked)
      EXPECT_REMOTE_PUBLICATION_BLOCKED=1
      shift
      ;;
    --skip-build)
      SKIP_BUILD=1
      shift
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

require_command cargo
require_command curl
require_command docker
require_command node
require_command npm
require_command python3

if [ -n "$REPOSITORY_TARGETS_FILE" ] && [ ! -f "$REPOSITORY_TARGETS_FILE" ]; then
  echo "repository targets file not found: $REPOSITORY_TARGETS_FILE" >&2
  exit 1
fi

ORCHESTRATOR_TARGET_ROOT="$(resolve_cargo_target_root)"
BIN="${ORCHESTRATOR_TARGET_ROOT}/debug/catalyst-continuum-orchestrator"
RUN_TAG="operator-ui-smoke-$(date +%Y%m%d%H%M%S)-$$"
RUN_TAG="${RUN_TAG//[^a-zA-Z0-9_.-]/-}"
OUTPUT_DIR="$OUTPUT_ROOT/$RUN_TAG"
ARTIFACT_ROOT="$OUTPUT_DIR/artifacts"
PLAYWRIGHT_RUNNER_DIR="$ROOT_DIR/.continuum/operator-ui-smoke-playwright"
TEMP_DIR="$(mktemp -d)"
BROWSER_CHECK_FILE="$TEMP_DIR/operator-ui-browser-check.js"
POSTGRES_CONTAINER_NAME="continuum-${RUN_TAG}-postgres"
POSTGRES_IMAGE="${OPERATOR_UI_SMOKE_POSTGRES_IMAGE:-postgres:${POSTGRES_VERSION}@${POSTGRES_IMAGE_DIGEST}}"
POSTGRES_DB="${OPERATOR_UI_SMOKE_POSTGRES_DB:-continuum}"
POSTGRES_USER="${OPERATOR_UI_SMOKE_POSTGRES_USER:-continuum}"
POSTGRES_PASSWORD="${OPERATOR_UI_SMOKE_POSTGRES_PASSWORD:-continuum-dev}"
HELPER_PID_DIR="${CATALYST_LOCAL_HELPER_PID_DIR:-$ROOT_DIR/.continuum/local-helper-pids}"
SCRIPT_PID_FILE="$HELPER_PID_DIR/operator-ui-smoke-$$.launcher.pid"
UI_PID=""
STARTED_POSTGRES=0

if [ -z "$HTTP_PORT" ]; then
  HTTP_PORT="$(allocate_loopback_port)"
fi
if [ -z "$POSTGRES_PORT" ]; then
  POSTGRES_PORT="$(allocate_loopback_port)"
fi

mkdir -p "$OUTPUT_DIR" "$ARTIFACT_ROOT" "$HELPER_PID_DIR"
printf '%s\n' "$$" >"$SCRIPT_PID_FILE"
SEED_LOG_FILE="$OUTPUT_DIR/seed-smoke.log"
UI_LOG_FILE="$OUTPUT_DIR/operator-ui.log"
READYZ_FILE="$OUTPUT_DIR/readyz.json"
EXPECT_REPOSITORY_BOOTSTRAP_GUIDANCE=1

if [ -n "$REPOSITORY_TARGETS_FILE" ]; then
  EXPECT_REPOSITORY_BOOTSTRAP_GUIDANCE=0
fi

if [ "$SKIP_BUILD" -ne 1 ]; then
  log_phase "building orchestrator"
  cargo build --quiet --locked -p catalyst-continuum-orchestrator
fi

if [ ! -x "$BIN" ]; then
  echo "orchestrator binary not found: $BIN" >&2
  echo "run cargo build --workspace --locked or omit --skip-build" >&2
  exit 1
fi

ensure_playwright_runner
write_browser_check

log_phase "starting disposable Postgres on 127.0.0.1:${POSTGRES_PORT}"
docker rm -f "$POSTGRES_CONTAINER_NAME" >/dev/null 2>&1 || true
docker run -d \
  --name "$POSTGRES_CONTAINER_NAME" \
  --label io.catalyst-continuum.local-helper=true \
  --label io.catalyst-continuum.helper=operator-ui-smoke \
  -e POSTGRES_DB="$POSTGRES_DB" \
  -e POSTGRES_USER="$POSTGRES_USER" \
  -e POSTGRES_PASSWORD="$POSTGRES_PASSWORD" \
  -p "127.0.0.1:${POSTGRES_PORT}:5432" \
  --health-cmd "pg_isready -U ${POSTGRES_USER} -d ${POSTGRES_DB} -p 5432" \
  --health-interval 2s \
  --health-timeout 5s \
  --health-retries 30 \
  "$POSTGRES_IMAGE" >/dev/null
STARTED_POSTGRES=1
wait_for_postgres

DATABASE_URL="postgres://${POSTGRES_USER}:${POSTGRES_PASSWORD}@127.0.0.1:${POSTGRES_PORT}/${POSTGRES_DB}"

log_phase "seeding ${SCENARIO} run data through ci-smoke"
CI_SMOKE_SCENARIO="$SCENARIO" \
CATALYST_DATABASE_URL="$DATABASE_URL" \
CATALYST_ARTIFACT_ROOT="$ARTIFACT_ROOT" \
CATALYST_SKIP_WORKSPACE_BUILD=1 \
  "$ROOT_DIR/scripts/ci-smoke.sh" >"$SEED_LOG_FILE" 2>&1

log_phase "starting operator UI on http://127.0.0.1:${HTTP_PORT}/ui"
CATALYST_DATABASE_URL="$DATABASE_URL" \
CATALYST_ARTIFACT_ROOT="$ARTIFACT_ROOT" \
CATALYST_REPOSITORY_TARGETS_FILE="$REPOSITORY_TARGETS_FILE" \
  "$ROOT_DIR/scripts/run-operator-ui.sh" \
    --http-port "$HTTP_PORT" \
    --skip-build >"$UI_LOG_FILE" 2>&1 &
UI_PID="$!"
wait_for_ui_ready
verify_dashboard_repository_targets >"$OUTPUT_DIR/repository-targets-summary.txt"

log_phase "running browser interaction check"
PLAYWRIGHT_MODULE="$PLAYWRIGHT_RUNNER_DIR/node_modules/playwright" \
OPERATOR_UI_SMOKE_OUTPUT_DIR="$OUTPUT_DIR" \
OPERATOR_UI_SMOKE_URL="http://127.0.0.1:${HTTP_PORT}/ui" \
OPERATOR_UI_EXPECT_REPOSITORY_BOOTSTRAP_GUIDANCE="$EXPECT_REPOSITORY_BOOTSTRAP_GUIDANCE" \
OPERATOR_UI_EXPECT_REPOSITORY_TARGET_POLICY="${REPOSITORY_TARGETS_FILE:+1}" \
OPERATOR_UI_EXPECT_REMOTE_PUBLICATION_BLOCKED="$EXPECT_REMOTE_PUBLICATION_BLOCKED" \
  node "$BROWSER_CHECK_FILE"

log_phase "summary: $OUTPUT_DIR/operator-ui-browser-summary.json"
log_phase "screenshot: $OUTPUT_DIR/operator-ui-browser.png"
if [ -n "$REPOSITORY_TARGETS_FILE" ]; then
  log_phase "repository targets summary: $OUTPUT_DIR/repository-targets-summary.txt"
fi
