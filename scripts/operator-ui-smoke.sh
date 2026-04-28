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
  - installs the pinned Playwright runner and Chromium browser binary
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

ensure_playwright_browser() {
  local install_args=(install chromium)

  if [ "${OPERATOR_UI_SMOKE_INSTALL_BROWSER_DEPS:-0}" = "1" ]; then
    install_args=(install --with-deps chromium)
  fi

  npm exec \
    --prefix "$PLAYWRIGHT_RUNNER_DIR" \
    -- playwright "${install_args[@]}" >/dev/null
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

seed_github_issue_workbench_fixture() {
  python3 - "$OUTPUT_DIR" <<'PY'
import json
import os
import pathlib
import sys
import time

output_dir = pathlib.Path(sys.argv[1]).resolve()
workflow_dir = output_dir / "github-issue-workflows" / "operator-ui-issue-workflow"
planned_workflow_dir = output_dir / "github-issue-workflows" / "operator-ui-planned-workflow"
session_dir = workflow_dir / "session"
sync_dir = workflow_dir / "issue-sync"
planned_session_dir = planned_workflow_dir / "session"
session_dir.mkdir(parents=True, exist_ok=True)
sync_dir.mkdir(parents=True, exist_ok=True)
planned_session_dir.mkdir(parents=True, exist_ok=True)

run_summary = workflow_dir / "run-summary.json"
sync_plan = sync_dir / "github-issue-sync-plan.json"
sync_comment = sync_dir / "comment.md"
report = workflow_dir / "workflow-report.md"
summary_file = workflow_dir / "workflow-summary.json"
planned_report = planned_workflow_dir / "workflow-plan.md"
planned_summary_file = planned_workflow_dir / "workflow-summary.json"

for prompt_name, prompt_text in {
    "codex-prompt.md": "Codex should implement issue #42 with Catalyst evidence.\n",
    "cursor-prompt.md": "Cursor should implement issue #42 with Catalyst evidence.\n",
    "openhands-prompt.md": "OpenHands should implement issue #42 with Catalyst evidence.\n",
}.items():
    (session_dir / prompt_name).write_text(prompt_text, encoding="utf-8")

(session_dir / "manifest.json").write_text(
    json.dumps(
        {
            "session_type": "github_issue_session",
            "repo_path": str(output_dir / "target-repo"),
            "agent_prompts": {
                "codex": "codex-prompt.md",
                "cursor": "cursor-prompt.md",
                "openhands": "openhands-prompt.md",
            },
            "github_issue": {
                "repository_full_name": "smartit/operator-ui-issue-smoke",
                "number": 42,
                "title": "Polish issue workbench",
                "url": "https://github.com/smartit/operator-ui-issue-smoke/issues/42",
                "state": "open",
                "labels": [{"name": "ui"}, {"name": "alpha"}],
            },
            "repository_context": {
                "repo_path": str(output_dir / "target-repo"),
            },
        },
        indent=2,
    )
    + "\n",
    encoding="utf-8",
)
run_summary.write_text(json.dumps({"run_id": "operator-ui-smoke", "run_status": "succeeded"}, indent=2) + "\n", encoding="utf-8")
sync_plan.write_text(json.dumps({"actions": [{"kind": "comment"}, {"kind": "label"}]}, indent=2) + "\n", encoding="utf-8")
sync_comment.write_text("Catalyst Continuum prepared a draft PR and issue update.\n", encoding="utf-8")
report.write_text("# Workflow report\n\nThe issue workflow is ready for review.\n", encoding="utf-8")

summary_file.write_text(
    json.dumps(
        {
            "repository_full_name": "smartit/operator-ui-issue-smoke",
            "pr_strategy": "per-issue",
            "workflow_output_dir": str(workflow_dir),
            "repo_path": str(output_dir / "target-repo"),
            "session": {
                "dir": str(session_dir),
                "brief_file": str(session_dir / "brief.yaml"),
            },
            "report": {"markdown": str(report)},
            "run": {"summary_file": str(run_summary), "exit_code": 0},
            "draft_pr": {
                "requested": True,
                "exit_code": 0,
                "pr_url": "https://github.com/smartit/operator-ui-issue-smoke/pull/42",
            },
            "issue_sync": {
                "skipped": False,
                "applied": False,
                "status": "ready-for-review",
                "pr_url": "https://github.com/smartit/operator-ui-issue-smoke/pull/42",
                "plan": str(sync_plan),
                "comment": str(sync_comment),
                "exit_code": 0,
            },
            "plan": {"next_command": "make github-issue-run GITHUB_ISSUE_WORKFLOW_DIR=" + str(workflow_dir)},
        },
        indent=2,
    )
    + "\n",
    encoding="utf-8",
)

(planned_session_dir / "manifest.json").write_text(
    json.dumps(
        {
            "session_type": "github_issue_session",
            "github_issue": {
                "repository_full_name": "smartit/operator-ui-issue-smoke",
                "number": 43,
                "title": "Plan selected workbench",
                "url": "https://github.com/smartit/operator-ui-issue-smoke/issues/43",
                "state": "open",
                "labels": [{"name": "planning"}, {"name": "agent"}],
            },
            "repository_context": {
                "repo_path": str(output_dir / "target-repo"),
            },
        },
        indent=2,
    )
    + "\n",
    encoding="utf-8",
)
planned_report.write_text("# Planned workflow\n\nReview this plan before execution.\n", encoding="utf-8")
planned_summary_file.write_text(
    json.dumps(
        {
            "repository_full_name": "smartit/operator-ui-issue-smoke",
            "pr_strategy": "per-issue",
            "plan_only": True,
            "workflow_output_dir": str(planned_workflow_dir),
            "repo_path": str(output_dir / "target-repo"),
            "session": {
                "dir": str(planned_session_dir),
                "brief_file": str(planned_session_dir / "brief.yaml"),
            },
            "plan": {
                "markdown": str(planned_report),
                "next_command": "make github-issue-run GITHUB_ISSUE_WORKFLOW_DIR=" + str(planned_workflow_dir),
            },
            "issue_sync": {"skipped": True, "applied": False, "exit_code": 0},
        },
        indent=2,
    )
    + "\n",
    encoding="utf-8",
)

now = time.time()
os.utime(summary_file, (now, now))
os.utime(planned_summary_file, (now - 120, now - 120))

print(f"github_issue_workflow_fixture={workflow_dir}")
print(f"github_issue_planned_workflow_fixture={planned_workflow_dir}")
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
  await context.grantPermissions(["clipboard-read", "clipboard-write"], {
    origin: new URL(uiUrl).origin,
  });
  const page = await context.newPage();
  const consoleMessages = [];
  const pageErrors = [];
  const requestFailures = [];
  let loadEvents = 0;
  let mainFrameNavigations = 0;
  let lastMainFrameLocation = "";
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
      const currentUrl = new URL(frame.url());
      const locationKey = `${currentUrl.origin}${currentUrl.pathname}`;
      if (!lastMainFrameLocation || locationKey !== lastMainFrameLocation) {
        mainFrameNavigations += 1;
      }
      lastMainFrameLocation = locationKey;
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

  await page.waitForSelector("[data-ui-brief-example]", { timeout: 10000 });
  await page.click("[data-ui-brief-example]");
  await page.waitForFunction(() => {
    const badge = document.querySelector("#briefReadinessBadge");
    return badge && badge.textContent.includes("4/4 ready");
  }, { timeout: 10000 });

  await page.click("#runsList [data-run-id]");
  await page.waitForSelector("#runDetailShell");
  await page.click('[data-mission-tab="developer"]');
  await page.waitForSelector('[data-developer-handoff-panel="true"]', { timeout: 10000 });
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

  const eventFilterCheck = {
    count: await page.locator("[data-run-event-filter]").count(),
    handoffItems: 0,
    handoffIncludesDraftPr: false,
    restoredAllFilter: false,
  };
  if (eventFilterCheck.count > 1) {
    const handoffFilter = page.locator('[data-run-event-filter="handoff"]');
    const expectedFocusKey = await handoffFilter.getAttribute("data-ui-stable-key");
    await handoffFilter.click();
    await page.waitForFunction(() => {
      const filter = document.querySelector('[data-run-event-filter="handoff"]');
      return filter && filter.classList.contains("is-active");
    }, { timeout: 10000 });
    await page.waitForTimeout(250);
    eventFilterCheck.handoffItems = await page.locator("#eventTimeline article").count();
    eventFilterCheck.handoffIncludesDraftPr = await page.evaluate(() => {
      const timeline = document.querySelector("#eventTimeline");
      return Boolean(timeline && timeline.textContent.includes("Draft PR opened"));
    });
    focusChecks.push({
      surface: "run-event-filter",
      expected: expectedFocusKey,
      actual: await stableFocusKey(page),
    });

    await page.locator('[data-run-event-filter="all"]').click();
    await page.waitForFunction(() => {
      const filter = document.querySelector('[data-run-event-filter="all"]');
      return filter && filter.classList.contains("is-active");
    }, { timeout: 10000 });
    eventFilterCheck.restoredAllFilter = true;
  }

  await page.click('[data-mission-tab="developer"]');
  await page.waitForSelector('[data-developer-codex-command-card="true"]', { timeout: 10000 });
  await page.locator('[data-developer-review-prompt-copy="true"]').first().click();
  await page.waitForFunction(() => {
    const button = document.querySelector('[data-developer-review-prompt-copy="true"]');
    return button && button.textContent.includes("Prompt copied");
  }, { timeout: 5000 });
  const copiedReviewPrompt = await page.evaluate(() => navigator.clipboard.readText());
  await page.locator('[data-developer-evidence-paths-copy="true"]').first().click();
  await page.waitForFunction(() => {
    const button = document.querySelector('[data-developer-evidence-paths-copy="true"]');
    return button && button.textContent.includes("Paths copied");
  }, { timeout: 5000 });
  const copiedEvidencePaths = await page.evaluate(() => navigator.clipboard.readText());
  await page.locator('[data-developer-next-command-copy="true"]').first().click();
  await page.waitForFunction(() => {
    const button = document.querySelector('[data-developer-next-command-copy="true"]');
    return button && button.textContent.includes("Command copied");
  }, { timeout: 5000 });
  const copiedNextCommand = await page.evaluate(() => navigator.clipboard.readText());
  await page.locator('[data-developer-live-brief-copy="true"]').first().click();
  await page.waitForFunction(() => {
    const button = document.querySelector('[data-developer-live-brief-copy="true"]');
    return button && button.textContent.includes("Brief copied");
  }, { timeout: 5000 });
  const copiedLiveBrief = await page.evaluate(() => navigator.clipboard.readText());
  await page.locator('[data-developer-github-update-copy="true"]').first().click();
  await page.waitForFunction(() => {
    const button = document.querySelector('[data-developer-github-update-copy="true"]');
    return button && button.textContent.includes("Update copied");
  }, { timeout: 5000 });
  const copiedGithubUpdate = await page.evaluate(() => navigator.clipboard.readText());
  await page
    .locator('[data-developer-codex-command-card="true"] [data-copy-command]')
    .first()
    .click();
  await page.waitForFunction(() => {
    const button = document.querySelector(
      '[data-developer-codex-command-card="true"] [data-copy-command]'
    );
    return button && button.textContent.includes("Copied");
  }, { timeout: 5000 });
  const copiedCodexCommand = await page.evaluate(() => navigator.clipboard.readText());
  await page.waitForSelector('[data-github-issue-workflow-card="true"]', { timeout: 10000 });
  await page.locator('[data-github-issue-review-command-copy="true"]').first().click();
  await page.waitForFunction(() => {
    const button = document.querySelector('[data-github-issue-review-command-copy="true"]');
    return button && button.textContent.includes("Command copied");
  }, { timeout: 5000 });
  const copiedIssueReviewCommand = await page.evaluate(() => navigator.clipboard.readText());
  await page.locator('[data-github-issue-workflow-path-copy="true"]').first().click();
  await page.waitForFunction(() => {
    const button = document.querySelector('[data-github-issue-workflow-path-copy="true"]');
    return button && button.textContent.includes("Path copied");
  }, { timeout: 5000 });
  const copiedIssueWorkflowPath = await page.evaluate(() => navigator.clipboard.readText());
  await page.locator('[data-github-issue-session-manifest-copy="true"]').first().click();
  await page.waitForFunction(() => {
    const button = document.querySelector('[data-github-issue-session-manifest-copy="true"]');
    return button && button.textContent.includes("Path copied");
  }, { timeout: 5000 });
  const copiedIssueSessionManifest = await page.evaluate(() => navigator.clipboard.readText());
  await page.locator('[data-github-issue-evidence-packet-copy="true"]').first().click();
  await page.waitForFunction(() => {
    const button = document.querySelector('[data-github-issue-evidence-packet-copy="true"]');
    return button && button.textContent.includes("Evidence copied");
  }, { timeout: 5000 });
  const copiedIssueEvidencePacket = await page.evaluate(() => navigator.clipboard.readText());
  await page.locator('[data-github-issue-context-packet-copy="true"]').first().click();
  await page.waitForFunction(() => {
    const button = document.querySelector('[data-github-issue-context-packet-copy="true"]');
    return button && button.textContent.includes("Issue context copied");
  }, { timeout: 5000 });
  const copiedIssueContextPacket = await page.evaluate(() => navigator.clipboard.readText());
  await page.locator('[data-github-issue-status-update-copy="true"]').first().click();
  await page.waitForFunction(() => {
    const button = document.querySelector('[data-github-issue-status-update-copy="true"]');
    return button && button.textContent.includes("Status update copied");
  }, { timeout: 5000 });
  const copiedIssueStatusUpdate = await page.evaluate(() => navigator.clipboard.readText());
  await page.locator('[data-github-issue-recommended-handoff-copy="true"]').first().click();
  await page.waitForFunction(() => {
    const button = document.querySelector('[data-github-issue-recommended-handoff-copy="true"]');
    return button && button.textContent.includes("Recommended command copied");
  }, { timeout: 5000 });
  const copiedIssueRecommendedHandoffCommand = await page.evaluate(() => navigator.clipboard.readText());
  await page.locator('[data-github-issue-selected-package-runbook-copy="true"]').first().click();
  await page.waitForFunction(() => {
    const button = document.querySelector('[data-github-issue-selected-package-runbook-copy="true"]');
    return button && button.textContent.includes("Runbook copied");
  }, { timeout: 5000 });
  const copiedIssueSelectedPackageRunbook = await page.evaluate(() => navigator.clipboard.readText());
  await page.locator('[data-github-issue-runbook-copy="true"]').first().click();
  await page.waitForFunction(() => {
    const button = document.querySelector('[data-github-issue-runbook-copy="true"]');
    return button && button.textContent.includes("Runbook copied");
  }, { timeout: 5000 });
  const copiedIssueRunbook = await page.evaluate(() => navigator.clipboard.readText());
  await page.locator('[data-github-issue-next-command-copy="true"]').first().click();
  await page.waitForFunction(() => {
    const button = document.querySelector('[data-github-issue-next-command-copy="true"]');
    return button && button.textContent.includes("Command copied");
  }, { timeout: 5000 });
  const copiedIssueNextCommand = await page.evaluate(() => navigator.clipboard.readText());
  await page.locator('[data-github-issue-preflight-command-copy="true"]').first().click();
  await page.waitForFunction(() => {
    const button = document.querySelector('[data-github-issue-preflight-command-copy="true"]');
    return button && button.textContent.includes("Command copied");
  }, { timeout: 5000 });
  const copiedIssuePreflightCommand = await page.evaluate(() => navigator.clipboard.readText());
  await page.locator('[data-github-issue-sync-command-copy="true"]').first().click();
  await page.waitForFunction(() => {
    const button = document.querySelector('[data-github-issue-sync-command-copy="true"]');
    return button && button.textContent.includes("Command copied");
  }, { timeout: 5000 });
  const copiedIssueSyncCommand = await page.evaluate(() => navigator.clipboard.readText());
  await page.locator('[data-github-issue-sync-comment-text-copy="true"]').first().click();
  await page.waitForFunction(() => {
    const button = document.querySelector('[data-github-issue-sync-comment-text-copy="true"]');
    return button && button.textContent.includes("Comment copied");
  }, { timeout: 5000 });
  const copiedIssueSyncComment = await page.evaluate(() => navigator.clipboard.readText());
  await page.locator('[data-github-issue-sync-comment-path-copy="true"]').first().click();
  await page.waitForFunction(() => {
    const button = document.querySelector('[data-github-issue-sync-comment-path-copy="true"]');
    return button && button.textContent.includes("Path copied");
  }, { timeout: 5000 });
  const copiedIssueSyncCommentPath = await page.evaluate(() => navigator.clipboard.readText());
  await page.locator('[data-github-issue-agent-prompt-text-copy="true"]').first().click();
  await page.waitForFunction(() => {
    const button = document.querySelector('[data-github-issue-agent-prompt-text-copy="true"]');
    return button && button.textContent.includes("Prompt copied");
  }, { timeout: 5000 });
  const copiedIssueAgentPromptText = await page.evaluate(() => navigator.clipboard.readText());
  await page.locator('[data-github-issue-agent-prompt-copy="true"]').first().click();
  await page.waitForFunction(() => {
    const button = document.querySelector('[data-github-issue-agent-prompt-copy="true"]');
    return button && button.textContent.includes("Command copied");
  }, { timeout: 5000 });
  const copiedIssueAgentPromptCommand = await page.evaluate(() => navigator.clipboard.readText());
  await page.locator('[data-github-issue-agent-prompt-path-copy="true"]').first().click();
  await page.waitForFunction(() => {
    const button = document.querySelector('[data-github-issue-agent-prompt-path-copy="true"]');
    return button && button.textContent.includes("Path copied");
  }, { timeout: 5000 });
  const copiedIssueAgentPromptPath = await page.evaluate(() => navigator.clipboard.readText());
  await page.locator('[data-github-issue-agent-clipboard-copy="true"]').first().click();
  await page.waitForFunction(() => {
    const button = document.querySelector('[data-github-issue-agent-clipboard-copy="true"]');
    return button && button.textContent.includes("Command copied");
  }, { timeout: 5000 });
  const copiedIssueAgentClipboardCommand = await page.evaluate(() => navigator.clipboard.readText());
  await page.locator('[data-github-issue-codex-ui-copy="true"]').first().click();
  await page.waitForFunction(() => {
    const button = document.querySelector('[data-github-issue-codex-ui-copy="true"]');
    return button && button.textContent.includes("Command copied");
  }, { timeout: 5000 });
  const copiedIssueCodexUiCommand = await page.evaluate(() => navigator.clipboard.readText());
  await page
    .locator('[data-github-issue-workflow-select]')
    .filter({ hasText: "#43 Plan selected workbench" })
    .first()
    .click();
  await page.waitForFunction(() => {
    const selected = document.querySelector('[data-github-issue-workflow-selected="true"]');
    return selected && selected.textContent.includes("#43 Plan selected workbench");
  }, { timeout: 5000 });
  const selectedPlannedWorkflowText = await page
    .locator('[data-github-issue-workflow-selected="true"]')
    .first()
    .textContent();
  const plannedWorkflowUrl = page.url();
  const plannedRecommendedHandoffText = await page
    .locator('[data-github-issue-recommended-handoff="true"]')
    .first()
    .textContent();
  await page.locator('[data-github-issue-recommended-handoff-copy="true"]').first().click();
  await page.waitForFunction(() => {
    const button = document.querySelector('[data-github-issue-recommended-handoff-copy="true"]');
    return button && button.textContent.includes("Recommended command copied");
  }, { timeout: 5000 });
  const copiedIssuePlannedRecommendedCommand = await page.evaluate(() => navigator.clipboard.readText());
  await page.locator('[data-github-issue-preflight-command-copy="true"]').first().click();
  await page.waitForFunction(() => {
    const button = document.querySelector('[data-github-issue-preflight-command-copy="true"]');
    return button && button.textContent.includes("Command copied");
  }, { timeout: 5000 });
  const copiedIssuePlannedPreflightCommand = await page.evaluate(() => navigator.clipboard.readText());
  await page.locator('[data-github-issue-review-command-copy="true"]').first().click();
  const copiedIssuePlannedReviewCommand = await page.evaluate(() => navigator.clipboard.readText());
  await page.locator('[data-github-issue-next-command-copy="true"]').first().click();
  const copiedIssuePlannedCommand = await page.evaluate(() => navigator.clipboard.readText());
  await page
    .locator('[data-github-issue-workflow-select]')
    .filter({ hasText: "#42 Polish issue workbench" })
    .first()
    .click();
  await page.waitForFunction(() => {
    const selected = document.querySelector('[data-github-issue-workflow-selected="true"]');
    return selected && selected.textContent.includes("#42 Polish issue workbench");
  }, { timeout: 5000 });
  const latestWorkflowUrl = page.url();

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
    const issueDraftPrLink = document.querySelector('[data-github-issue-draft-pr-link="true"]');
    const issueLink = document.querySelector('[data-github-issue-link="true"]');

    return {
      title: document.title,
      heroHeading: document.querySelector("h1")?.textContent?.trim() || "",
      firstRunPlaybookHeading: text("#first-run-playbook h2"),
      firstRunPlaybookStepCount: count("#first-run-playbook li"),
      briefReadinessItemCount: count("[data-brief-readiness-item]"),
      briefReadinessBadge: text("#briefReadinessBadge"),
      briefReadinessIncludesRepository:
        document.body.textContent.includes("Repository target"),
      currentFocusCardCount: count('[data-pulse-card="current-focus"]'),
      currentFocusHeading: text('[data-pulse-card="current-focus"] h3'),
      currentFocusActionHref:
        document.querySelector('[data-pulse-card="current-focus"] a')?.getAttribute("href") || "",
      operatorDockCardCount: count("[data-operator-dock-card]"),
      operatorDockPosition:
        window.getComputedStyle(document.querySelector("#operator-dock")).position,
      operatorDockIncludesSelectedRun:
        document.body.textContent.includes("Selected run"),
      operatorDockIncludesNextStep:
        document.body.textContent.includes("Next step"),
      githubIssueWorkbenchCount: count('[data-github-issue-workbench="true"]'),
      githubIssueWorkflowCardCount: count('[data-github-issue-workflow-card="true"]'),
      githubIssueWorkflowPostureRowCount: count('[data-github-issue-workflow-posture-row="true"]'),
      githubIssueWorkflowPosturePillCount: count('[data-github-issue-workflow-posture-pill="true"]'),
      githubIssueWorkflowPostureText:
        Array.from(document.querySelectorAll('[data-github-issue-workflow-posture-pill="true"]'))
          .map((element) => element.textContent.trim())
          .join(" | "),
      githubIssueSelectedPackageCount: count('[data-github-issue-selected-package="true"]'),
      githubIssueSelectedPackageMetaCount: count('[data-github-issue-selected-package-meta="true"]'),
      githubIssueWorkflowPathCopyButtonCount: count('[data-github-issue-workflow-path-copy="true"]'),
      githubIssueSessionManifestCopyButtonCount: count('[data-github-issue-session-manifest-copy="true"]'),
      githubIssueEvidencePacketCopyButtonCount: count('[data-github-issue-evidence-packet-copy="true"]'),
      githubIssueContextPacketCopyButtonCount: count('[data-github-issue-context-packet-copy="true"]'),
      githubIssueStatusUpdateCopyButtonCount: count('[data-github-issue-status-update-copy="true"]'),
      githubIssueSelectedPackageRunbookCopyButtonCount:
        count('[data-github-issue-selected-package-runbook-copy="true"]'),
      githubIssueRecommendedHandoffCount: count('[data-github-issue-recommended-handoff="true"]'),
      githubIssueRecommendedHandoffMetaCount: count('[data-github-issue-recommended-handoff-meta="true"]'),
      githubIssueRecommendedHandoffCopyButtonCount:
        count('[data-github-issue-recommended-handoff-copy="true"]'),
      githubIssueRecommendedHandoffCommandText: text('[data-github-issue-recommended-handoff-command="true"]'),
      githubIssueRecommendedHandoffIncludesCodex:
        document.body.textContent.includes("Recommended next action") &&
        document.body.textContent.includes("Continue in Codex UI") &&
        document.body.textContent.includes("Codex UI bridge") &&
        document.body.textContent.includes("Best handoff"),
      githubIssueRunbookCardCount: count('[data-github-issue-runbook-card="true"]'),
      githubIssueRunbookPreviewCount: count('[data-github-issue-runbook-preview="true"]'),
      githubIssueRunbookCopyButtonCount: count('[data-github-issue-runbook-copy="true"]'),
      githubIssueDraftPrLinkCount: count('[data-github-issue-draft-pr-link="true"]'),
      githubIssueDraftPrLinkHref: issueDraftPrLink?.getAttribute("href") || "",
      githubIssueDraftPrLinkTarget: issueDraftPrLink?.getAttribute("target") || "",
      githubIssueDraftPrLinkRel: issueDraftPrLink?.getAttribute("rel") || "",
      githubIssueDeveloperPathCount: count('[data-github-issue-developer-path="true"]'),
      githubIssueDeveloperPathStepCount: count('[data-github-issue-developer-path-step="true"]'),
      githubIssueDeveloperPathIncludesNativeAgent:
        document.body.textContent.includes("How this becomes useful work") &&
        document.body.textContent.includes("Run strict preflight") &&
        document.body.textContent.includes("Continue in Codex, Cursor, or OpenHands") &&
        document.body.textContent.includes("Review PR evidence and sync the issue"),
      githubIssueProgressStepCount: count('[data-github-issue-progress-step="true"]'),
      githubIssuePreflightCardCount: count('[data-github-issue-preflight-action="true"]'),
      githubIssuePreflightCommandCopyButtonCount: count('[data-github-issue-preflight-command-copy="true"]'),
      githubIssueReviewCommandCopyButtonCount: count('[data-github-issue-review-command-copy="true"]'),
      githubIssueItemCount: count('[data-github-issue-item="true"]'),
      githubIssueLinkCount: count('[data-github-issue-link="true"]'),
      githubIssueLinkHref: issueLink?.getAttribute("href") || "",
      githubIssueLinkTarget: issueLink?.getAttribute("target") || "",
      githubIssueLinkRel: issueLink?.getAttribute("rel") || "",
      githubIssueAgentHandoffPanelCount: count('[data-github-issue-agent-handoff="true"]'),
      githubIssueAgentPromptCardCount: count('[data-github-issue-agent-prompt="true"]'),
      githubIssueAgentPromptMetaCount: count('[data-github-issue-agent-prompt-meta="true"]'),
      githubIssueAgentPromptPreviewCount: count('[data-github-issue-agent-prompt-preview="true"]'),
      githubIssueAgentPromptTextCopyButtonCount: count('[data-github-issue-agent-prompt-text-copy="true"]'),
      githubIssueAgentPromptCopyButtonCount: count('[data-github-issue-agent-prompt-copy="true"]'),
      githubIssueAgentPromptPathCopyButtonCount: count('[data-github-issue-agent-prompt-path-copy="true"]'),
      githubIssueAgentClipboardCopyButtonCount: count('[data-github-issue-agent-clipboard-copy="true"]'),
      githubIssueCodexUiCopyButtonCount: count('[data-github-issue-codex-ui-copy="true"]'),
      githubIssueNextCommandCopyButtonCount: count('[data-github-issue-next-command-copy="true"]'),
      githubIssueSyncCommandCopyButtonCount: count('[data-github-issue-sync-command-copy="true"]'),
      githubIssueSyncCommentCardCount: count('[data-github-issue-sync-comment-card="true"]'),
      githubIssueSyncCommentMetaCount: count('[data-github-issue-sync-comment-meta="true"]'),
      githubIssueSyncCommentPreviewCount: count('[data-github-issue-sync-comment-preview="true"]'),
      githubIssueSyncCommentTextCopyButtonCount: count('[data-github-issue-sync-comment-text-copy="true"]'),
      githubIssueSyncCommentPathCopyButtonCount: count('[data-github-issue-sync-comment-path-copy="true"]'),
      githubIssueSelectedWorkflowText: text('[data-github-issue-workflow-selected="true"]'),
      githubIssueWorkbenchIncludesIssue:
        document.body.textContent.includes("GitHub Issue Workbench") &&
        document.body.textContent.includes("#42 Polish issue workbench") &&
        document.body.textContent.includes("#43 Plan selected workbench") &&
        document.body.textContent.includes("smartit/operator-ui-issue-smoke"),
      githubIssueWorkbenchIncludesAgentHandoff:
        document.body.textContent.includes("Continue in the native agent UI") &&
        document.body.textContent.includes("Codex") &&
        document.body.textContent.includes("Cursor") &&
        document.body.textContent.includes("OpenHands") &&
        document.body.textContent.includes("Codex should implement issue #42") &&
        document.body.textContent.includes("browser copy ready") &&
        document.body.textContent.includes("make github-issue-agent-prompt"),
      missionContextCardCount: count('[data-mission-context-card="true"]'),
      missionContextIncludesApprovalBoundary:
        document.body.textContent.includes("GitHub remains the human approval boundary"),
      developerHandoffPanelCount: count('[data-developer-handoff-panel="true"]'),
      developerNextCommandPanelCount: count('[data-developer-next-command-panel="true"]'),
      developerNextCommandCopyButtonCount: count('[data-developer-next-command-copy="true"]'),
      developerNextCommandIncludesRunId:
        document.body.textContent.includes("Next terminal command") &&
        document.querySelector('[data-developer-next-command-text="true"]')?.textContent.includes("/runs/"),
      developerLiveBriefPanelCount: count('[data-developer-live-brief-panel="true"]'),
      developerLiveBriefCopyButtonCount: count('[data-developer-live-brief-copy="true"]'),
      developerLiveBriefIncludesSummary:
        document.body.textContent.includes("Live run brief") &&
        document.body.textContent.includes("Share the current run state") &&
        document.querySelector('[data-developer-live-brief-text="true"]')?.textContent.includes("Next safe action:"),
      developerGithubUpdatePanelCount: count('[data-developer-github-update-panel="true"]'),
      developerGithubUpdateCopyButtonCount: count('[data-developer-github-update-copy="true"]'),
      developerGithubUpdateIncludesSummary:
        document.body.textContent.includes("GitHub update") &&
        document.body.textContent.includes("Copy a concise issue or PR status comment") &&
        document.querySelector('[data-developer-github-update-text="true"]')?.textContent.includes("PR handoff:"),
      developerReviewPromptPanelCount: count('[data-developer-review-prompt-panel="true"]'),
      developerReviewPromptCopyButtonCount: count('[data-developer-review-prompt-copy="true"]'),
      developerReviewPromptIncludesAgent:
        document.body.textContent.includes("Bring Continuum evidence into Cursor, Codex, or OpenHands") &&
        document.body.textContent.includes("Review this Catalyst Continuum run before I trust or merge"),
      developerEvidencePacketPanelCount: count('[data-developer-evidence-packet-panel="true"]'),
      developerEvidencePathsCopyButtonCount: count('[data-developer-evidence-paths-copy="true"]'),
      developerEvidencePacketIncludesPaths:
        document.body.textContent.includes("Evidence packet") &&
        document.body.textContent.includes("Copy the exact files a reviewer should open first") &&
        document.body.textContent.includes("developer_handoff:"),
      developerCodexPanelCount: count('[data-developer-codex-panel="true"]'),
      developerCodexCommandCardCount: count('[data-developer-codex-command-card="true"]'),
      developerCodexIncludesCommand:
        document.body.textContent.includes("make codex-app-server-run") &&
        document.body.textContent.includes("CODEX_APP_SERVER_PROMPT_FILE="),
      developerValueCardCount: count("[data-developer-value-card]"),
      developerReviewItemCount: count("[data-developer-review-item]"),
      developerEvidenceCardCount: count("[data-developer-evidence-card]"),
      developerAgentCardCount: count("[data-developer-agent-card]"),
      developerTabIncludesValue:
        document.body.textContent.includes("What this gives a developer") &&
        document.body.textContent.includes("One audit trail across tools"),
      missionFreshnessCardCount: count('[data-mission-freshness-card="true"]'),
      missionFreshnessLatest: text('[data-mission-freshness-latest="true"]'),
      missionFreshnessIncludesQuality:
        document.body.textContent.includes("Quality evidence is fresh") ||
        document.body.textContent.includes("Quality evidence may be stale") ||
        document.body.textContent.includes("Quality evidence is missing"),
      runGuideProgressNow:
        document.querySelector("#runGuideProgressMeter")?.getAttribute("aria-valuenow") || "",
      runGuideProgressMax:
        document.querySelector("#runGuideProgressMeter")?.getAttribute("aria-valuemax") || "",
      runSectionNavLinkCount: count("#runSectionNav a"),
      runSectionNavBadgeCount: count("[data-run-section-count]"),
      runSectionNavIncludesArtifacts:
        Array.from(document.querySelectorAll("#runSectionNav a")).some(
          (link) => link.getAttribute("href") === "#run-artifacts"
        ),
      runSectionNavGuideText: text('[data-run-section-count="guide"]'),
      runSectionNavTaskText: text('[data-run-section-count="tasks"]'),
      runSectionNavArtifactText: text('[data-run-section-count="artifacts"]'),
      runSectionNavEventText: text('[data-run-section-count="events"]'),
      lastRefresh: text("#lastRefresh"),
      selectedRunLabel: text("#selectedRunLabel"),
      runOutcomeBannerCount: count("[data-run-outcome-banner]"),
      runOutcomeProofCount: count("[data-run-outcome-proof]"),
      runOutcomeIncludesDeliveryFlow:
        document.body.textContent.includes("Selected run outcome") &&
        document.body.textContent.includes("Delivery flow"),
      runCardCount: count("#runsList [data-run-id]"),
      runCardNextStepCount: count("[data-run-card-next-step]"),
      runCardPhaseRailCount: count("[data-run-card-phase-rail]"),
      runCardPhaseStepCount: count("[data-run-card-phase-step]"),
      runCardPhaseIncludesHandoff:
        document.body.textContent.includes("PR handoff"),
      runCardTaskMeterCount: count("[data-run-card-task-meter]"),
      runCardTaskMeterSegmentCount: count("[data-run-card-task-meter-segment]"),
      taskRows: count("#taskTableWrap tbody tr"),
      artifactEvidenceCardCount: count("[data-artifact-evidence-card]"),
      artifactEvidenceIncludesPlanning:
        document.body.textContent.includes("Planning evidence"),
      artifactRows: count("#artifactTableWrap tbody tr"),
      eventItems: count("#eventTimeline article"),
      eventHumanTitleCount: count("[data-run-event-human-title]"),
      eventTimelineIncludesHumanTitle:
        document.body.textContent.includes("Draft PR opened") ||
        document.body.textContent.includes("Task succeeded") ||
        document.body.textContent.includes("Quality gate evaluated"),
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
      runControlReadinessCardCount: count("[data-run-control-readiness-card]"),
      runControlReadinessDeveloperHandoffCount: count('[data-run-control-action="developer-handoff"]'),
      runControlReadinessDraftPrCount: count('[data-run-control-action="draft-pr"]'),
      runControlReadinessIncludesGuard:
        document.body.textContent.includes("Guard passed") ||
        document.body.textContent.includes("No queued tasks remain"),
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
    eventFilterCheck,
    copiedReviewPrompt,
    copiedEvidencePaths,
    copiedNextCommand,
    copiedLiveBrief,
    copiedGithubUpdate,
    copiedCodexCommand,
    copiedIssueReviewCommand,
    copiedIssueWorkflowPath,
    copiedIssueSessionManifest,
    copiedIssueEvidencePacket,
    copiedIssueContextPacket,
    copiedIssueStatusUpdate,
    copiedIssueRecommendedHandoffCommand,
    copiedIssueSelectedPackageRunbook,
    copiedIssueRunbook,
    copiedIssueNextCommand,
    copiedIssuePreflightCommand,
    copiedIssueSyncCommand,
    copiedIssueSyncComment,
    copiedIssueSyncCommentPath,
    copiedIssueAgentPromptText,
    copiedIssueAgentPromptCommand,
    copiedIssueAgentPromptPath,
    copiedIssueAgentClipboardCommand,
    copiedIssueCodexUiCommand,
    copiedIssuePlannedCommand,
    copiedIssuePlannedRecommendedCommand,
    copiedIssuePlannedPreflightCommand,
    copiedIssuePlannedReviewCommand,
    selectedPlannedWorkflowText,
    plannedRecommendedHandoffText,
    plannedWorkflowUrl,
    latestWorkflowUrl,
    ok: false,
  };

  const problems = [];
  if (!response || response.status() !== 200) {
    problems.push(`unexpected status ${response && response.status()}`);
  }
  if (summary.title !== "Catalyst Continuum Control Surface") {
    problems.push(`unexpected title ${summary.title}`);
  }
  if (summary.heroHeading !== "Turn a product brief into a draft pull request") {
    problems.push(`unexpected hero heading ${summary.heroHeading}`);
  }
  if (summary.firstRunPlaybookHeading !== "If you are new, follow this exact order") {
    problems.push(`first-run playbook heading is missing: ${summary.firstRunPlaybookHeading}`);
  }
  if (summary.firstRunPlaybookStepCount !== 4) {
    problems.push(`expected 4 first-run playbook steps, got ${summary.firstRunPlaybookStepCount}`);
  }
  if (summary.briefReadinessItemCount !== 4) {
    problems.push(`expected 4 brief readiness items, got ${summary.briefReadinessItemCount}`);
  }
  if (!summary.briefReadinessBadge) {
    problems.push("brief readiness badge is missing");
  }
  if (!summary.briefReadinessBadge.includes("4/4 ready")) {
    problems.push(`starter brief should make readiness complete, got ${summary.briefReadinessBadge}`);
  }
  if (!summary.briefReadinessIncludesRepository) {
    problems.push("brief readiness checklist should explain repository target metadata");
  }
  if (summary.currentFocusCardCount !== 1) {
    problems.push(`expected one current-focus pulse card, got ${summary.currentFocusCardCount}`);
  }
  if (!summary.currentFocusHeading) {
    problems.push("current-focus pulse card heading is missing");
  }
  if (summary.currentFocusActionHref !== "#run-detail") {
    problems.push(`current-focus pulse action should target #run-detail, got ${summary.currentFocusActionHref}`);
  }
  if (summary.operatorDockCardCount !== 4) {
    problems.push(`expected four operator dock cards, got ${summary.operatorDockCardCount}`);
  }
  if (summary.operatorDockPosition !== "sticky") {
    problems.push(`operator dock should stay sticky on desktop, got ${summary.operatorDockPosition}`);
  }
  if (!summary.operatorDockIncludesSelectedRun) {
    problems.push("operator dock should keep selected-run context visible");
  }
  if (!summary.operatorDockIncludesNextStep) {
    problems.push("operator dock should keep the next operator step visible");
  }
  if (summary.githubIssueWorkbenchCount !== 1) {
    problems.push(`expected one GitHub issue workbench, got ${summary.githubIssueWorkbenchCount}`);
  }
  if (summary.githubIssueWorkflowCardCount < 2) {
    problems.push("GitHub issue workbench should show at least two workflow cards");
  }
  if (summary.githubIssueWorkflowPostureRowCount !== summary.githubIssueWorkflowCardCount) {
    problems.push(
      `expected one posture row per GitHub issue workflow card, got ${summary.githubIssueWorkflowPostureRowCount} for ${summary.githubIssueWorkflowCardCount} cards`
    );
  }
  if (summary.githubIssueWorkflowPosturePillCount < summary.githubIssueWorkflowCardCount * 5) {
    problems.push(
      `expected five posture pills per GitHub issue workflow card, got ${summary.githubIssueWorkflowPosturePillCount}`
    );
  }
  if (
    !summary.githubIssueWorkflowPostureText.includes("draft PR ready") ||
    !summary.githubIssueWorkflowPostureText.includes("sync ready") ||
    !summary.githubIssueWorkflowPostureText.includes("agent prompts ready") ||
    !summary.githubIssueWorkflowPostureText.includes("plan only")
  ) {
    problems.push("GitHub issue workflow cards should expose draft PR, issue-sync, agent-prompt, and plan-only posture badges");
  }
  if (summary.githubIssueSelectedPackageCount !== 1) {
    problems.push(`expected one selected GitHub issue package, got ${summary.githubIssueSelectedPackageCount}`);
  }
  if (summary.githubIssueSelectedPackageMetaCount !== 1) {
    problems.push(`expected selected package evidence metadata, got ${summary.githubIssueSelectedPackageMetaCount}`);
  }
  if (summary.githubIssueWorkflowPathCopyButtonCount !== 1) {
    problems.push(`expected one workflow-dir copy button, got ${summary.githubIssueWorkflowPathCopyButtonCount}`);
  }
  if (summary.githubIssueSessionManifestCopyButtonCount !== 1) {
    problems.push(`expected one session-manifest copy button, got ${summary.githubIssueSessionManifestCopyButtonCount}`);
  }
  if (summary.githubIssueEvidencePacketCopyButtonCount !== 1) {
    problems.push(`expected one GitHub issue evidence packet copy button, got ${summary.githubIssueEvidencePacketCopyButtonCount}`);
  }
  if (summary.githubIssueContextPacketCopyButtonCount !== 1) {
    problems.push(`expected one GitHub issue context packet copy button, got ${summary.githubIssueContextPacketCopyButtonCount}`);
  }
  if (summary.githubIssueStatusUpdateCopyButtonCount !== 1) {
    problems.push(`expected one GitHub issue status-update copy button, got ${summary.githubIssueStatusUpdateCopyButtonCount}`);
  }
  if (summary.githubIssueSelectedPackageRunbookCopyButtonCount !== 1) {
    problems.push(
      `expected one selected-package runbook shortcut, got ${summary.githubIssueSelectedPackageRunbookCopyButtonCount}`
    );
  }
  if (summary.githubIssueRecommendedHandoffCount !== 1) {
    problems.push(`expected one recommended handoff card, got ${summary.githubIssueRecommendedHandoffCount}`);
  }
  if (summary.githubIssueRecommendedHandoffMetaCount !== 1) {
    problems.push(`expected one recommended handoff metadata row, got ${summary.githubIssueRecommendedHandoffMetaCount}`);
  }
  if (summary.githubIssueRecommendedHandoffCopyButtonCount !== 1) {
    problems.push(
      `expected one recommended handoff copy button, got ${summary.githubIssueRecommendedHandoffCopyButtonCount}`
    );
  }
  if (!summary.githubIssueRecommendedHandoffIncludesCodex) {
    problems.push("recommended action should explain the Codex UI bridge as the best handoff");
  }
  if (
    !summary.githubIssueRecommendedHandoffCommandText.includes("make github-issue-codex-ui") ||
    !summary.githubIssueRecommendedHandoffCommandText.includes("GITHUB_ISSUE_WORKFLOW_DIR=")
  ) {
    problems.push("recommended handoff should show the Codex UI continuation command");
  }
  if (summary.githubIssueRunbookCardCount !== 1) {
    problems.push(`expected one GitHub issue runbook card, got ${summary.githubIssueRunbookCardCount}`);
  }
  if (summary.githubIssueRunbookPreviewCount !== 1) {
    problems.push(`expected one GitHub issue runbook preview, got ${summary.githubIssueRunbookPreviewCount}`);
  }
  if (summary.githubIssueRunbookCopyButtonCount !== 1) {
    problems.push(`expected one GitHub issue runbook copy button, got ${summary.githubIssueRunbookCopyButtonCount}`);
  }
  if (summary.githubIssueDraftPrLinkCount !== 1) {
    problems.push(`expected one GitHub issue draft PR link, got ${summary.githubIssueDraftPrLinkCount}`);
  }
  if (summary.githubIssueDraftPrLinkHref !== "https://github.com/smartit/operator-ui-issue-smoke/pull/42") {
    problems.push(`unexpected GitHub issue draft PR link href ${summary.githubIssueDraftPrLinkHref}`);
  }
  if (summary.githubIssueDraftPrLinkTarget !== "_blank") {
    problems.push(`GitHub issue draft PR link should open a new tab, got ${summary.githubIssueDraftPrLinkTarget}`);
  }
  if (
    !summary.githubIssueDraftPrLinkRel.includes("noreferrer") ||
    !summary.githubIssueDraftPrLinkRel.includes("noopener")
  ) {
    problems.push(`GitHub issue draft PR link should use noopener/noreferrer, got ${summary.githubIssueDraftPrLinkRel}`);
  }
  if (summary.githubIssueDeveloperPathCount !== 1) {
    problems.push(`expected one GitHub issue developer path card, got ${summary.githubIssueDeveloperPathCount}`);
  }
  if (summary.githubIssueDeveloperPathStepCount !== 4) {
    problems.push(`expected four GitHub issue developer path steps, got ${summary.githubIssueDeveloperPathStepCount}`);
  }
  if (!summary.githubIssueDeveloperPathIncludesNativeAgent) {
    problems.push("GitHub issue developer path should explain preflight, native-agent handoff, and PR/issue sync order");
  }
  if (summary.githubIssueProgressStepCount !== 4) {
    problems.push(`expected four GitHub issue workflow readiness steps, got ${summary.githubIssueProgressStepCount}`);
  }
  if (summary.githubIssuePreflightCardCount !== 1) {
    problems.push(`expected one GitHub issue preflight card, got ${summary.githubIssuePreflightCardCount}`);
  }
  if (summary.githubIssuePreflightCommandCopyButtonCount !== 1) {
    problems.push(
      `expected one GitHub issue preflight command copy button, got ${summary.githubIssuePreflightCommandCopyButtonCount}`
    );
  }
  if (summary.githubIssueReviewCommandCopyButtonCount !== 1) {
    problems.push(
      `expected one GitHub issue review command copy button, got ${summary.githubIssueReviewCommandCopyButtonCount}`
    );
  }
  if (summary.githubIssueItemCount < 1) {
    problems.push("selected GitHub issue package should show at least one issue");
  }
  if (summary.githubIssueLinkCount !== 1) {
    problems.push(`expected one selected GitHub issue link, got ${summary.githubIssueLinkCount}`);
  }
  if (summary.githubIssueLinkHref !== "https://github.com/smartit/operator-ui-issue-smoke/issues/42") {
    problems.push(`unexpected selected GitHub issue link href ${summary.githubIssueLinkHref}`);
  }
  if (summary.githubIssueLinkTarget !== "_blank") {
    problems.push(`selected GitHub issue link should open a new tab, got ${summary.githubIssueLinkTarget}`);
  }
  if (
    !summary.githubIssueLinkRel.includes("noreferrer") ||
    !summary.githubIssueLinkRel.includes("noopener")
  ) {
    problems.push(`selected GitHub issue link should use noopener/noreferrer, got ${summary.githubIssueLinkRel}`);
  }
  if (!copiedIssueContextPacket.includes("#42 Polish issue workbench")) {
    problems.push("copied GitHub issue context packet should include the selected issue");
  }
  if (
    !copiedIssueContextPacket.includes("https://github.com/smartit/operator-ui-issue-smoke/issues/42")
  ) {
    problems.push("copied GitHub issue context packet should include the source issue URL");
  }
  if (!copiedIssueContextPacket.includes("make github-issue-agent-prompt")) {
    problems.push("copied GitHub issue context packet should include native-agent prompt commands");
  }
  if (!copiedIssueContextPacket.includes("untrusted context")) {
    problems.push("copied GitHub issue context packet should preserve the untrusted-context warning");
  }
  if (!copiedIssueStatusUpdate.includes("## Catalyst issue workflow update")) {
    problems.push("copied GitHub issue status update should include its heading");
  }
  if (!copiedIssueStatusUpdate.includes("Workflow status: Succeeded")) {
    problems.push("copied GitHub issue status update should include the workflow status");
  }
  if (!copiedIssueStatusUpdate.includes("Issue sync: ready-for-review pending")) {
    problems.push("copied GitHub issue status update should include pending issue-sync posture");
  }
  if (!copiedIssueStatusUpdate.includes("Draft PR: https://github.com/smartit/operator-ui-issue-smoke/pull/42")) {
    problems.push("copied GitHub issue status update should include draft PR evidence");
  }
  if (!copiedIssueStatusUpdate.includes("make github-issue-review")) {
    problems.push("copied GitHub issue status update should include the review command");
  }
  if (
    !copiedIssueRunbook.includes("Catalyst GitHub issue workflow runbook") ||
    !copiedIssueRunbook.includes("#42 Polish issue workbench") ||
    !copiedIssueRunbook.includes("make github-issue-preflight-strict") ||
    !copiedIssueRunbook.includes("make github-issue-agent-prompt") ||
    !copiedIssueRunbook.includes("make github-issue-sync") ||
    !copiedIssueRunbook.includes("https://github.com/smartit/operator-ui-issue-smoke/pull/42") ||
    !copiedIssueRunbook.includes("GitHub human review still wins")
  ) {
    problems.push("copied GitHub issue runbook should include the ordered commands, PR evidence, and safety boundary");
  }
  if (copiedIssueSelectedPackageRunbook !== copiedIssueRunbook) {
    problems.push("selected package runbook shortcut should copy the same ordered runbook as the runbook card");
  }
  if (
    !copiedIssueRecommendedHandoffCommand.includes("make github-issue-codex-ui") ||
    !copiedIssueRecommendedHandoffCommand.includes("GITHUB_ISSUE_WORKFLOW_DIR=") ||
    !copiedIssueRecommendedHandoffCommand.includes("REPO_PATH=")
  ) {
    problems.push("recommended handoff copy should write the Codex app-server continuation command");
  }
  if (summary.githubIssueAgentHandoffPanelCount !== 1) {
    problems.push(`expected one GitHub issue agent handoff panel, got ${summary.githubIssueAgentHandoffPanelCount}`);
  }
  if (summary.githubIssueAgentPromptCardCount !== 3) {
    problems.push(`expected three GitHub issue agent prompt cards, got ${summary.githubIssueAgentPromptCardCount}`);
  }
  if (summary.githubIssueAgentPromptMetaCount !== 3) {
    problems.push(`expected three GitHub issue agent prompt metadata rows, got ${summary.githubIssueAgentPromptMetaCount}`);
  }
  if (summary.githubIssueAgentPromptPreviewCount !== 3) {
    problems.push(`expected three GitHub issue agent prompt previews, got ${summary.githubIssueAgentPromptPreviewCount}`);
  }
  if (summary.githubIssueAgentPromptTextCopyButtonCount !== 3) {
    problems.push(
      `expected three GitHub issue agent prompt text copy buttons, got ${summary.githubIssueAgentPromptTextCopyButtonCount}`
    );
  }
  if (summary.githubIssueAgentPromptCopyButtonCount !== 3) {
    problems.push(
      `expected three GitHub issue agent prompt copy buttons, got ${summary.githubIssueAgentPromptCopyButtonCount}`
    );
  }
  if (summary.githubIssueAgentPromptPathCopyButtonCount !== 3) {
    problems.push(
      `expected three GitHub issue agent prompt path copy buttons, got ${summary.githubIssueAgentPromptPathCopyButtonCount}`
    );
  }
  if (summary.githubIssueAgentClipboardCopyButtonCount !== 3) {
    problems.push(
      `expected three GitHub issue agent clipboard command buttons, got ${summary.githubIssueAgentClipboardCopyButtonCount}`
    );
  }
  if (summary.githubIssueCodexUiCopyButtonCount !== 1) {
    problems.push(`expected one GitHub issue Codex UI copy button, got ${summary.githubIssueCodexUiCopyButtonCount}`);
  }
  if (!(selectedPlannedWorkflowText || "").includes("#43 Plan selected workbench")) {
    problems.push("clicking a GitHub issue workflow should select it without navigating");
  }
  if (
    !(plannedWorkflowUrl || "").includes("issue_workflow=") ||
    !(plannedWorkflowUrl || "").includes("operator-ui-planned-workflow")
  ) {
    problems.push("selecting a planned GitHub issue workflow should persist URL state in place");
  }
  if (
    !(plannedRecommendedHandoffText || "").includes("Review the plan before execution") ||
    !(plannedRecommendedHandoffText || "").includes("plan review") ||
    !(plannedRecommendedHandoffText || "").includes("Review first")
  ) {
    problems.push("selecting a planned GitHub issue workflow should show plan review as the recommended next action");
  }
  if (
    !(latestWorkflowUrl || "").includes("issue_workflow=") ||
    !(latestWorkflowUrl || "").includes("operator-ui-issue-workflow")
  ) {
    problems.push("reselecting the latest GitHub issue workflow should refresh URL state in place");
  }
  if (summary.githubIssueNextCommandCopyButtonCount !== 1) {
    problems.push(
      `expected one GitHub issue next-command copy button, got ${summary.githubIssueNextCommandCopyButtonCount}`
    );
  }
  if (summary.githubIssueSyncCommandCopyButtonCount !== 1) {
    problems.push(
      `expected one GitHub issue sync-command copy button, got ${summary.githubIssueSyncCommandCopyButtonCount}`
    );
  }
  if (summary.githubIssueSyncCommentCardCount !== 1) {
    problems.push(`expected one GitHub issue sync comment card, got ${summary.githubIssueSyncCommentCardCount}`);
  }
  if (summary.githubIssueSyncCommentMetaCount !== 1) {
    problems.push(`expected one GitHub issue sync comment metadata row, got ${summary.githubIssueSyncCommentMetaCount}`);
  }
  if (summary.githubIssueSyncCommentPreviewCount !== 1) {
    problems.push(`expected one GitHub issue sync comment preview, got ${summary.githubIssueSyncCommentPreviewCount}`);
  }
  if (summary.githubIssueSyncCommentTextCopyButtonCount !== 1) {
    problems.push(
      `expected one GitHub issue sync comment text copy button, got ${summary.githubIssueSyncCommentTextCopyButtonCount}`
    );
  }
  if (summary.githubIssueSyncCommentPathCopyButtonCount !== 1) {
    problems.push(
      `expected one GitHub issue sync comment path copy button, got ${summary.githubIssueSyncCommentPathCopyButtonCount}`
    );
  }
  if (!summary.githubIssueWorkbenchIncludesIssue) {
    problems.push("GitHub issue workbench should expose latest issue refs and repository context");
  }
  if (!summary.githubIssueWorkbenchIncludesAgentHandoff) {
    problems.push("GitHub issue workbench should expose native-agent handoff commands");
  }
  if (summary.missionContextCardCount !== 4) {
    problems.push(`expected four mission context cards, got ${summary.missionContextCardCount}`);
  }
  if (!summary.missionContextIncludesApprovalBoundary) {
    problems.push("mission context should explain that GitHub remains the approval boundary");
  }
  if (summary.developerHandoffPanelCount !== 1) {
    problems.push(`expected one developer handoff panel, got ${summary.developerHandoffPanelCount}`);
  }
  if (summary.developerNextCommandPanelCount !== 1) {
    problems.push(`expected one developer next command panel, got ${summary.developerNextCommandPanelCount}`);
  }
  if (summary.developerNextCommandCopyButtonCount !== 1) {
    problems.push(`expected one developer next command copy button, got ${summary.developerNextCommandCopyButtonCount}`);
  }
  if (!summary.developerNextCommandIncludesRunId) {
    problems.push("developer next command panel should expose a run-scoped terminal command");
  }
  if (summary.developerLiveBriefPanelCount !== 1) {
    problems.push(`expected one developer live brief panel, got ${summary.developerLiveBriefPanelCount}`);
  }
  if (summary.developerLiveBriefCopyButtonCount !== 1) {
    problems.push(`expected one developer live brief copy button, got ${summary.developerLiveBriefCopyButtonCount}`);
  }
  if (!summary.developerLiveBriefIncludesSummary) {
    problems.push("developer live brief should summarize current run state and next action");
  }
  if (summary.developerGithubUpdatePanelCount !== 1) {
    problems.push(`expected one developer GitHub update panel, got ${summary.developerGithubUpdatePanelCount}`);
  }
  if (summary.developerGithubUpdateCopyButtonCount !== 1) {
    problems.push(`expected one developer GitHub update copy button, got ${summary.developerGithubUpdateCopyButtonCount}`);
  }
  if (!summary.developerGithubUpdateIncludesSummary) {
    problems.push("developer GitHub update should summarize PR handoff state");
  }
  if (summary.developerReviewPromptPanelCount !== 1) {
    problems.push(`expected one developer review prompt panel, got ${summary.developerReviewPromptPanelCount}`);
  }
  if (!summary.developerReviewPromptIncludesAgent) {
    problems.push("developer tab should expose a portable Cursor/Codex/OpenHands review prompt");
  }
  if (summary.developerReviewPromptCopyButtonCount !== 1) {
    problems.push(`expected one developer review prompt copy button, got ${summary.developerReviewPromptCopyButtonCount}`);
  }
  if (summary.developerEvidencePacketPanelCount !== 1) {
    problems.push(`expected one developer evidence packet panel, got ${summary.developerEvidencePacketPanelCount}`);
  }
  if (summary.developerEvidencePathsCopyButtonCount !== 1) {
    problems.push(`expected one developer evidence paths copy button, got ${summary.developerEvidencePathsCopyButtonCount}`);
  }
  if (!summary.developerEvidencePacketIncludesPaths) {
    problems.push("developer evidence packet should expose the key artifact path list");
  }
  if (
    !copiedReviewPrompt.includes("Review this Catalyst Continuum run") ||
    !copiedReviewPrompt.includes("Continuum review checklist")
  ) {
    problems.push("developer review prompt copy action should write the portable prompt to clipboard");
  }
  if (
    copiedReviewPrompt.includes("artifact group(s)") ||
    !copiedReviewPrompt.includes("artifact(s) across")
  ) {
    problems.push("developer review prompt evidence map should use valid artifact/type wording");
  }
  if (
    !copiedReviewPrompt.includes("Key artifact paths:") ||
    !copiedReviewPrompt.includes("developer_handoff:") ||
    !copiedReviewPrompt.includes("quality_report:")
  ) {
    problems.push("developer review prompt should include key artifact paths for native agent review");
  }
  if (
    !copiedEvidencePaths.includes("developer_handoff:") ||
    !copiedEvidencePaths.includes("quality_report:") ||
    !copiedEvidencePaths.includes("agent_task_report:") ||
    !copiedEvidencePaths.includes("log:")
  ) {
    problems.push("developer evidence paths copy action should write the prioritized artifact path packet");
  }
  if (!copiedNextCommand.includes("curl -fsS") || !copiedNextCommand.includes("/runs/")) {
    problems.push("developer next command copy action should write a run-scoped local UI command");
  }
  if (
    !copiedLiveBrief.includes("Catalyst Continuum live run brief") ||
    !copiedLiveBrief.includes("Next safe action:") ||
    !copiedLiveBrief.includes("Evidence groups:") ||
    !copiedLiveBrief.includes("Next terminal command:")
  ) {
    problems.push("developer live brief copy action should write the compact run brief to clipboard");
  }
  if (
    !copiedGithubUpdate.includes("Catalyst Continuum update") ||
    !copiedGithubUpdate.includes("Delivery evidence:") ||
    !copiedGithubUpdate.includes("PR handoff:") ||
    !copiedGithubUpdate.includes("Draft PR:") ||
    !copiedGithubUpdate.includes("Human review remains in GitHub")
  ) {
    problems.push("developer GitHub update copy action should write the compact issue/PR comment");
  }
  if (summary.bodyTextIncludesDraftPr && copiedGithubUpdate.includes("Draft PR: not opened yet")) {
    problems.push("developer GitHub update should not say the draft PR is missing when PR evidence exists");
  }
  if (summary.developerCodexPanelCount !== 1) {
    problems.push(`expected one developer Codex app-server panel, got ${summary.developerCodexPanelCount}`);
  }
  if (summary.developerCodexCommandCardCount !== 2) {
    problems.push(`expected two developer Codex command cards, got ${summary.developerCodexCommandCardCount}`);
  }
  if (!summary.developerCodexIncludesCommand) {
    problems.push("developer tab should expose a runnable Codex app-server command for the handoff prompt");
  }
  if (
    !copiedCodexCommand.includes("make codex-app-server-run") ||
    !copiedCodexCommand.includes("CODEX_APP_SERVER_PROMPT_FILE=")
  ) {
    problems.push("developer Codex command copy action should write the runnable command to clipboard");
  }
  if (
    !copiedIssueWorkflowPath.includes("operator-ui-issue-workflow") ||
    copiedIssueWorkflowPath.endsWith("manifest.json")
  ) {
    problems.push("GitHub issue workbench workflow path copy should write the selected workflow directory");
  }
  if (
    !copiedIssueSessionManifest.endsWith("/manifest.json") ||
    !copiedIssueSessionManifest.includes("operator-ui-issue-workflow/session")
  ) {
    problems.push("GitHub issue workbench session manifest copy should write the selected manifest path");
  }
  if (
    !copiedIssueEvidencePacket.includes("Catalyst GitHub issue workflow evidence") ||
    !copiedIssueEvidencePacket.includes("Workflow dir:") ||
    !copiedIssueEvidencePacket.includes("Session manifest:") ||
    !copiedIssueEvidencePacket.includes("Strict preflight command:") ||
    !copiedIssueEvidencePacket.includes("Draft PR:")
  ) {
    problems.push("GitHub issue workbench evidence packet should include paths, commands, and PR evidence");
  }
  if (
    !copiedIssueReviewCommand.includes("make github-issue-review") ||
    !copiedIssueReviewCommand.includes("operator-ui-issue-workflow")
  ) {
    problems.push("GitHub issue workbench review command should reopen the selected workflow report");
  }
  if (
    !copiedIssueNextCommand.includes("make github-issue-review") ||
    !copiedIssueNextCommand.includes("GITHUB_ISSUE_WORKFLOW_DIR=")
  ) {
    problems.push("GitHub issue workbench next command should point at the selected workflow review");
  }
  if (
    !copiedIssuePreflightCommand.includes("make github-issue-preflight-strict") ||
    !copiedIssuePreflightCommand.includes("operator-ui-issue-workflow")
  ) {
    problems.push("GitHub issue workbench preflight command should point at the selected workflow");
  }
  if (
    !copiedIssueSyncCommand.includes("make github-issue-sync") ||
    !copiedIssueSyncCommand.includes("GITHUB_ISSUE_SYNC_APPLY=1") ||
    !copiedIssueSyncCommand.includes("GITHUB_ISSUE_SYNC_PR_URL=")
  ) {
    problems.push("GitHub issue workbench sync command should include apply, run summary, and PR URL inputs");
  }
  if (!copiedIssueSyncComment.includes("Catalyst Continuum prepared a draft PR and issue update.")) {
    problems.push("GitHub issue workbench sync comment copy should write the generated comment");
  }
  if (
    !copiedIssueSyncCommentPath.endsWith("/comment.md") ||
    !copiedIssueSyncCommentPath.includes("operator-ui-issue-workflow/issue-sync")
  ) {
    problems.push("GitHub issue workbench sync comment path copy should write the comment artifact path");
  }
  if (
    !copiedIssueAgentPromptText.includes("Codex should implement issue #42") ||
    copiedIssueAgentPromptText.includes("make github-issue-agent-prompt")
  ) {
    problems.push("GitHub issue workbench prompt text copy should write the generated prompt, not the command");
  }
  if (
    !copiedIssueAgentPromptCommand.includes("make github-issue-agent-prompt") ||
    !copiedIssueAgentPromptCommand.includes("GITHUB_ISSUE_WORKFLOW_DIR=") ||
    !copiedIssueAgentPromptCommand.includes("AGENT=codex")
  ) {
    problems.push("GitHub issue workbench agent prompt copy should write the selected Codex prompt command");
  }
  if (
    !copiedIssueAgentPromptPath.endsWith("/codex-prompt.md") ||
    copiedIssueAgentPromptPath.includes("make github-issue-agent-prompt")
  ) {
    problems.push("GitHub issue workbench prompt path copy should write the generated prompt path, not a command");
  }
  if (
    !copiedIssueAgentClipboardCommand.includes("make github-issue-agent-prompt-copy") ||
    !copiedIssueAgentClipboardCommand.includes("AGENT=codex")
  ) {
    problems.push("GitHub issue workbench agent clipboard copy should write the selected prompt-to-clipboard command");
  }
  if (
    !copiedIssueCodexUiCommand.includes("make github-issue-codex-ui") ||
    !copiedIssueCodexUiCommand.includes("GITHUB_ISSUE_WORKFLOW_DIR=") ||
    !copiedIssueCodexUiCommand.includes("REPO_PATH=")
  ) {
    problems.push("GitHub issue workbench Codex UI copy should write the app-server continuation command");
  }
  if (
    !copiedIssuePlannedCommand.includes("make github-issue-run") ||
    !copiedIssuePlannedCommand.includes("operator-ui-planned-workflow")
  ) {
    problems.push("selecting a planned GitHub issue workflow should update the next command in place");
  }
  if (
    !copiedIssuePlannedRecommendedCommand.includes("make github-issue-plan-review") ||
    !copiedIssuePlannedRecommendedCommand.includes("operator-ui-planned-workflow")
  ) {
    problems.push("planned workflow recommended action should copy the plan-review command");
  }
  if (
    !copiedIssuePlannedPreflightCommand.includes("make github-issue-preflight-strict") ||
    !copiedIssuePlannedPreflightCommand.includes("operator-ui-planned-workflow")
  ) {
    problems.push("selecting a planned GitHub issue workflow should update the preflight command in place");
  }
  if (
    !copiedIssuePlannedReviewCommand.includes("make github-issue-plan-review") ||
    !copiedIssuePlannedReviewCommand.includes("operator-ui-planned-workflow")
  ) {
    problems.push("selecting a planned GitHub issue workflow should update the review command in place");
  }
  if (summary.developerValueCardCount < 5) {
    problems.push(`expected developer value cards, got ${summary.developerValueCardCount}`);
  }
  if (summary.developerReviewItemCount < 6) {
    problems.push(`expected developer review checklist items, got ${summary.developerReviewItemCount}`);
  }
  if (summary.developerEvidenceCardCount !== 4) {
    problems.push(`expected four developer evidence cards, got ${summary.developerEvidenceCardCount}`);
  }
  if (summary.developerAgentCardCount < 1) {
    problems.push("developer handoff should summarize at least one agent lane");
  }
  if (!summary.developerTabIncludesValue) {
    problems.push("developer tab should explain concrete developer value");
  }
  if (summary.missionFreshnessCardCount !== 4) {
    problems.push(`expected four mission freshness cards, got ${summary.missionFreshnessCardCount}`);
  }
  if (!summary.missionFreshnessLatest) {
    problems.push("mission freshness latest evidence badge is missing");
  }
  if (!summary.missionFreshnessIncludesQuality) {
    problems.push("mission freshness board should summarize quality freshness");
  }
  if (summary.runGuideProgressNow !== "6" || summary.runGuideProgressMax !== "6") {
    problems.push(
      `completed smoke run should show 6/6 guide progress, got ${summary.runGuideProgressNow}/${summary.runGuideProgressMax}`
    );
  }
  if (summary.runSectionNavLinkCount !== 5) {
    problems.push(`expected five selected-run nav links, got ${summary.runSectionNavLinkCount}`);
  }
  if (summary.runSectionNavBadgeCount !== 5) {
    problems.push(`expected five selected-run nav status badges, got ${summary.runSectionNavBadgeCount}`);
  }
  if (!summary.runSectionNavIncludesArtifacts) {
    problems.push("selected-run nav should include a direct artifacts jump link");
  }
  if (!summary.runSectionNavGuideText.includes("6/6")) {
    problems.push(`selected-run nav should expose guide progress, got ${summary.runSectionNavGuideText}`);
  }
  if (!summary.runSectionNavTaskText.includes("task")) {
    problems.push(`selected-run nav should expose task count, got ${summary.runSectionNavTaskText}`);
  }
  if (!summary.runSectionNavArtifactText.includes("artifact")) {
    problems.push(`selected-run nav should expose artifact count, got ${summary.runSectionNavArtifactText}`);
  }
  if (!summary.runSectionNavEventText.includes("event")) {
    problems.push(`selected-run nav should expose event count, got ${summary.runSectionNavEventText}`);
  }
  if (!summary.lastRefresh.includes("WebSocket stream active")) {
    problems.push(`websocket not active: ${summary.lastRefresh}`);
  }
  if (summary.runOutcomeBannerCount !== 1) {
    problems.push(`expected one selected-run outcome banner, got ${summary.runOutcomeBannerCount}`);
  }
  if (summary.runOutcomeProofCount !== 4) {
    problems.push(`expected four selected-run proof chips, got ${summary.runOutcomeProofCount}`);
  }
  if (!summary.runOutcomeIncludesDeliveryFlow) {
    problems.push("selected-run outcome banner should summarize the delivery flow");
  }
  if (summary.runCardCount < 1) {
    problems.push("no run cards visible");
  }
  if (summary.runCardNextStepCount !== summary.runCardCount) {
    problems.push(
      `expected one next-step hint per run card, got ${summary.runCardNextStepCount}/${summary.runCardCount}`
    );
  }
  if (summary.runCardPhaseRailCount !== summary.runCardCount) {
    problems.push(
      `expected one phase rail per run card, got ${summary.runCardPhaseRailCount}/${summary.runCardCount}`
    );
  }
  if (summary.runCardPhaseStepCount < summary.runCardCount * 4) {
    problems.push(
      `expected four phase steps per run card, got ${summary.runCardPhaseStepCount}/${summary.runCardCount}`
    );
  }
  if (!summary.runCardPhaseIncludesHandoff) {
    problems.push("run-card phase rail should expose the PR handoff phase");
  }
  if (summary.runCardTaskMeterCount !== summary.runCardCount) {
    problems.push(
      `expected one task-progress meter per run card, got ${summary.runCardTaskMeterCount}/${summary.runCardCount}`
    );
  }
  if (summary.runCardTaskMeterSegmentCount < 1) {
    problems.push("run cards should expose task-progress meter segments");
  }
  if (summary.taskRows < 1) {
    problems.push("no task rows visible");
  }
  if (summary.artifactEvidenceCardCount !== 4) {
    problems.push(`expected four artifact evidence cards, got ${summary.artifactEvidenceCardCount}`);
  }
  if (!summary.artifactEvidenceIncludesPlanning) {
    problems.push("artifact evidence strip should explain planning evidence");
  }
  if (summary.artifactRows < 1) {
    problems.push("no artifact rows visible");
  }
  if (summary.eventItems < 1) {
    problems.push("no event items visible");
  }
  if (summary.eventHumanTitleCount !== summary.eventItems) {
    problems.push(
      `expected humanized event titles for every timeline item, got ${summary.eventHumanTitleCount}/${summary.eventItems}`
    );
  }
  if (!summary.eventTimelineIncludesHumanTitle) {
    problems.push("event timeline should expose human-readable event titles");
  }
  if (eventFilterCheck.count !== 5) {
    problems.push(`expected five run event filters, got ${eventFilterCheck.count}`);
  }
  if (eventFilterCheck.handoffItems < 1) {
    problems.push("PR handoff event filter should expose at least one handoff event");
  }
  if (!eventFilterCheck.handoffIncludesDraftPr) {
    problems.push("PR handoff event filter should include the draft PR event");
  }
  if (!eventFilterCheck.restoredAllFilter) {
    problems.push("run event filters should restore the all-events view after interaction");
  }
  if (summary.agentLaneCount < 1) {
    problems.push("no agent lanes visible");
  }
  if (summary.agentReportCardCount < 1) {
    problems.push("no agent report cards visible");
  }
  if (summary.runControlReadinessCardCount !== 8) {
    problems.push(
      `expected eight run-control readiness cards, got ${summary.runControlReadinessCardCount}`
    );
  }
  if (summary.runControlReadinessDeveloperHandoffCount !== 1) {
    problems.push("run-control readiness board should explain the developer handoff control");
  }
  if (summary.runControlReadinessDraftPrCount !== 1) {
    problems.push("run-control readiness board should explain the draft PR control");
  }
  if (!summary.runControlReadinessIncludesGuard) {
    problems.push("run-control readiness board should expose guard pass or lock reasons");
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
ensure_playwright_browser
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

log_phase "seeding GitHub issue workbench fixture"
seed_github_issue_workbench_fixture >"$OUTPUT_DIR/github-issue-workbench-fixture.txt"

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
