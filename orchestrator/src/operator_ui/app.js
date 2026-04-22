const BRIEF_STORAGE_KEY = "catalystContinuum.operatorUi.brief";
const AUTO_REFRESH_INTERVAL_MS = 15000;

const state = {
  selectedRunId: new URL(window.location.href).searchParams.get("run"),
  selectedRunStatus: "",
  autoRefresh: true,
  refreshInFlight: false,
  latestRuns: [],
};

const elements = {};

document.addEventListener("DOMContentLoaded", () => {
  cacheElements();
  bindEvents();
  restoreBriefDraft();
  refreshDashboard().catch((error) => {
    renderStatusGrid({
      readyz: failedEnvelope(error),
      aiGateway: failedEnvelope(error),
      config: failedEnvelope(error),
      packs: failedEnvelope(error),
    });
    writeConsole(
      elements.briefConsole,
      elements.briefConsoleStatus,
      "error",
      { error: error.message }
    );
  });
  window.setInterval(() => {
    if (!state.autoRefresh || state.refreshInFlight) {
      return;
    }

    refreshDashboard().catch((error) => {
      console.error("operator UI auto refresh failed", error);
    });
  }, AUTO_REFRESH_INTERVAL_MS);
});

function cacheElements() {
  const ids = [
    "actionConsole",
    "actionConsoleStatus",
    "artifactHeadline",
    "artifactHighlights",
    "artifactTableWrap",
    "automationActionFilter",
    "automationConsole",
    "automationConsoleStatus",
    "automationSignalKindInput",
    "autoRefreshToggle",
    "branchNameInput",
    "briefConsole",
    "briefConsoleStatus",
    "briefEditor",
    "clearBriefButton",
    "deliveryCount",
    "detailEmptyState",
    "eventHeadline",
    "eventTimeline",
    "lastRefresh",
    "packChips",
    "publishPushToggle",
    "refreshButton",
    "remoteUrlInput",
    "repositorySignalsList",
    "runNextWebhookButton",
    "runRepositoryAutomationButton",
    "runDetailShell",
    "runStatusFilter",
    "runSummaryCards",
    "runsList",
    "selectedRunLabel",
    "signalCount",
    "statusGrid",
    "submitNextSignalButton",
    "submitBriefButton",
    "taskHeadline",
    "taskTableWrap",
    "validateBriefButton",
    "webhookActionCount",
    "webhookActionsList",
    "webhookDeliveriesList",
  ];

  for (const id of ids) {
    elements[id] = document.getElementById(id);
  }
}

function bindEvents() {
  elements.refreshButton.addEventListener("click", () => {
    refreshDashboard().catch((error) => {
      console.error("manual refresh failed", error);
    });
  });

  elements.autoRefreshToggle.addEventListener("change", () => {
    state.autoRefresh = elements.autoRefreshToggle.checked;
  });

  elements.runStatusFilter.addEventListener("change", () => {
    state.selectedRunStatus = elements.runStatusFilter.value;
    refreshDashboard().catch((error) => {
      console.error("run filter refresh failed", error);
    });
  });

  elements.runsList.addEventListener("click", (event) => {
    const runButton = event.target.closest("[data-run-id]");
    if (!runButton) {
      return;
    }

    selectRun(runButton.dataset.runId).catch((error) => {
      console.error("run selection failed", error);
    });
  });

  elements.briefEditor.addEventListener("input", () => {
    window.localStorage.setItem(BRIEF_STORAGE_KEY, elements.briefEditor.value);
  });

  elements.clearBriefButton.addEventListener("click", () => {
    elements.briefEditor.value = "";
    window.localStorage.removeItem(BRIEF_STORAGE_KEY);
    writeConsole(
      elements.briefConsole,
      elements.briefConsoleStatus,
      "neutral",
      "Brief editor cleared."
    );
  });

  elements.validateBriefButton.addEventListener("click", () => {
    submitBriefRequest("validate").catch((error) => {
      console.error("brief validation failed", error);
    });
  });

  elements.submitBriefButton.addEventListener("click", () => {
    submitBriefRequest("submit").catch((error) => {
      console.error("brief submission failed", error);
    });
  });

  elements.runNextWebhookButton.addEventListener("click", () => {
    runNextWebhookRequest().catch((error) => {
      console.error("next webhook execution failed", error);
    });
  });

  elements.submitNextSignalButton.addEventListener("click", () => {
    submitNextSignalRequest().catch((error) => {
      console.error("next repository signal submission failed", error);
    });
  });

  elements.runRepositoryAutomationButton.addEventListener("click", () => {
    runRepositoryAutomationRequest().catch((error) => {
      console.error("repository automation cycle failed", error);
    });
  });

  elements.runDetailShell.addEventListener("click", (event) => {
    const actionButton = event.target.closest("[data-run-action]");
    if (!actionButton) {
      return;
    }

    executeRunAction(actionButton.dataset.runAction).catch((error) => {
      console.error("run action failed", error);
    });
  });
}

async function refreshDashboard() {
  state.refreshInFlight = true;
  elements.refreshButton.disabled = true;
  elements.refreshButton.textContent = "Refreshing...";

  try {
    const [
      dashboardEnvelope,
      runsEnvelope,
      webhookActionsEnvelope,
      signalsEnvelope,
      deliveriesEnvelope,
    ] = await Promise.all([
      fetchJsonEnvelope("/ui/dashboard"),
      fetchJsonEnvelope(buildRunsPath()),
      fetchJsonEnvelope("/github/webhook-actions?limit=8"),
      fetchJsonEnvelope("/repository-signals?limit=8"),
      fetchJsonEnvelope("/github/webhooks?limit=8"),
    ]);

    const readyzEnvelope =
      dashboardEnvelope.data?.readyz ?? failedEnvelope(new Error("missing readyz snapshot"));
    const aiGatewayEnvelope =
      dashboardEnvelope.data?.ai_gateway ??
      failedEnvelope(new Error("missing AI gateway snapshot"));
    const configEnvelope =
      dashboardEnvelope.data?.config ?? failedEnvelope(new Error("missing config snapshot"));
    const packsEnvelope =
      dashboardEnvelope.data?.packs ?? failedEnvelope(new Error("missing packs snapshot"));

    renderStatusGrid({
      readyz: readyzEnvelope,
      aiGateway: aiGatewayEnvelope,
      config: configEnvelope,
      packs: packsEnvelope,
    });
    const runs = Array.isArray(runsEnvelope.data?.runs) ? runsEnvelope.data.runs : [];
    state.latestRuns = runs;
    renderPackChips(packsEnvelope.data);
    renderRuns(runsEnvelope.data);
    renderAutomationRail({
      webhookActions: webhookActionsEnvelope.data,
      repositorySignals: signalsEnvelope.data,
      webhookDeliveries: deliveriesEnvelope.data,
    });

    if (runs.length === 0) {
      clearRunSelection("No runs available yet. Submit a brief to materialize the first run.");
    } else if (state.selectedRunId && runs.some((run) => run.run_id === state.selectedRunId)) {
      await loadRunDetail(state.selectedRunId);
    } else {
      await selectRun(runs[0].run_id);
    }

    elements.lastRefresh.textContent = `Last refresh ${new Date().toLocaleTimeString()}`;
  } finally {
    state.refreshInFlight = false;
    elements.refreshButton.disabled = false;
    elements.refreshButton.textContent = "Refresh";
  }
}

function buildRunsPath() {
  const params = new URLSearchParams({ limit: "12" });
  if (state.selectedRunStatus) {
    params.set("status", state.selectedRunStatus);
  }
  return `/runs?${params.toString()}`;
}

async function selectRun(runId, options = {}) {
  state.selectedRunId = runId;
  renderRuns({ runs: state.latestRuns });
  if (!options.refreshOnly) {
    const nextUrl = new URL(window.location.href);
    nextUrl.searchParams.set("run", runId);
    window.history.replaceState({}, "", nextUrl);
  }
  await loadRunDetail(runId);
}

async function loadRunDetail(runId) {
  const [runEnvelope, eventsEnvelope] = await Promise.all([
    fetchJsonEnvelope(`/runs/${encodeURIComponent(runId)}`),
    fetchJsonEnvelope(`/runs/${encodeURIComponent(runId)}/events?limit=30`),
  ]);

  if (!runEnvelope.ok) {
    clearRunSelection(formatEnvelopeError(runEnvelope));
    return;
  }

  renderRunDetail(runEnvelope.data, eventsEnvelope.data);
}

async function submitBriefRequest(mode) {
  const body = elements.briefEditor.value.trim();
  if (!body) {
    writeConsole(
      elements.briefConsole,
      elements.briefConsoleStatus,
      "warning",
      { error: "Brief editor is empty." }
    );
    return;
  }

  const path = mode === "submit" ? "/briefs/submit" : "/briefs/validate";
  const envelope = await fetchJsonEnvelope(path, {
    method: "POST",
    headers: {
      "Content-Type": "application/yaml; charset=utf-8",
    },
    body,
  });

  writeConsole(
    elements.briefConsole,
    elements.briefConsoleStatus,
    envelope.ok ? "success" : "error",
    envelope.data
  );

  if (envelope.ok && mode === "submit" && envelope.data?.run_id) {
    state.selectedRunId = envelope.data.run_id;
    await refreshDashboard();
  }
}

async function runNextWebhookRequest() {
  const action = elements.automationActionFilter.value.trim();
  const envelope = await fetchJsonEnvelope("/github/webhook-actions/next", {
    method: "POST",
    headers: {
      "Content-Type": "application/json; charset=utf-8",
    },
    body: JSON.stringify(action ? { action } : {}),
  });

  writeConsole(
    elements.automationConsole,
    elements.automationConsoleStatus,
    envelope.ok ? "success" : "error",
    envelope.data
  );

  await refreshDashboard();
}

async function submitNextSignalRequest() {
  const brief = requireAutomationBrief();
  if (!brief) {
    return;
  }

  const query = automationQuery({ includeAction: false, includeSignalKind: true });
  const envelope = await fetchJsonEnvelope(`/repository-signals/next${query}`, {
    method: "POST",
    headers: {
      "Content-Type": "application/yaml; charset=utf-8",
    },
    body: brief,
  });

  writeConsole(
    elements.automationConsole,
    elements.automationConsoleStatus,
    envelope.ok ? "success" : "error",
    envelope.data
  );

  const submittedRunId = nextSubmittedRunId(envelope.data);
  if (envelope.ok && submittedRunId) {
    state.selectedRunId = submittedRunId;
  }

  await refreshDashboard();
}

async function runRepositoryAutomationRequest() {
  const brief = requireAutomationBrief();
  if (!brief) {
    return;
  }

  const query = automationQuery({ includeAction: true, includeSignalKind: true });
  const envelope = await fetchJsonEnvelope(`/repository-automation/next${query}`, {
    method: "POST",
    headers: {
      "Content-Type": "application/yaml; charset=utf-8",
    },
    body: brief,
  });

  writeConsole(
    elements.automationConsole,
    elements.automationConsoleStatus,
    envelope.ok ? "success" : "error",
    envelope.data
  );

  const submittedRunId = automationSubmittedRunId(envelope.data);
  if (envelope.ok && submittedRunId) {
    state.selectedRunId = submittedRunId;
  }

  await refreshDashboard();
}

async function executeRunAction(actionId) {
  if (!state.selectedRunId) {
    writeConsole(
      elements.actionConsole,
      elements.actionConsoleStatus,
      "warning",
      { error: "Select a run before executing an action." }
    );
    return;
  }

  const body = buildRunActionBody(actionId);
  const action = actionSpec(actionId, state.selectedRunId);
  const envelope = await fetchJsonEnvelope(action.path, {
    method: "POST",
    headers: body ? { "Content-Type": "application/json; charset=utf-8" } : undefined,
    body: body ? JSON.stringify(body) : undefined,
  });

  writeConsole(
    elements.actionConsole,
    elements.actionConsoleStatus,
    envelope.ok ? "success" : "error",
    envelope.data
  );

  await refreshDashboard();
}

function actionSpec(actionId, runId) {
  const base = `/runs/${encodeURIComponent(runId)}`;
  switch (actionId) {
    case "tasks-next":
      return { path: `${base}/tasks/next` };
    case "worker-once":
      return { path: `${base}/worker/once` };
    case "evaluate-policy":
      return { path: `${base}/evaluate-policy` };
    case "evaluate-quality":
      return { path: `${base}/evaluate-quality` };
    case "export-pr":
      return { path: `${base}/export-pr-candidate` };
    case "publish-pr":
      return { path: `${base}/publish-pr-export` };
    case "draft-pr":
      return { path: `${base}/draft-pr` };
    default:
      throw new Error(`Unsupported run action: ${actionId}`);
  }
}

function buildRunActionBody(actionId) {
  const branchName = elements.branchNameInput.value.trim();
  const remoteUrl = elements.remoteUrlInput.value.trim();
  const push = elements.publishPushToggle.checked;

  switch (actionId) {
    case "export-pr":
      return branchName ? { branch_name: branchName } : null;
    case "publish-pr":
      return {
        ...(remoteUrl ? { remote_url: remoteUrl } : {}),
        push,
      };
    case "draft-pr":
      return {
        ...(remoteUrl ? { remote_url: remoteUrl } : {}),
        ...(branchName ? { branch_name: branchName } : {}),
      };
    default:
      return null;
  }
}

function requireAutomationBrief() {
  const brief = elements.briefEditor.value.trim();
  if (brief) {
    return brief;
  }

  writeConsole(
    elements.automationConsole,
    elements.automationConsoleStatus,
    "warning",
    { error: "Brief editor is empty." }
  );
  return null;
}

function automationQuery(options) {
  const params = new URLSearchParams();
  const action = elements.automationActionFilter.value.trim();
  const signalKind = elements.automationSignalKindInput.value.trim();

  if (options.includeAction && action) {
    params.set("action", action);
  }
  if (options.includeSignalKind && signalKind) {
    params.set("signal_kind", signalKind);
  }

  const rendered = params.toString();
  return rendered ? `?${rendered}` : "";
}

function nextSubmittedRunId(payload) {
  return payload?.submission?.run_id ?? null;
}

function automationSubmittedRunId(payload) {
  if (payload?.signal_submission?.outcome !== "submitted") {
    return null;
  }

  return payload.signal_submission.submission?.run_id ?? null;
}

function renderStatusGrid(payload) {
  const readyz = payload.readyz?.data ?? {};
  const gateway = payload.aiGateway?.data ?? {};
  const config = payload.config?.data ?? {};
  const packs = payload.packs?.data ?? {};
  const runtimeStatuses = Array.isArray(config.runtime_provider_statuses)
    ? config.runtime_provider_statuses
    : [];
  const enabledRuntimeCount = runtimeStatuses.filter((status) => status.enabled).length;
  const externalServers = Array.isArray(config.external_mcp_servers?.servers)
    ? config.external_mcp_servers.servers
    : [];
  const githubApp = config.github_app ?? {};

  elements.statusGrid.innerHTML = [
    renderStatusCard({
      title: "Control plane",
      statusClass: payload.readyz?.ok ? "success" : "warning",
      badge: readyz.status ?? "unknown",
      primary: readyz.database_name ?? "Database unresolved",
      secondary: readyz.error ?? `Schema ${readyz.schema ?? "unknown"}`,
      detail: `${readyz.database ?? "unknown"} / ${readyz.schema ?? "unknown"}`,
    }),
    renderStatusCard({
      title: "AI gateway",
      statusClass: gateway.ready ? "success" : payload.aiGateway?.ok ? "warning" : "error",
      badge: gateway.status ?? "unreachable",
      primary: gateway.current_host_default_model_alias ?? "No model alias",
      secondary: gateway.error ?? `${gateway.available_model_count ?? 0} model(s) visible`,
      detail: gateway.host_base_url ?? "No base URL configured",
    }),
    renderStatusCard({
      title: "Runtime provider",
      statusClass: config.runtime_providers?.default_provider ? "success" : "warning",
      badge: config.runtime_providers?.default_provider ?? "unknown",
      primary: `${enabledRuntimeCount} enabled / ${runtimeStatuses.length} declared`,
      secondary: runtimeStatuses
        .map((status) => `${status.provider}:${status.registered ? "ready" : "disabled"}`)
        .join(" · "),
      detail: config.runtime_providers?.source_path ?? "Embedded defaults",
    }),
    renderStatusCard({
      title: "GitHub App",
      statusClass: githubApp.ready ? "success" : "warning",
      badge: githubApp.ready ? "ready" : "incomplete",
      primary: githubApp.publication_ready ? "Publication ready" : "Publication gated",
      secondary: githubApp.missing_fields?.length
        ? githubApp.missing_fields.join(", ")
        : "Webhook and publication fields resolved",
      detail: githubApp.private_key_path ?? "No private key path",
    }),
    renderStatusCard({
      title: "External MCP",
      statusClass: externalServers.length > 0 ? "success" : "warning",
      badge: `${externalServers.length} server(s)`,
      primary: externalServers
        .map((server) => server.server_id)
        .slice(0, 3)
        .join(" · ") || "No external servers configured",
      secondary: externalServers
        .flatMap((server) => server.allowed_agents || [])
        .filter(uniqueValue)
        .join(", ") || "No allowed agents declared",
      detail: config.external_mcp_servers?.source_path ?? "Embedded defaults",
    }),
    renderStatusCard({
      title: "Repository packs",
      statusClass: packs.pack_count > 0 ? "success" : "warning",
      badge: `${packs.pack_count ?? 0} pack(s)`,
      primary: packs.default_pack_id ?? "No default pack",
      secondary: Array.isArray(packs.items)
        ? packs.items.map((item) => item.pack_id).join(" · ")
        : "No catalog available",
      detail: "Static catalog loaded through the orchestrator",
    }),
  ].join("");
}

function renderStatusCard(card) {
  return `
    <article class="status-card status-card-${escapeHtml(card.statusClass)}">
      <div class="status-card-head">
        <p class="panel-kicker">${escapeHtml(card.title)}</p>
        <span class="badge badge-${escapeHtml(card.statusClass)}">${escapeHtml(card.badge)}</span>
      </div>
      <h3>${escapeHtml(card.primary)}</h3>
      <p>${escapeHtml(card.secondary)}</p>
      <p class="microcopy">${escapeHtml(card.detail)}</p>
    </article>
  `;
}

function renderPackChips(packs) {
  if (!Array.isArray(packs?.items) || packs.items.length === 0) {
    elements.packChips.innerHTML = '<span class="chip">No pack catalog</span>';
    return;
  }

  elements.packChips.innerHTML = packs.items
    .slice(0, 4)
    .map(
      (item) =>
        `<span class="chip">${escapeHtml(item.pack_id)} · ${escapeHtml(
          item.agent_profile?.default_agent ?? "no-default-agent"
        )}</span>`
    )
    .join("");
}

function renderRuns(response) {
  const runs = Array.isArray(response?.runs) ? response.runs : [];
  if (runs.length === 0) {
    elements.runsList.innerHTML = `
      <div class="empty-state compact">
        No runs match the current filter.
      </div>
    `;
    return;
  }

  elements.runsList.innerHTML = runs
    .map((run) => {
      const repositoryName = run.repository?.owner && run.repository?.name
        ? `${run.repository.owner}/${run.repository.name}`
        : "repository not declared";
      const isSelected = run.run_id === state.selectedRunId;
      return `
        <button
          class="run-card${isSelected ? " is-selected" : ""}"
          type="button"
          data-run-id="${escapeHtml(run.run_id)}"
        >
          <div class="run-card-head">
            <span class="badge badge-${escapeHtml(statusTone(run.status))}">
              ${escapeHtml(run.status)}
            </span>
            <span class="mono">${escapeHtml(shortId(run.run_id))}</span>
          </div>
          <h3>${escapeHtml(run.title)}</h3>
          <p class="microcopy">
            ${escapeHtml(run.target_pack ?? "no pack")} · ${escapeHtml(run.trigger)}
          </p>
          <div class="run-card-stats">
            <span>${escapeHtml(repositoryName)}</span>
            <span>${escapeHtml(run.task_counts?.total ?? 0)} task(s)</span>
            <span>${escapeHtml(run.artifact_count ?? 0)} artifact(s)</span>
          </div>
          <div class="run-card-foot">
            <span>ok ${escapeHtml(run.task_counts?.succeeded ?? 0)}</span>
            <span>run ${escapeHtml(run.task_counts?.running ?? 0)}</span>
            <span>fail ${escapeHtml(run.task_counts?.failed ?? 0)}</span>
            <span>${escapeHtml(formatTimestamp(run.created_at))}</span>
          </div>
        </button>
      `;
    })
    .join("");
}

function renderAutomationRail(payload) {
  const actions = Array.isArray(payload.webhookActions?.requests)
    ? payload.webhookActions.requests
    : [];
  const signals = Array.isArray(payload.repositorySignals?.signals)
    ? payload.repositorySignals.signals
    : [];
  const deliveries = Array.isArray(payload.webhookDeliveries?.deliveries)
    ? payload.webhookDeliveries.deliveries
    : [];

  elements.webhookActionCount.textContent = String(actions.length);
  elements.signalCount.textContent = String(signals.length);
  elements.deliveryCount.textContent = String(deliveries.length);

  elements.webhookActionsList.innerHTML = renderRailItems(
    actions,
    (item) => item.action,
    (item) => `${item.status} · ${item.repository_full_name ?? item.delivery_id}`,
    (item) => item.updated_at ?? item.created_at
  );
  elements.repositorySignalsList.innerHTML = renderRailItems(
    signals,
    (item) => item.signal_kind,
    (item) => `${item.status} · ${item.repository_full_name}`,
    (item) => item.updated_at ?? item.created_at
  );
  elements.webhookDeliveriesList.innerHTML = renderRailItems(
    deliveries,
    (item) => item.event,
    (item) => `${item.routing_status} · ${item.repository_full_name ?? item.delivery_id}`,
    (item) => item.updated_at ?? item.created_at
  );
}

function renderRailItems(items, headingSelector, summarySelector, timeSelector) {
  if (!items.length) {
    return '<div class="empty-state compact">Nothing queued right now.</div>';
  }

  return items
    .map(
      (item) => `
        <article class="rail-item">
          <div class="rail-item-head">
            <h4>${escapeHtml(headingSelector(item))}</h4>
            <span class="badge badge-${escapeHtml(statusTone(item.status ?? item.routing_status))}">
              ${escapeHtml(item.status ?? item.routing_status ?? "unknown")}
            </span>
          </div>
          <p>${escapeHtml(summarySelector(item))}</p>
          <p class="microcopy">${escapeHtml(formatTimestamp(timeSelector(item)))}</p>
        </article>
      `
    )
    .join("");
}

function renderRunDetail(runDetail, eventsResponse) {
  elements.detailEmptyState.classList.add("hidden");
  elements.runDetailShell.classList.remove("hidden");
  elements.selectedRunLabel.textContent = `${runDetail.title} · ${shortId(runDetail.run_id)}`;

  elements.runSummaryCards.innerHTML = [
    summaryCard("Status", runDetail.status, `${runDetail.trigger} trigger`),
    summaryCard("Pack", runDetail.target_pack ?? "unassigned", runDetail.requested_by ?? "requested_by unknown"),
    summaryCard(
      "Repository",
      runDetail.repository?.owner && runDetail.repository?.name
        ? `${runDetail.repository.owner}/${runDetail.repository.name}`
        : "No repository target",
      runDetail.repository?.default_branch ?? "default branch unknown"
    ),
    summaryCard(
      "Task counts",
      `${runDetail.task_counts?.total ?? 0} total`,
      `ok ${runDetail.task_counts?.succeeded ?? 0} · fail ${runDetail.task_counts?.failed ?? 0} · run ${runDetail.task_counts?.running ?? 0}`
    ),
    summaryCard(
      "Artifacts",
      `${runDetail.artifact_count ?? 0} persisted`,
      `${runDetail.artifact_highlights?.length ?? 0} highlight(s)`
    ),
    summaryCard("Created", formatTimestamp(runDetail.created_at), runDetail.brief_source_path ?? "no brief path"),
  ].join("");

  renderTasks(runDetail.tasks || []);
  renderArtifacts(runDetail);
  renderEvents(eventsResponse?.events || []);
}

function renderTasks(tasks) {
  elements.taskHeadline.textContent = `${tasks.length} task(s) materialized`;
  if (!tasks.length) {
    elements.taskTableWrap.innerHTML = '<div class="empty-state compact">No tasks persisted for this run yet.</div>';
    return;
  }

  elements.taskTableWrap.innerHTML = `
    <table class="data-table">
      <thead>
        <tr>
          <th>Status</th>
          <th>Title</th>
          <th>Agent</th>
          <th>Provider</th>
          <th>Retry</th>
          <th>Updated</th>
        </tr>
      </thead>
      <tbody>
        ${tasks
          .map(
            (task) => `
              <tr>
                <td><span class="badge badge-${escapeHtml(statusTone(task.status))}">${escapeHtml(
                  task.status
                )}</span></td>
                <td>
                  <strong>${escapeHtml(task.title)}</strong>
                  <div class="microcopy">${escapeHtml(task.backlog_item_id)}</div>
                </td>
                <td>
                  ${escapeHtml(task.assigned_agent ?? "n/a")}
                  <div class="microcopy">${escapeHtml(
                    task.orchestrator_model ?? task.agent_execution?.mode ?? "no model hint"
                  )}</div>
                </td>
                <td>${escapeHtml(task.execution?.provider ?? "n/a")}</td>
                <td>
                  ${escapeHtml(task.retry_state?.retry_count ?? 0)} / ${escapeHtml(
                    task.retry_state?.max_retry_count ?? 0
                  )}
                </td>
                <td>${escapeHtml(
                  formatTimestamp(task.completed_at ?? task.lease_expires_at ?? task.started_at ?? task.created_at)
                )}</td>
              </tr>
            `
          )
          .join("")}
      </tbody>
    </table>
  `;
}

function renderArtifacts(runDetail) {
  const highlights = Array.isArray(runDetail.artifact_highlights)
    ? runDetail.artifact_highlights
    : [];
  const artifacts = Array.isArray(runDetail.artifacts) ? runDetail.artifacts : [];
  elements.artifactHeadline.textContent = `${artifacts.length} artifact(s), ${highlights.length} highlight(s)`;

  elements.artifactHighlights.innerHTML = highlights.length
    ? highlights
        .map(
          (artifact) => `
            <article class="artifact-card">
              <p class="panel-kicker">${escapeHtml(artifact.artifact_type)}</p>
              <h4>${escapeHtml(artifact.format)}</h4>
              <p>${escapeHtml(artifact.location_value)}</p>
              <p class="microcopy">${escapeHtml(formatTimestamp(artifact.created_at))}</p>
            </article>
          `
        )
        .join("")
    : '<div class="empty-state compact">No highlight artifacts selected for this run.</div>';

  if (!artifacts.length) {
    elements.artifactTableWrap.innerHTML = '<div class="empty-state compact">No artifacts persisted for this run yet.</div>';
    return;
  }

  elements.artifactTableWrap.innerHTML = `
    <table class="data-table">
      <thead>
        <tr>
          <th>Type</th>
          <th>Format</th>
          <th>Location</th>
          <th>Created</th>
        </tr>
      </thead>
      <tbody>
        ${artifacts
          .map(
            (artifact) => `
              <tr>
                <td>${escapeHtml(artifact.artifact_type)}</td>
                <td>${escapeHtml(artifact.format)}</td>
                <td class="mono">${escapeHtml(artifact.location_value)}</td>
                <td>${escapeHtml(formatTimestamp(artifact.created_at))}</td>
              </tr>
            `
          )
          .join("")}
      </tbody>
    </table>
  `;
}

function renderEvents(events) {
  elements.eventHeadline.textContent = `${events.length} recent event(s)`;
  if (!events.length) {
    elements.eventTimeline.innerHTML = '<div class="empty-state compact">No run events persisted for this run yet.</div>';
    return;
  }

  elements.eventTimeline.innerHTML = events
    .map(
      (event) => `
        <article class="timeline-item">
          <div class="timeline-head">
            <div>
              <p class="panel-kicker">${escapeHtml(event.scope)}</p>
              <h4>${escapeHtml(event.event_type)}</h4>
            </div>
            <span class="badge badge-${escapeHtml(statusTone(event.status ?? event.scope))}">
              ${escapeHtml(event.status ?? event.scope)}
            </span>
          </div>
          <p>${escapeHtml(event.summary)}</p>
          <div class="timeline-meta">
            <span>${escapeHtml(formatTimestamp(event.created_at))}</span>
            <span class="mono">${escapeHtml(event.task_id ? shortId(event.task_id) : shortId(event.event_id))}</span>
          </div>
        </article>
      `
    )
    .join("");
}

function clearRunSelection(message) {
  state.selectedRunId = null;
  elements.selectedRunLabel.textContent = "No run selected.";
  elements.detailEmptyState.textContent = message;
  elements.detailEmptyState.classList.remove("hidden");
  elements.runDetailShell.classList.add("hidden");
  const nextUrl = new URL(window.location.href);
  nextUrl.searchParams.delete("run");
  window.history.replaceState({}, "", nextUrl);
}

function writeConsole(target, badge, tone, payload) {
  target.textContent =
    typeof payload === "string" ? payload : JSON.stringify(payload, null, 2);
  badge.className = `badge badge-${tone}`;
  badge.textContent = toneLabel(tone);
}

function summaryCard(title, primary, secondary) {
  return `
    <article class="summary-card">
      <p class="panel-kicker">${escapeHtml(title)}</p>
      <h3>${escapeHtml(primary)}</h3>
      <p>${escapeHtml(secondary)}</p>
    </article>
  `;
}

async function fetchJsonEnvelope(url, options = {}) {
  let response;
  try {
    response = await window.fetch(url, {
      ...options,
      headers: {
        Accept: "application/json",
        ...(options.headers || {}),
      },
    });
  } catch (error) {
    return failedEnvelope(error);
  }

  const text = await response.text();
  let data = null;
  if (text.trim()) {
    try {
      data = JSON.parse(text);
    } catch (error) {
      data = { raw_text: text, parse_error: error.message };
    }
  }

  return {
    ok: response.ok,
    status: response.status,
    statusText: response.statusText,
    data,
    url,
  };
}

function failedEnvelope(error) {
  return {
    ok: false,
    status: 0,
    statusText: "network_error",
    data: { error: error.message },
  };
}

function formatEnvelopeError(envelope) {
  const message = envelope?.data?.error;
  if (message) {
    return message;
  }

  if (envelope?.status) {
    return `Request failed with HTTP ${envelope.status}`;
  }

  return "Request failed before the orchestrator returned a response.";
}

function restoreBriefDraft() {
  const saved = window.localStorage.getItem(BRIEF_STORAGE_KEY);
  if (saved) {
    elements.briefEditor.value = saved;
  }
  state.autoRefresh = elements.autoRefreshToggle.checked;
}

function statusTone(value) {
  switch (value) {
    case "ready":
    case "ok":
    case "succeeded":
    case "completed":
    case "observed_default_branch_head":
    case "allowed":
      return "success";
    case "running":
    case "queued":
    case "pending":
    case "candidate":
    case "degraded":
      return "warning";
    case "failed":
    case "error":
    case "denied":
    case "unavailable":
    case "http_error":
    case "invalid_response":
    case "unauthorized":
      return "error";
    default:
      return "neutral";
  }
}

function toneLabel(tone) {
  switch (tone) {
    case "success":
      return "Success";
    case "warning":
      return "Warning";
    case "error":
      return "Error";
    default:
      return "Idle";
  }
}

function shortId(value) {
  if (!value) {
    return "n/a";
  }
  const rendered = String(value);
  return rendered.length > 12 ? rendered.slice(0, 8) : rendered;
}

function formatTimestamp(value) {
  if (!value) {
    return "not recorded";
  }

  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) {
    return String(value);
  }

  return parsed.toLocaleString();
}

function uniqueValue(value, index, items) {
  return items.indexOf(value) === index;
}

function escapeHtml(value) {
  return String(value ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}
