const BRIEF_STORAGE_KEY = "catalystContinuum.operatorUi.brief";
const RUN_ACTION_DRAFTS_STORAGE_KEY = "catalystContinuum.operatorUi.runActionDrafts";
const AUTO_REFRESH_INTERVAL_MS = 15000;
const DASHBOARD_LOADING_CARD_TITLES = [
  "Control plane",
  "AI gateway",
  "Runtime provider",
  "GitHub App",
  "External MCP",
  "Repository packs",
];
const CAPABILITY_LOADING_CARD_TITLES = [
  "Brief -> Run",
  "Runtime execution",
  "Model gateway",
  "GitHub handoff",
];
const PR_CANDIDATE_ARTIFACT_TYPE = "pr_candidate";
const PR_EXPORT_ARTIFACT_TYPE = "pr_export";
const BACKLOG_ARTIFACT_TYPE = "backlog";
const POLICY_REPORT_ARTIFACT_TYPE = "policy_report";
const DISPATCH_PLAN_ARTIFACT_TYPE = "agent_dispatch_plan";
const QUALITY_REPORT_ARTIFACT_TYPE = "quality_report";
const PR_PUBLICATION_ARTIFACT_TYPE = "pr_publication";
const GITHUB_PULL_REQUEST_ARTIFACT_TYPE = "github_pull_request";
const RUN_QUALITY_EVALUATED_EVENT_TYPE = "run_quality_evaluated";
const PR_CANDIDATE_EXPORTED_EVENT_TYPE = "pr_candidate_exported";
const PR_EXPORT_PUBLISHED_EVENT_TYPE = "pr_export_published";
const GITHUB_PR_OPENED_EVENT_TYPE = "github_pr_opened";
const BRIEF_BUSY_LABELS = {
  validate: "Validating...",
  submit: "Submitting...",
};
const AUTOMATION_BUSY_LABELS = {
  webhook: "Running webhook...",
  signal: "Submitting signal...",
  cycle: "Running cycle...",
};
const RUN_STATUS_FILTER_VALUES = new Set([
  "",
  "queued",
  "executing",
  "succeeded",
  "failed",
]);
const RUN_ACTION_BUSY_LABELS = {
  "tasks-next": "Running next task...",
  "worker-once": "Running worker...",
  "evaluate-policy": "Evaluating policy...",
  "evaluate-quality": "Evaluating quality...",
  "export-pr": "Exporting PR...",
  "publish-pr": "Publishing PR...",
  "draft-pr": "Creating draft PR...",
};
const initialUiUrl = new URL(window.location.href);

const state = {
  selectedRunId: initialUiUrl.searchParams.get("run"),
  selectedRunStatus: normalizeRunStatusFilter(initialUiUrl.searchParams.get("status")),
  runSearchQuery: normalizeRunSearchQuery(initialUiUrl.searchParams.get("run_query")),
  selectedRunDetail: null,
  selectedRunEvents: [],
  briefExamples: [],
  activeBriefExampleId: null,
  autoRefresh: true,
  refreshInFlight: false,
  briefRequestInFlight: false,
  automationRequestInFlight: false,
  runActionInFlight: false,
  latestRuns: [],
  latestWebhookActions: [],
  latestRepositorySignals: [],
  latestWebhookDeliveries: [],
  runActionDrafts: {},
  runActionResults: {},
  selectedQueueItem: null,
  selectedQueueRunId: null,
};

const elements = {};

document.addEventListener("DOMContentLoaded", () => {
  cacheElements();
  bindEvents();
  restoreBriefDraft();
  loadBriefExamples().catch((error) => {
    console.error("brief example load failed", error);
    renderBriefExamplesError(error.message);
  });
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
    "actionHighlights",
    "actionSummaryHeadline",
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
    "briefExampleHint",
    "briefExamples",
    "capabilityGrid",
    "clearBriefButton",
    "deliveryCount",
    "detailEmptyState",
    "eventHeadline",
    "eventTimeline",
    "lastRefresh",
    "packChips",
    "publishPushToggle",
    "queueInspectorActions",
    "queueInspectorConsole",
    "queueInspectorDetailStatus",
    "queueInspectorHeadline",
    "queueInspectorLinkedConsole",
    "queueInspectorLinkedStatus",
    "queueInspectorOpenRunButton",
    "queueInspectorPrimaryStatus",
    "queueInspectorSummary",
    "refreshButton",
    "remoteUrlInput",
    "repositorySignalsList",
    "runActionHint",
    "runActionDraftHint",
    "runGuideBadge",
    "runGuideBlockers",
    "runGuideCurrentStage",
    "runGuideCurrentStageDetail",
    "runGuideHeadline",
    "runGuideNextAction",
    "runGuideProgressSummary",
    "runGuideRecommendation",
    "runGuideStages",
    "runLedgerHint",
    "runSearchInput",
    "runNextWebhookButton",
    "runRepositoryAutomationButton",
    "runDetailShell",
    "runStatusFilter",
    "runSummaryCards",
    "runsList",
    "clearRunSearchButton",
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
    "resetRunActionDraftButton",
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
    syncUiUrlState();
    refreshDashboard().catch((error) => {
      console.error("run filter refresh failed", error);
    });
  });

  elements.runSearchInput.addEventListener("input", () => {
    state.runSearchQuery = normalizeRunSearchQuery(elements.runSearchInput.value);
    syncUiUrlState();
    renderRuns({ runs: state.latestRuns });
  });

  elements.clearRunSearchButton.addEventListener("click", () => {
    elements.runSearchInput.value = "";
    state.runSearchQuery = "";
    syncUiUrlState();
    renderRuns({ runs: state.latestRuns });
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

  elements.briefExamples.addEventListener("click", (event) => {
    const exampleButton = event.target.closest("[data-brief-example-id]");
    if (!exampleButton) {
      return;
    }

    loadBriefExampleIntoEditor(exampleButton.dataset.briefExampleId);
  });

  elements.clearBriefButton.addEventListener("click", () => {
    state.activeBriefExampleId = null;
    elements.briefEditor.value = "";
    window.localStorage.removeItem(BRIEF_STORAGE_KEY);
    renderBriefExamples();
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

  elements.webhookActionsList.addEventListener("click", handleQueueItemClick);
  elements.repositorySignalsList.addEventListener("click", handleQueueItemClick);
  elements.webhookDeliveriesList.addEventListener("click", handleQueueItemClick);

  elements.queueInspectorOpenRunButton.addEventListener("click", () => {
    if (!state.selectedQueueRunId) {
      return;
    }

    selectRun(state.selectedQueueRunId).then(() => {
      document
        .querySelector(".detail-panel")
        ?.scrollIntoView({ behavior: "smooth", block: "start" });
    }).catch((error) => {
      console.error("queue inspector run selection failed", error);
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

  elements.branchNameInput.addEventListener("input", () => {
    updateSelectedRunActionDraft({
      branchName: elements.branchNameInput.value,
    });
  });

  elements.remoteUrlInput.addEventListener("input", () => {
    updateSelectedRunActionDraft({
      remoteUrl: elements.remoteUrlInput.value,
    });
  });

  elements.publishPushToggle.addEventListener("change", () => {
    updateSelectedRunActionDraft({
      push: elements.publishPushToggle.checked,
    });
  });

  elements.resetRunActionDraftButton.addEventListener("click", () => {
    resetSelectedRunActionDraft();
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
    await refreshQueueInspectorSelection();

    if (runs.length === 0) {
      clearRunSelection(
        "Load a starter brief, validate it, submit it, then open the new run from this ledger.",
        "No runs materialized yet"
      );
    } else if (state.selectedRunId && runs.some((run) => run.run_id === state.selectedRunId)) {
      syncUiUrlState();
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

async function loadBriefExamples() {
  const envelope = await fetchJsonEnvelope("/ui/brief-examples");
  if (!envelope.ok) {
    throw new Error(formatEnvelopeError(envelope));
  }

  state.briefExamples = sanitizeBriefExamples(envelope.data?.examples);
  renderBriefExamples();
}

function buildRunsPath() {
  const params = new URLSearchParams({ limit: "12" });
  if (state.selectedRunStatus) {
    params.set("status", state.selectedRunStatus);
  }
  return `/runs?${params.toString()}`;
}

function normalizeRunStatusFilter(value) {
  const candidate = String(value ?? "");
  return RUN_STATUS_FILTER_VALUES.has(candidate) ? candidate : "";
}

function normalizeRunSearchQuery(value) {
  return String(value ?? "")
    .trim()
    .replace(/\s+/g, " ");
}

async function selectRun(runId, options = {}) {
  state.selectedRunId = runId;
  renderRuns({ runs: state.latestRuns });
  if (!options.refreshOnly) {
    syncUiUrlState();
  }
  await loadRunDetail(runId);
}

function syncUiUrlState() {
  const nextUrl = new URL(window.location.href);
  if (state.selectedRunId) {
    nextUrl.searchParams.set("run", state.selectedRunId);
  } else {
    nextUrl.searchParams.delete("run");
  }
  if (state.selectedRunStatus) {
    nextUrl.searchParams.set("status", state.selectedRunStatus);
  } else {
    nextUrl.searchParams.delete("status");
  }
  if (state.runSearchQuery) {
    nextUrl.searchParams.set("run_query", state.runSearchQuery);
  } else {
    nextUrl.searchParams.delete("run_query");
  }

  if (nextUrl.toString() === window.location.href) {
    return;
  }

  window.history.replaceState({}, "", nextUrl);
}

async function loadRunDetail(runId) {
  state.selectedRunDetail = null;
  syncRunActionControlsWithState();
  const [runEnvelope, eventsEnvelope] = await Promise.all([
    fetchJsonEnvelope(`/runs/${encodeURIComponent(runId)}`),
    fetchJsonEnvelope(`/runs/${encodeURIComponent(runId)}/events?limit=30`),
  ]);

  if (!runEnvelope.ok) {
    clearRunSelection(formatEnvelopeError(runEnvelope), "Run detail unavailable");
    return;
  }

  renderRunDetail(runEnvelope.data, eventsEnvelope.data);
}

async function submitBriefRequest(mode) {
  if (state.briefRequestInFlight) {
    return;
  }

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

  state.briefRequestInFlight = true;
  setBriefControlsBusyState(mode, true);
  writeBusyConsole(
    elements.briefConsole,
    elements.briefConsoleStatus,
    mode === "submit" ? "Submitting" : "Validating",
    {
      status: mode === "submit" ? "submitting" : "validating",
    }
  );

  const path = mode === "submit" ? "/briefs/submit" : "/briefs/validate";
  try {
    const envelope = await fetchJsonEnvelope(path, {
      method: "POST",
      headers: {
        "Content-Type": "application/yaml; charset=utf-8",
      },
      body,
    });

    writeEnvelopeConsole(
      elements.briefConsole,
      elements.briefConsoleStatus,
      envelope
    );

    if (envelope.ok && mode === "submit" && envelope.data?.run_id) {
      state.selectedRunId = envelope.data.run_id;
      await refreshDashboard();
    }
  } finally {
    state.briefRequestInFlight = false;
    setBriefControlsBusyState(mode, false);
  }
}

async function runNextWebhookRequest() {
  if (state.automationRequestInFlight) {
    return;
  }

  state.automationRequestInFlight = true;
  setAutomationControlsBusyState(elements.runNextWebhookButton, AUTOMATION_BUSY_LABELS.webhook, true);
  writeBusyConsole(
    elements.automationConsole,
    elements.automationConsoleStatus,
    "Running",
    { status: "running_webhook_action" }
  );

  const action = elements.automationActionFilter.value.trim();
  try {
    const envelope = await fetchJsonEnvelope("/github/webhook-actions/next", {
      method: "POST",
      headers: {
        "Content-Type": "application/json; charset=utf-8",
      },
      body: JSON.stringify(action ? { action } : {}),
    });

    writeEnvelopeConsole(
      elements.automationConsole,
      elements.automationConsoleStatus,
      envelope
    );

    await refreshDashboard();
  } finally {
    state.automationRequestInFlight = false;
    setAutomationControlsBusyState(elements.runNextWebhookButton, AUTOMATION_BUSY_LABELS.webhook, false);
  }
}

async function submitNextSignalRequest() {
  if (state.automationRequestInFlight) {
    return;
  }

  const brief = requireAutomationBrief();
  if (!brief) {
    return;
  }

  state.automationRequestInFlight = true;
  setAutomationControlsBusyState(elements.submitNextSignalButton, AUTOMATION_BUSY_LABELS.signal, true);
  writeBusyConsole(
    elements.automationConsole,
    elements.automationConsoleStatus,
    "Running",
    { status: "submitting_repository_signal" }
  );

  const query = automationQuery({ includeAction: false, includeSignalKind: true });
  try {
    const envelope = await fetchJsonEnvelope(`/repository-signals/next${query}`, {
      method: "POST",
      headers: {
        "Content-Type": "application/yaml; charset=utf-8",
      },
      body: brief,
    });

    writeEnvelopeConsole(
      elements.automationConsole,
      elements.automationConsoleStatus,
      envelope
    );

    const submittedRunId = nextSubmittedRunId(envelope.data);
    if (envelope.ok && submittedRunId) {
      state.selectedRunId = submittedRunId;
    }

    await refreshDashboard();
  } finally {
    state.automationRequestInFlight = false;
    setAutomationControlsBusyState(elements.submitNextSignalButton, AUTOMATION_BUSY_LABELS.signal, false);
  }
}

async function runRepositoryAutomationRequest() {
  if (state.automationRequestInFlight) {
    return;
  }

  const brief = requireAutomationBrief();
  if (!brief) {
    return;
  }

  state.automationRequestInFlight = true;
  setAutomationControlsBusyState(
    elements.runRepositoryAutomationButton,
    AUTOMATION_BUSY_LABELS.cycle,
    true
  );
  writeBusyConsole(
    elements.automationConsole,
    elements.automationConsoleStatus,
    "Running",
    { status: "running_repository_automation_cycle" }
  );

  const query = automationQuery({ includeAction: true, includeSignalKind: true });
  try {
    const envelope = await fetchJsonEnvelope(`/repository-automation/next${query}`, {
      method: "POST",
      headers: {
        "Content-Type": "application/yaml; charset=utf-8",
      },
      body: brief,
    });

    writeEnvelopeConsole(
      elements.automationConsole,
      elements.automationConsoleStatus,
      envelope
    );

    const submittedRunId = automationSubmittedRunId(envelope.data);
    if (envelope.ok && submittedRunId) {
      state.selectedRunId = submittedRunId;
    }

    await refreshDashboard();
  } finally {
    state.automationRequestInFlight = false;
    setAutomationControlsBusyState(
      elements.runRepositoryAutomationButton,
      AUTOMATION_BUSY_LABELS.cycle,
      false
    );
  }
}

function handleQueueItemClick(event) {
  const queueButton = event.target.closest("[data-queue-kind][data-queue-id]");
  if (!queueButton) {
    return;
  }

  loadQueueItemDetail(
    queueButton.dataset.queueKind,
    queueButton.dataset.queueId
  ).catch((error) => {
    console.error("queue inspector selection failed", error);
  });
}

async function executeRunAction(actionId) {
  if (state.runActionInFlight) {
    return;
  }

  if (!state.selectedRunId) {
    writeConsole(
      elements.actionConsole,
      elements.actionConsoleStatus,
      "warning",
      { error: "Select a run before executing an action." }
    );
    return;
  }

  state.runActionInFlight = true;
  setRunActionControlsBusyState(actionId, true);
  writeBusyConsole(
    elements.actionConsole,
    elements.actionConsoleStatus,
    "Running",
    {
      status: "running",
      action: actionId,
      run_id: state.selectedRunId,
    }
  );

  const body = buildRunActionBody(actionId);
  const action = actionSpec(actionId, state.selectedRunId);
  try {
    const envelope = await fetchJsonEnvelope(action.path, {
      method: "POST",
      headers: body ? { "Content-Type": "application/json; charset=utf-8" } : undefined,
      body: body ? JSON.stringify(body) : undefined,
    });

    writeEnvelopeConsole(
      elements.actionConsole,
      elements.actionConsoleStatus,
      envelope
    );
    updateRunActionDraftFromResult(actionId, envelope.data);
    rememberRunActionResult(actionId, envelope);

    await refreshDashboard();
  } finally {
    state.runActionInFlight = false;
    setRunActionControlsBusyState(actionId, false);
  }
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

function sanitizeBriefExamples(value) {
  if (!Array.isArray(value)) {
    return [];
  }

  return value
    .map((example) => sanitizeBriefExample(example))
    .filter((example) => example !== null);
}

function sanitizeBriefExample(example) {
  if (!example || typeof example !== "object" || Array.isArray(example)) {
    return null;
  }

  const exampleId = typeof example.example_id === "string" ? example.example_id.trim() : "";
  const label = typeof example.label === "string" ? example.label.trim() : "";
  const content = typeof example.content === "string" ? example.content : "";

  if (!exampleId || !label || !content.trim()) {
    return null;
  }

  return {
    example_id: exampleId,
    label,
    summary: typeof example.summary === "string" ? example.summary.trim() : "",
    source_path: typeof example.source_path === "string" ? example.source_path.trim() : "",
    target_pack: typeof example.target_pack === "string" ? example.target_pack.trim() : "",
    content,
  };
}

function restoreRunActionDrafts() {
  const raw = window.localStorage.getItem(RUN_ACTION_DRAFTS_STORAGE_KEY);
  if (!raw) {
    return {};
  }

  try {
    const parsed = JSON.parse(raw);
    return sanitizeRunActionDraftMap(parsed);
  } catch (error) {
    console.error("failed to restore run action drafts", error);
    return {};
  }
}

function sanitizeRunActionDraftMap(value) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return {};
  }

  return Object.fromEntries(
    Object.entries(value).map(([runId, draft]) => [
      runId,
      sanitizeRunActionDraft(draft),
    ])
  );
}

function sanitizeRunActionDraft(draft) {
  if (!draft || typeof draft !== "object" || Array.isArray(draft)) {
    return {
      branchName: "",
      remoteUrl: "",
      push: false,
    };
  }

  return {
    branchName: typeof draft.branchName === "string" ? draft.branchName : "",
    remoteUrl: typeof draft.remoteUrl === "string" ? draft.remoteUrl : "",
    push: draft.push === true,
  };
}

function persistRunActionDrafts() {
  window.localStorage.setItem(
    RUN_ACTION_DRAFTS_STORAGE_KEY,
    JSON.stringify(state.runActionDrafts)
  );
}

function defaultPromotionBranchName(runId) {
  if (!runId) {
    return "";
  }

  return `continuum/run-${String(runId).slice(0, 8)}`;
}

function inferredRemoteUrl(runDetail) {
  const repository = runDetail?.repository;
  const owner = repository?.owner?.trim();
  const name = repository?.name?.trim();
  const host = repository?.host ?? "github";

  if (!owner || !name || host !== "github") {
    return "";
  }

  return `https://github.com/${owner}/${name}.git`;
}

function defaultRunActionDraft(runDetail) {
  return {
    branchName: defaultPromotionBranchName(runDetail?.run_id),
    remoteUrl: inferredRemoteUrl(runDetail),
    push: false,
  };
}

function runActionDraftForRun(runDetail) {
  if (!runDetail?.run_id) {
    return defaultRunActionDraft(null);
  }

  const defaults = defaultRunActionDraft(runDetail);
  const stored = sanitizeRunActionDraft(state.runActionDrafts[runDetail.run_id]);

  return {
    branchName: stored.branchName || defaults.branchName,
    remoteUrl: stored.remoteUrl || defaults.remoteUrl,
    push: stored.push,
  };
}

function saveRunActionDraft(runId, draft) {
  if (!runId) {
    return;
  }

  state.runActionDrafts[runId] = sanitizeRunActionDraft(draft);
  persistRunActionDrafts();
}

function updateSelectedRunActionDraft(partialDraft) {
  if (!state.selectedRunDetail?.run_id) {
    return;
  }

  saveRunActionDraft(state.selectedRunDetail.run_id, {
    ...runActionDraftForRun(state.selectedRunDetail),
    ...partialDraft,
  });
  renderRunActionDraftHint(state.selectedRunDetail);
}

function resetSelectedRunActionDraft() {
  if (!state.selectedRunDetail?.run_id) {
    return;
  }

  saveRunActionDraft(
    state.selectedRunDetail.run_id,
    defaultRunActionDraft(state.selectedRunDetail)
  );
  syncRunActionDraftInputs(state.selectedRunDetail);
}

function updateRunActionDraftFromResult(actionId, payload) {
  if (!state.selectedRunDetail?.run_id || !payload || typeof payload !== "object") {
    return;
  }

  const nextDraft = {};
  if (actionId === "export-pr" || actionId === "draft-pr") {
    if (typeof payload.branch_name === "string" && payload.branch_name.trim()) {
      nextDraft.branchName = payload.branch_name;
    }
  }
  if (actionId === "publish-pr") {
    if (typeof payload.head_branch === "string" && payload.head_branch.trim()) {
      nextDraft.branchName = payload.head_branch;
    }
    if (typeof payload.remote_url === "string" && payload.remote_url.trim()) {
      nextDraft.remoteUrl = payload.remote_url;
    }
    if (typeof payload.push_status === "string") {
      nextDraft.push = payload.push_status === "pushed";
    }
  }
  if (actionId === "draft-pr") {
    if (typeof payload.remote_url === "string" && payload.remote_url.trim()) {
      nextDraft.remoteUrl = payload.remote_url;
    }
    nextDraft.push = true;
  }

  if (Object.keys(nextDraft).length === 0) {
    return;
  }

  saveRunActionDraft(state.selectedRunDetail.run_id, {
    ...runActionDraftForRun(state.selectedRunDetail),
    ...nextDraft,
  });
  syncRunActionDraftInputs(state.selectedRunDetail);
}

function rememberRunActionResult(actionId, envelope) {
  if (!state.selectedRunId) {
    return;
  }

  state.runActionResults[state.selectedRunId] = {
    actionId,
    envelope,
  };
}

function syncRunActionDraftInputs(runDetail) {
  if (!runDetail?.run_id) {
    elements.branchNameInput.value = "";
    elements.remoteUrlInput.value = "";
    elements.publishPushToggle.checked = false;
    renderRunActionDraftHint(null);
    return;
  }

  const draft = runActionDraftForRun(runDetail);
  elements.branchNameInput.value = draft.branchName;
  elements.remoteUrlInput.value = draft.remoteUrl;
  elements.publishPushToggle.checked = draft.push;
  renderRunActionDraftHint(runDetail);
}

function renderRunActionDraftHint(runDetail) {
  if (!runDetail?.run_id) {
    elements.runActionDraftHint.textContent =
      "Promotion defaults appear once a run is selected.";
    return;
  }

  const defaults = defaultRunActionDraft(runDetail);
  const draft = runActionDraftForRun(runDetail);
  const messages = [];

  if (draft.branchName === defaults.branchName) {
    messages.push(`Branch defaults to ${defaults.branchName}.`);
  } else if (draft.branchName.trim()) {
    messages.push(`Branch override active: ${draft.branchName.trim()}.`);
  } else {
    messages.push("Branch name will be resolved by the orchestrator.");
  }

  if (defaults.remoteUrl) {
    if (draft.remoteUrl === defaults.remoteUrl) {
      messages.push(`Remote defaults to ${defaults.remoteUrl}.`);
    } else {
      messages.push(`Remote override active: ${draft.remoteUrl.trim()}.`);
    }
  } else if (draft.remoteUrl.trim()) {
    messages.push(`Remote override active: ${draft.remoteUrl.trim()}.`);
  } else {
    messages.push(
      "No repository-derived remote is available for this run."
    );
  }

  messages.push(
    "Reset defaults to restore the repository-derived promotion values for this run."
  );
  elements.runActionDraftHint.textContent = messages.join(" ");
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
  const capabilityCards = [
    renderStatusCard(briefRunCapabilityCard(payload.readyz?.ok, packs.pack_count ?? 0)),
    renderStatusCard(runtimeExecutionCapabilityCard(enabledRuntimeCount, runtimeStatuses)),
    renderStatusCard(modelGatewayCapabilityCard(gateway)),
    renderStatusCard(githubHandoffCapabilityCard(githubApp)),
  ];

  elements.capabilityGrid.innerHTML = capabilityCards.join("");

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
      detail: `Host ${friendlyConfigValue(gateway.host_base_url, "not configured")}`,
    }),
    renderStatusCard({
      title: "Runtime provider",
      statusClass: config.runtime_providers?.default_provider ? "success" : "warning",
      badge: config.runtime_providers?.default_provider ?? "unknown",
      primary: `${enabledRuntimeCount} enabled / ${runtimeStatuses.length} declared`,
      secondary:
        summarizeValues(
          runtimeStatuses.map(
            (status) => `${status.provider}:${status.registered ? "ready" : "disabled"}`
          ),
          "No runtime providers declared"
        ),
      detail: `Config ${friendlySourcePath(config.runtime_providers?.source_path)}`,
    }),
    renderStatusCard({
      title: "GitHub App",
      statusClass: githubApp.ready ? "success" : "warning",
      badge: githubApp.ready ? "ready" : "incomplete",
      primary: githubApp.publication_ready ? "Publication ready" : "Publication gated",
      secondary: githubApp.missing_fields?.length
        ? summarizeValues(githubApp.missing_fields, "No missing fields", ", ")
        : "Webhook and publication fields resolved",
      detail: `Key ${friendlySourcePath(githubApp.private_key_path, "not configured")}`,
    }),
    renderStatusCard({
      title: "External MCP",
      statusClass: externalServers.length > 0 ? "success" : "warning",
      badge: `${externalServers.length} server(s)`,
      primary:
        summarizeValues(
          externalServers.map((server) => server.server_id),
          "No external servers configured"
        ),
      secondary:
        summarizeValues(
          externalServers
            .flatMap((server) => server.allowed_agents || [])
            .filter(uniqueValue),
          "No allowed agents declared",
          ", "
        ),
      detail: `Config ${friendlySourcePath(config.external_mcp_servers?.source_path)}`,
    }),
    renderStatusCard({
      title: "Repository packs",
      statusClass: packs.pack_count > 0 ? "success" : "warning",
      badge: `${packs.pack_count ?? 0} pack(s)`,
      primary: packs.default_pack_id ?? "No default pack",
      secondary: Array.isArray(packs.items)
        ? summarizeValues(
            packs.items.map((item) => item.pack_id),
            "No catalog available"
          )
        : "No catalog available",
      detail: "Static catalog loaded through the orchestrator",
    }),
  ].join("");
}

function briefRunCapabilityCard(controlPlaneReady, packCount) {
  const ready = controlPlaneReady && packCount > 0;
  return {
    title: "Brief -> Run",
    statusClass: ready ? "success" : "warning",
    badge: ready ? "ready" : "blocked",
    primary: ready ? "Ready to materialize runs" : "Run creation is blocked",
    secondary: ready
      ? "Brief validation and submission can create a durable run, backlog, and artifact lineage."
      : "Run creation needs a healthy control plane and at least one available repository pack.",
    detail: `Packs ${packCount} · Control plane ${controlPlaneReady ? "ready" : "unhealthy"}`,
  };
}

function runtimeExecutionCapabilityCard(enabledRuntimeCount, runtimeStatuses) {
  const ready = enabledRuntimeCount > 0;
  return {
    title: "Runtime execution",
    statusClass: ready ? "success" : "warning",
    badge: ready ? "ready" : "blocked",
    primary: ready ? "Ready for isolated task runtimes" : "Runtime execution is blocked",
    secondary: ready
      ? "At least one runtime provider can host isolated task workspaces and execution steps."
      : "Enable a runtime provider before expecting workspace preparation or runtime-backed task execution.",
    detail:
      summarizeValues(
        runtimeStatuses.map(
          (status) => `${status.provider}:${status.registered ? "ready" : "disabled"}`
        ),
        "No runtime providers declared"
      ),
  };
}

function modelGatewayCapabilityCard(gateway) {
  const ready = gateway.ready === true;
  return {
    title: "Model gateway",
    statusClass: ready ? "success" : "warning",
    badge: ready ? "ready" : "blocked",
    primary: ready ? "Ready for model-backed paths" : "Model-backed paths are blocked",
    secondary: ready
      ? "LiteLLM is reachable for worker or agent flows that need a model gateway."
      : "LiteLLM is unreachable, so worker or agent flows that depend on a model gateway will block until it responds.",
    detail: `Host ${friendlyConfigValue(gateway.host_base_url, "not configured")}`,
  };
}

function githubHandoffCapabilityCard(githubApp) {
  const ready = githubApp.ready === true;
  return {
    title: "GitHub handoff",
    statusClass: ready ? "success" : "warning",
    badge: ready ? "ready" : "blocked",
    primary: ready ? "Ready for draft PR handoff" : "Draft PR handoff is blocked",
    secondary: ready
      ? "When the run reaches promotion readiness, the orchestrator can open or reuse the GitHub draft PR."
      : "GitHub App configuration is incomplete, so export-only flow remains available until publication fields are configured.",
    detail: githubApp.missing_fields?.length
      ? `Missing ${summarizeValues(githubApp.missing_fields, "none", ", ")}`
      : `Key ${friendlySourcePath(githubApp.private_key_path, "configured")}`,
  };
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
      <p class="microcopy status-card-detail">${escapeHtml(card.detail)}</p>
    </article>
  `;
}

function summarizeValues(values, fallback, separator = " · ", maxItems = 3) {
  const items = values.filter(Boolean);
  if (!items.length) {
    return fallback;
  }

  if (items.length <= maxItems) {
    return items.join(separator);
  }

  return `${items.slice(0, maxItems).join(separator)}${separator}+${items.length - maxItems} more`;
}

function friendlySourcePath(value, fallback = "embedded defaults") {
  if (typeof value !== "string" || !value.trim()) {
    return fallback;
  }

  const segments = value.split("/");
  return segments[segments.length - 1] || value;
}

function friendlyConfigValue(value, fallback) {
  if (typeof value !== "string" || !value.trim()) {
    return fallback;
  }

  return value.trim();
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

function renderBriefExamples() {
  if (!state.briefExamples.length) {
    elements.briefExamples.innerHTML = renderSectionEmptyState(
      "Starter briefs",
      "No curated starters configured",
      "Paste YAML manually or use files from examples/briefs when this instance does not expose starter scenarios."
    );
    renderBriefExamplesError("No curated starters are configured for this instance.");
    return;
  }

  elements.briefExamples.innerHTML = state.briefExamples
    .map(
      (example) => {
        const isActive = example.example_id === state.activeBriefExampleId;
        const summary =
          example.summary?.trim() ||
          `${example.target_pack} starter sourced from ${example.source_path}`;

        return `
          <article class="starter-card${isActive ? " is-active" : ""}">
            <div class="starter-card-head">
              <div>
                <p class="panel-kicker">Starter scenario</p>
                <h3>${escapeHtml(example.label)}</h3>
              </div>
              <span class="chip">${escapeHtml(example.target_pack)}</span>
            </div>
            <p class="starter-card-summary">${escapeHtml(summary)}</p>
            <div class="starter-card-meta">
              <span class="mono">${escapeHtml(example.source_path)}</span>
              ${isActive ? '<span class="chip">Loaded into editor</span>' : ""}
            </div>
            <div class="starter-card-foot">
              <p class="microcopy">
                ${
                  isActive
                    ? "Review repository owner/name and requested_by before validation."
                    : "Load this scenario into the editor, then review metadata before validation."
                }
              </p>
              <button
                class="button ${isActive ? "button-primary" : "button-secondary"}"
                type="button"
                data-brief-example-id="${escapeHtml(example.example_id)}"
                data-ui-brief-example="true"
                aria-label="${escapeHtml(`Load ${example.label} starter brief`)}"
                title="${escapeHtml(summary)}"
              >
                ${isActive ? "Reload starter" : "Load starter"}
              </button>
            </div>
          </article>
        `;
      }
    )
    .join("");
  renderBriefExampleHint();
}

function renderBriefExamplesError(message) {
  elements.briefExamples.innerHTML = renderSectionEmptyState(
    "Starter briefs",
    "Starter scenarios unavailable",
    "Paste YAML manually or use the repository examples until this instance can serve curated starter briefs."
  );
  elements.briefExampleHint.textContent =
    `${message} Paste YAML manually or use the repo examples/briefs files directly.`;
}

function renderBriefExampleHint(activeExample) {
  if (activeExample) {
    elements.briefExampleHint.textContent =
      `Loaded ${activeExample.label} from ${activeExample.source_path}. Review repository owner/name and requested_by before validation or submission.`;
    return;
  }

  if (!state.briefExamples.length) {
    elements.briefExampleHint.textContent = "Loading curated starters...";
    return;
  }

  elements.briefExampleHint.textContent =
    "Choose a starter scenario, load it into the editor, then adjust repository metadata before validation and submission.";
}

function loadBriefExampleIntoEditor(exampleId) {
  const example = state.briefExamples.find((candidate) => candidate.example_id === exampleId);
  if (!example) {
    return;
  }

  state.activeBriefExampleId = example.example_id;
  elements.briefEditor.value = example.content;
  window.localStorage.setItem(BRIEF_STORAGE_KEY, example.content);
  renderBriefExamples();
  renderBriefExampleHint(example);
  writeConsole(
    elements.briefConsole,
    elements.briefConsoleStatus,
    "neutral",
    {
      loaded_example: example.label,
      source_path: example.source_path,
      target_pack: example.target_pack,
    }
  );
  elements.briefEditor.focus();
  elements.briefEditor.setSelectionRange(0, 0);
}

function renderRuns(response) {
  const allRuns = Array.isArray(response?.runs)
    ? response.runs
    : Array.isArray(response)
      ? response
      : [];
  const { visibleRuns, preservedSelectedRun } = filterVisibleRuns(allRuns);
  renderRunLedgerHint(allRuns, visibleRuns, preservedSelectedRun);

  if (allRuns.length === 0) {
    elements.runsList.innerHTML = state.selectedRunStatus
      ? loadingOrEmptyState(
          `No ${displayRunStatus(state.selectedRunStatus).toLowerCase()} runs are loaded right now.`
        )
      : renderSectionEmptyState(
          "Run ledger",
          "No runs materialized yet",
          "A durable run appears here only after brief submission materializes backlog, routing, and artifacts.",
          {
            steps: [
              "Load a starter brief or paste product-brief YAML in the left column.",
              "Validate first so pack resolution, routing, and policy are explicit.",
              "Submit the brief, then reopen the new run from this ledger.",
            ],
            actions: [
              {
                href: "#brief-intake",
                label: "Jump to brief intake",
                variant: "primary",
              },
            ],
          }
        );
    return;
  }

  if (visibleRuns.length === 0) {
    elements.runsList.innerHTML = loadingOrEmptyState(
      state.runSearchQuery
        ? "No loaded runs match the current search. Clear search or refresh if you expect a newer run."
        : "No runs match the current filter."
    );
    return;
  }

  elements.runsList.innerHTML = visibleRuns
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
          data-ui-run-card="true"
          aria-pressed="${isSelected ? "true" : "false"}"
          aria-label="${escapeHtml(`Open run ${shortId(run.run_id)} ${run.title}`)}"
        >
          <div class="run-card-head">
            <span class="badge badge-${escapeHtml(statusTone(run.status))}">
              ${escapeHtml(displayRunStatus(run.status))}
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

function filterVisibleRuns(runs) {
  let preservedSelectedRun = false;
  const visibleRuns = runs.filter((run) => {
    if (runMatchesActiveFilters(run)) {
      return true;
    }
    if (state.selectedRunId && run.run_id === state.selectedRunId) {
      preservedSelectedRun = Boolean(state.runSearchQuery);
      return true;
    }
    return false;
  });

  return {
    visibleRuns,
    preservedSelectedRun,
  };
}

function runMatchesActiveFilters(run) {
  return runMatchesSearch(run);
}

function runMatchesSearch(run) {
  if (!state.runSearchQuery) {
    return true;
  }

  return runSearchHaystack(run).includes(state.runSearchQuery.toLowerCase());
}

function runSearchHaystack(run) {
  const repositoryName = run.repository?.owner && run.repository?.name
    ? `${run.repository.owner}/${run.repository.name}`
    : "";

  return [
    run.run_id,
    shortId(run.run_id),
    run.title,
    repositoryName,
    run.target_pack,
    run.trigger,
    run.status,
  ]
    .filter(Boolean)
    .join(" ")
    .toLowerCase();
}

function renderRunLedgerHint(allRuns, visibleRuns, preservedSelectedRun) {
  if (allRuns.length === 0 && !state.runSearchQuery && !state.selectedRunStatus) {
    elements.runLedgerHint.textContent =
      "Search the loaded run ledger by title, repository, pack, trigger, or run id.";
    return;
  }

  const fragments = [`Showing ${visibleRuns.length} of ${allRuns.length} loaded runs.`];

  if (state.selectedRunStatus) {
    fragments.push(`Status filter: ${displayRunStatus(state.selectedRunStatus)}.`);
  }
  if (state.runSearchQuery) {
    fragments.push(`Search: "${state.runSearchQuery}".`);
  }
  if (preservedSelectedRun) {
    fragments.push("Kept the selected run visible even though it does not match the current search.");
  }

  elements.runLedgerHint.textContent = fragments.join(" ");
}

function renderAutomationRail(payload) {
  state.latestWebhookActions = Array.isArray(payload.webhookActions?.requests)
    ? payload.webhookActions.requests
    : [];
  state.latestRepositorySignals = Array.isArray(payload.repositorySignals?.signals)
    ? payload.repositorySignals.signals
    : [];
  state.latestWebhookDeliveries = Array.isArray(payload.webhookDeliveries?.deliveries)
    ? payload.webhookDeliveries.deliveries
    : [];
  renderAutomationRailCollections();
}

function renderAutomationRailCollections() {
  elements.webhookActionCount.textContent = String(state.latestWebhookActions.length);
  elements.signalCount.textContent = String(state.latestRepositorySignals.length);
  elements.deliveryCount.textContent = String(state.latestWebhookDeliveries.length);

  elements.webhookActionsList.innerHTML = renderRailItems(
    state.latestWebhookActions,
    "webhook_action",
    (item) => item.request_id,
    (item) => item.action,
    (item) => `${item.status} · ${item.repository_full_name ?? item.delivery_id}`,
    (item) => item.updated_at ?? item.created_at,
    "No pending webhook actions. Signed GitHub deliveries will queue control-plane work here before a run exists."
  );
  elements.repositorySignalsList.innerHTML = renderRailItems(
    state.latestRepositorySignals,
    "repository_signal",
    (item) => item.signal_id,
    (item) => item.signal_kind,
    (item) => `${item.status} · ${item.repository_full_name}`,
    (item) => item.updated_at ?? item.created_at,
    "No repository signals yet. Once a routed webhook action succeeds, the automation handoff will appear here."
  );
  elements.webhookDeliveriesList.innerHTML = renderRailItems(
    state.latestWebhookDeliveries,
    "webhook_delivery",
    (item) => item.delivery_id,
    (item) => item.event,
    (item) => `${item.routing_status} · ${item.repository_full_name ?? item.delivery_id}`,
    (item) => item.updated_at ?? item.created_at,
    "No signed webhook deliveries are loaded yet. Incoming GitHub App traffic will appear here for inspection."
  );
}

function renderRailItems(
  items,
  queueKind,
  idSelector,
  headingSelector,
  summarySelector,
  timeSelector,
  emptyMessage
) {
  if (!items.length) {
    return loadingOrEmptyState(emptyMessage ?? "Nothing queued right now.");
  }

  return items
    .map((item) => {
      const itemId = idSelector(item);
      const isSelected =
        state.selectedQueueItem?.kind === queueKind &&
        state.selectedQueueItem?.id === itemId;
      return `
        <button
          class="rail-item rail-item-button${isSelected ? " is-selected" : ""}"
          type="button"
          data-queue-kind="${escapeHtml(queueKind)}"
          data-queue-id="${escapeHtml(itemId)}"
          aria-pressed="${isSelected ? "true" : "false"}"
        >
          <div class="rail-item-head">
            <h4>${escapeHtml(headingSelector(item))}</h4>
            <span class="badge badge-${escapeHtml(statusTone(item.status ?? item.routing_status))}">
              ${escapeHtml(item.status ?? item.routing_status ?? "unknown")}
            </span>
          </div>
          <p>${escapeHtml(summarySelector(item))}</p>
          <p class="microcopy">
            ${escapeHtml(shortId(itemId))} · ${escapeHtml(formatTimestamp(timeSelector(item)))}
          </p>
        </button>
      `;
    })
    .join("");
}

async function refreshQueueInspectorSelection() {
  if (!state.selectedQueueItem) {
    return;
  }

  await loadQueueItemDetail(
    state.selectedQueueItem.kind,
    state.selectedQueueItem.id,
    { silentLoading: true }
  );
}

async function loadQueueItemDetail(queueKind, queueId, options = {}) {
  state.selectedQueueItem = {
    kind: queueKind,
    id: queueId,
  };
  state.selectedQueueRunId = null;
  renderAutomationRailCollections();
  if (!options.silentLoading) {
    renderQueueInspectorLoading(queueKind, queueId);
  }

  const primaryEnvelope = await fetchQueuePrimaryEnvelope(queueKind, queueId);
  const linkedEnvelope = await fetchQueueLinkedEnvelope(queueKind, queueId, primaryEnvelope);
  renderQueueInspector(queueKind, queueId, primaryEnvelope, linkedEnvelope);
}

async function fetchQueuePrimaryEnvelope(queueKind, queueId) {
  const routeQueueId = queueRouteId(queueId);
  switch (queueKind) {
    case "webhook_action":
      return fetchJsonEnvelope(`/github/webhook-actions/${routeQueueId}`);
    case "repository_signal":
      return fetchJsonEnvelope(`/repository-signals/${routeQueueId}`);
    case "webhook_delivery":
      return fetchJsonEnvelope(`/github/webhooks/${routeQueueId}`);
    default:
      throw new Error(`Unsupported queue inspector kind: ${queueKind}`);
  }
}

async function fetchQueueLinkedEnvelope(queueKind, queueId, primaryEnvelope) {
  if (!primaryEnvelope.ok) {
    return null;
  }

  const routeQueueId = queueRouteId(queueId);
  switch (queueKind) {
    case "webhook_action":
      if (!primaryEnvelope.data?.report_path) {
        return null;
      }
      return fetchJsonEnvelope(`/github/webhook-actions/${routeQueueId}/report`);
    case "repository_signal":
      return fetchJsonEnvelope(`/repository-signals/${routeQueueId}/payload`);
    case "webhook_delivery":
      return fetchJsonEnvelope(`/github/webhooks/${routeQueueId}/receipt`);
    default:
      return null;
  }
}

function queueRouteId(queueId) {
  if (!queueId || queueId.includes("/")) {
    throw new Error(`Unsupported queue route identifier: ${queueId}`);
  }

  return queueId;
}

function renderQueueInspectorLoading(queueKind, queueId) {
  elements.queueInspectorHeadline.textContent = `Loading ${queueInspectorKindLabel(queueKind)} ${shortId(queueId)}...`;
  elements.queueInspectorSummary.innerHTML =
    '<div class="empty-state compact">Loading queue detail...</div>';
  elements.queueInspectorActions.classList.add("hidden");
  setConsolePayload(elements.queueInspectorConsole, "Loading queue detail...");
  setConsolePayload(elements.queueInspectorLinkedConsole, "Loading linked document...");
  setBadge(elements.queueInspectorPrimaryStatus, "warning", "Loading");
  setBadge(elements.queueInspectorDetailStatus, "warning", "Loading");
  setBadge(elements.queueInspectorLinkedStatus, "neutral", "Pending");
}

function renderQueueInspector(queueKind, queueId, primaryEnvelope, linkedEnvelope) {
  const detail = primaryEnvelope.data ?? {};
  const primaryStatus = queueInspectorPrimaryStatus(queueKind, detail, primaryEnvelope);
  const relatedRunId = queueInspectorRelatedRunId(queueKind, detail);
  state.selectedQueueRunId = relatedRunId;

  elements.queueInspectorHeadline.textContent =
    `${queueInspectorKindTitle(queueKind)} · ${shortId(queueId)}`;
  elements.queueInspectorSummary.innerHTML = renderQueueInspectorSummary(
    queueKind,
    queueId,
    primaryEnvelope
  );
  setConsolePayload(
    elements.queueInspectorConsole,
    primaryEnvelope.ok ? primaryEnvelope.data ?? "No detail payload returned." : formatEnvelopeError(primaryEnvelope)
  );
  setConsolePayload(
    elements.queueInspectorLinkedConsole,
    linkedEnvelope
      ? linkedEnvelope.ok
        ? linkedEnvelope.data ?? "No linked document payload returned."
        : formatEnvelopeError(linkedEnvelope)
      : `No linked ${queueInspectorLinkedDocumentLabel(queueKind)} available.`
  );

  setBadge(
    elements.queueInspectorPrimaryStatus,
    statusTone(primaryStatus),
    primaryStatus
  );
  setBadge(
    elements.queueInspectorDetailStatus,
    primaryEnvelope.ok ? "success" : "error",
    primaryEnvelope.ok ? "Loaded" : "Error"
  );
  if (linkedEnvelope) {
    setBadge(
      elements.queueInspectorLinkedStatus,
      linkedEnvelope.ok ? "success" : "warning",
      linkedEnvelope.ok ? queueInspectorLinkedDocumentLabel(queueKind) : "Unavailable"
    );
  } else {
    setBadge(elements.queueInspectorLinkedStatus, "neutral", "No link");
  }

  if (relatedRunId) {
    elements.queueInspectorActions.classList.remove("hidden");
  } else {
    elements.queueInspectorActions.classList.add("hidden");
  }
}

function renderQueueInspectorSummary(queueKind, queueId, primaryEnvelope) {
  if (!primaryEnvelope.ok) {
    return `
      <div class="empty-state compact">
        ${escapeHtml(formatEnvelopeError(primaryEnvelope))}
      </div>
    `;
  }

  const detail = primaryEnvelope.data ?? {};
  const rows = queueInspectorRows(queueKind, queueId, detail)
    .filter((row) => row.value)
    .map(
      (row) => `
        <div class="inspector-row">
          <span class="inspector-row-label">${escapeHtml(row.label)}</span>
          <strong class="inspector-row-value">${escapeHtml(row.value)}</strong>
        </div>
      `
    );

  return rows.length
    ? rows.join("")
    : '<div class="empty-state compact">No summary available for this queue item.</div>';
}

function queueInspectorRows(queueKind, queueId, detail) {
  switch (queueKind) {
    case "webhook_action":
      return [
        { label: "Request", value: detail.request_id ?? queueId },
        { label: "Repository", value: detail.repository_full_name },
        { label: "Branch", value: detail.ref_name ?? detail.repository_default_branch },
        { label: "Action", value: detail.action },
        {
          label: "Attempts",
          value: detail.attempt_count != null ? String(detail.attempt_count) : null,
        },
        {
          label: "Updated",
          value: formatTimestamp(detail.updated_at ?? detail.completed_at ?? detail.created_at),
        },
        { label: "Reason", value: detail.requested_reason },
      ];
    case "repository_signal":
      return [
        { label: "Signal", value: detail.signal_id ?? queueId },
        { label: "Repository", value: detail.repository_full_name },
        { label: "Kind", value: detail.signal_kind },
        { label: "Source action", value: detail.source_action },
        { label: "Run trigger", value: detail.proposed_run_trigger },
        { label: "Run", value: detail.materialized_run_id },
        {
          label: "Updated",
          value: formatTimestamp(detail.updated_at ?? detail.created_at),
        },
        { label: "Message", value: detail.message },
      ];
    case "webhook_delivery":
      return [
        { label: "Delivery", value: detail.delivery_id ?? queueId },
        { label: "Repository", value: detail.repository_full_name },
        { label: "Event", value: detail.event },
        { label: "Routing", value: detail.routing_action ?? detail.routing_status },
        {
          label: "Signature",
          value:
            detail.signature_verified == null
              ? null
              : detail.signature_verified
                ? "verified"
                : "unverified",
        },
        {
          label: "Updated",
          value: formatTimestamp(detail.updated_at ?? detail.created_at),
        },
        { label: "Message", value: detail.message ?? detail.routing_reason },
      ];
    default:
      return [];
  }
}

function queueInspectorPrimaryStatus(queueKind, detail, primaryEnvelope) {
  if (!primaryEnvelope.ok) {
    return "error";
  }

  switch (queueKind) {
    case "webhook_action":
      return detail.status ?? "loaded";
    case "repository_signal":
      return detail.status ?? "loaded";
    case "webhook_delivery":
      return detail.routing_status ?? detail.status ?? "loaded";
    default:
      return "loaded";
  }
}

function queueInspectorRelatedRunId(queueKind, detail) {
  if (queueKind === "repository_signal") {
    return detail.materialized_run_id ?? null;
  }

  return null;
}

function queueInspectorKindLabel(queueKind) {
  switch (queueKind) {
    case "webhook_action":
      return "webhook action";
    case "repository_signal":
      return "repository signal";
    case "webhook_delivery":
      return "webhook delivery";
    default:
      return "queue item";
  }
}

function queueInspectorKindTitle(queueKind) {
  switch (queueKind) {
    case "webhook_action":
      return "Webhook action";
    case "repository_signal":
      return "Repository signal";
    case "webhook_delivery":
      return "Webhook delivery";
    default:
      return "Queue item";
  }
}

function queueInspectorLinkedDocumentLabel(queueKind) {
  switch (queueKind) {
    case "webhook_action":
      return "Report";
    case "repository_signal":
      return "Payload";
    case "webhook_delivery":
      return "Receipt";
    default:
      return "Document";
  }
}

function renderRunDetail(runDetail, eventsResponse) {
  const events = Array.isArray(eventsResponse?.events) ? eventsResponse.events : [];
  state.selectedRunDetail = runDetail;
  state.selectedRunEvents = events;
  elements.detailEmptyState.classList.add("hidden");
  elements.runDetailShell.classList.remove("hidden");
  elements.selectedRunLabel.textContent = `${runDetail.title} · ${shortId(runDetail.run_id)}`;

  elements.runSummaryCards.innerHTML = [
    summaryCard("Status", displayRunStatus(runDetail.status), `${runDetail.trigger} trigger`),
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

  renderRunGuide(runDetail, events);
  renderTasks(runDetail.tasks || []);
  renderArtifacts(runDetail);
  renderEvents(events);
  renderRunActionHighlights(runDetail);
  syncRunActionControlsWithState();
}

function renderRunActionHighlights(runDetail) {
  if (!runDetail?.run_id) {
    elements.actionSummaryHeadline.textContent =
      "Run a control to surface branch, PR, quality, and task execution details here.";
    elements.actionHighlights.innerHTML =
      '<div class="empty-state compact">No run action summary captured for this run yet.</div>';
    return;
  }

  const actionResult = state.runActionResults[runDetail.run_id];
  if (!actionResult) {
    elements.actionSummaryHeadline.textContent =
      "Run a control to surface branch, PR, quality, and task execution details here.";
    elements.actionHighlights.innerHTML =
      '<div class="empty-state compact">No run action summary captured for this run yet.</div>';
    return;
  }

  const presentation = envelopeBadgePresentation(actionResult.envelope);
  elements.actionSummaryHeadline.textContent =
    `Last action: ${displayRunActionLabel(actionResult.actionId)} · ${presentation.label}.`;

  const items = actionHighlightItems(actionResult.actionId, actionResult.envelope);
  elements.actionHighlights.innerHTML = items.length
    ? items.map(renderActionSummaryCard).join("")
    : '<div class="empty-state compact">The latest action returned no structured summary fields.</div>';
}

function renderRunGuide(runDetail, events) {
  const guide = buildRunGuide(runDetail, events);

  setBadge(elements.runGuideBadge, guide.badgeTone, guide.badgeLabel);
  elements.runGuideHeadline.textContent = guide.headline;
  elements.runGuideNextAction.textContent = guide.nextActionTitle;
  elements.runGuideRecommendation.textContent = guide.nextActionDetail;
  elements.runGuideCurrentStage.textContent = guide.currentStageTitle;
  elements.runGuideCurrentStageDetail.textContent = guide.currentStageDetail;
  elements.runGuideProgressSummary.textContent = guide.progressSummary;
  elements.runGuideBlockers.textContent = guide.blockerDetail;
  elements.runGuideStages.innerHTML = guide.stages.length
    ? guide.stages.map(renderGuideStage).join("")
    : '<div class="empty-state compact">No run-stage guidance is available for this run yet.</div>';
}

function buildRunGuide(runDetail, events) {
  if (!runDetail) {
    return {
      badgeTone: "neutral",
      badgeLabel: "Idle",
      headline:
        "Select a run to see which stage is active, what the orchestrator already did, and which operator action should happen next.",
      nextActionTitle: "Choose a run",
      nextActionDetail:
        "Open a run from the ledger after brief submission to unlock execution, quality, and PR guidance.",
      currentStageTitle: "Waiting for a run",
      currentStageDetail:
        "The selected-run guide only activates once a concrete run, task set, and artifact history exist.",
      progressSummary: "0 of 5 stages complete",
      blockerDetail:
        "Execution, quality, and PR handoff stay pending until a run is materialized from the brief.",
      stages: [],
    };
  }

  const artifactTypes = runArtifactTypes(runDetail);
  const eventTypes = new Set(events.map((event) => event.event_type));
  const taskCounts = normalizedTaskCounts(runDetail.task_counts);
  const planningReady =
    taskCounts.total > 0 ||
    artifactTypes.has(BACKLOG_ARTIFACT_TYPE) ||
    artifactTypes.has(POLICY_REPORT_ARTIFACT_TYPE) ||
    artifactTypes.has(DISPATCH_PLAN_ARTIFACT_TYPE);
  const executionBlocked = runDetail.status === "failed";
  const executionComplete = runDetail.status === "succeeded";
  const queuedTaskCount = taskCounts.queued;
  const runningTaskCount = taskCounts.running;
  const qualityReady =
    artifactTypes.has(QUALITY_REPORT_ARTIFACT_TYPE) ||
    eventTypes.has(RUN_QUALITY_EVALUATED_EVENT_TYPE);
  const prCandidateReady = artifactTypes.has(PR_CANDIDATE_ARTIFACT_TYPE);
  const prExportReady =
    artifactTypes.has(PR_EXPORT_ARTIFACT_TYPE) ||
    eventTypes.has(PR_CANDIDATE_EXPORTED_EVENT_TYPE);
  const publicationReady =
    artifactTypes.has(PR_PUBLICATION_ARTIFACT_TYPE) ||
    eventTypes.has(PR_EXPORT_PUBLISHED_EVENT_TYPE);
  const githubPrReady =
    artifactTypes.has(GITHUB_PULL_REQUEST_ARTIFACT_TYPE) ||
    eventTypes.has(GITHUB_PR_OPENED_EVENT_TYPE);

  const stages = [
    {
      title: "Brief intake",
      state: "complete",
      detail: `Accepted through ${runDetail.trigger} and persisted as run ${shortId(runDetail.run_id)}.`,
    },
    {
      title: "Plan and routing",
      state: planningReady ? "complete" : "active",
      detail: planningReady
        ? `${taskCounts.total} task(s), policy, and agent routing are materialized for execution.`
        : "The orchestrator is still preparing the backlog, policy report, and agent dispatch plan.",
    },
    {
      title: "Task execution",
      state: executionBlocked
        ? "blocked"
        : executionComplete
          ? "complete"
          : "active",
      detail: executionBlocked
        ? `${taskCounts.failed} task(s) failed. Inspect tasks and run events before continuing.`
        : executionComplete
          ? `All tasks finished. ${taskCounts.succeeded} succeeded and ${taskCounts.failed} failed.`
          : runningTaskCount > 0
            ? `${runningTaskCount} task(s) running and ${queuedTaskCount} still queued.`
            : queuedTaskCount > 0
              ? `${queuedTaskCount} queued task(s) are waiting for a worker or external agent claim.`
              : "Execution is ready to start but no active task is recorded yet.",
    },
    {
      title: "Quality gate",
      state: executionBlocked
        ? "blocked"
        : qualityReady
          ? "complete"
          : executionComplete
            ? "active"
            : "pending",
      detail: executionBlocked
        ? "Quality stays blocked until task execution succeeds."
        : qualityReady
          ? "A quality report exists for this run. Re-evaluate after any new artifact-changing action."
          : executionComplete
            ? "Run Evaluate quality to prove artifact freshness and promotion readiness."
            : "Quality evaluation unlocks after execution succeeds.",
    },
    {
      title: "PR handoff",
      state: executionBlocked
        ? "blocked"
        : githubPrReady
          ? "complete"
          : publicationReady || prExportReady || (qualityReady && prCandidateReady)
            ? "active"
            : "pending",
      detail: executionBlocked
        ? "PR handoff is blocked because execution did not complete successfully."
        : githubPrReady
          ? "The draft PR already exists. Remaining approval now continues in GitHub review."
          : publicationReady
            ? "Branch publication is complete. Open or reuse the draft PR when you want review to start."
            : prExportReady
              ? "The exported PR bundle is ready. Publish it or create the draft PR for GitHub review."
              : qualityReady && prCandidateReady
                ? "The run is promotable. Export or draft the PR when you are ready for remote handoff."
                : "PR promotion stays locked until execution succeeds and quality is evaluated.",
    },
  ];

  const completedStageCount = stages.filter((stage) => stage.state === "complete").length;
  const currentStage =
    stages.find((stage) => stage.state === "blocked" || stage.state === "active") ??
    stages.find((stage) => stage.state === "pending") ??
    stages[stages.length - 1];

  const nextAction = recommendedRunAction({
    executionBlocked,
    executionComplete,
    githubPrReady,
    planningReady,
    prCandidateReady,
    prExportReady,
    publicationReady,
    qualityReady,
    queuedTaskCount,
    runDetail,
    runningTaskCount,
  });

  return {
    badgeTone: nextAction.badgeTone,
    badgeLabel: nextAction.badgeLabel,
    headline: guideHeadline({
      executionBlocked,
      executionComplete,
      githubPrReady,
      prExportReady,
      publicationReady,
      qualityReady,
      queuedTaskCount,
      runDetail,
      runningTaskCount,
    }),
    nextActionTitle: nextAction.title,
    nextActionDetail: nextAction.detail,
    currentStageTitle: currentStage.title,
    currentStageDetail: currentStage.detail,
    progressSummary: `${completedStageCount} of ${stages.length} stages complete`,
    blockerDetail: guideBlockerDetail({
      executionBlocked,
      executionComplete,
      githubPrReady,
      prCandidateReady,
      prExportReady,
      publicationReady,
      qualityReady,
      queuedTaskCount,
      runDetail,
      runningTaskCount,
    }),
    stages,
  };
}

function renderGuideStage(stage, index) {
  const tone = guideTone(stage.state);
  return `
    <article
      class="guide-stage guide-stage-${escapeHtml(stage.state)}"
      data-stage-index="${escapeHtml(`Stage ${index + 1}`)}"
    >
      <div class="detail-section-head">
        <h4>${escapeHtml(stage.title)}</h4>
        <span class="badge badge-${escapeHtml(tone)}">${escapeHtml(guideBadgeLabel(stage.state))}</span>
      </div>
      <p class="guide-stage-copy">${escapeHtml(stage.detail)}</p>
    </article>
  `;
}

function normalizedTaskCounts(taskCounts) {
  return {
    total: Number(taskCounts?.total ?? 0),
    queued: Number(taskCounts?.queued ?? 0),
    running: Number(taskCounts?.running ?? 0),
    succeeded: Number(taskCounts?.succeeded ?? 0),
    failed: Number(taskCounts?.failed ?? 0),
  };
}

function recommendedRunAction(context) {
  if (context.githubPrReady) {
    return {
      badgeTone: "success",
      badgeLabel: "Review",
      title: "Review the draft PR",
      detail:
        "The orchestrator already finished the GitHub handoff. Continue with normal engineering review and approval in GitHub.",
    };
  }

  if (context.executionBlocked) {
    return {
      badgeTone: "error",
      badgeLabel: "Blocked",
      title: "Inspect the failure",
      detail:
        "Review the failing task cards and recent run events before retrying, changing the brief, or promoting anything further.",
    };
  }

  if (context.runningTaskCount > 0 || context.runDetail.status === "executing") {
    return {
      badgeTone: "warning",
      badgeLabel: "Running",
      title: "Monitor active work",
      detail:
        "At least one task is already running. Let it finish, then refresh or use Worker once if you are manually draining the queue.",
    };
  }

  if (context.queuedTaskCount > 0 || context.runDetail.status === "queued") {
    return {
      badgeTone: "warning",
      badgeLabel: "Queued",
      title: "Run next task",
      detail:
        "Backlog, policy, and routing are ready. Use Run next task for one controlled step or Worker once to advance one queued item end-to-end.",
    };
  }

  if (context.executionComplete && !context.qualityReady) {
    return {
      badgeTone: "warning",
      badgeLabel: "Quality",
      title: "Evaluate quality",
      detail:
        "Execution finished. Evaluate quality before you export, publish, or open a PR so promotion stays tied to fresh artifacts.",
    };
  }

  if (context.publicationReady) {
    return {
      badgeTone: "warning",
      badgeLabel: "Draft PR",
      title: "Create draft PR",
      detail:
        "Branch publication is complete. Create the draft PR now to move the run into GitHub review without bypassing human approval.",
    };
  }

  if (context.prExportReady) {
    return {
      badgeTone: "warning",
      badgeLabel: "Publish",
      title: "Publish PR export",
      detail:
        "The PR export bundle is ready. Publish it when you want the remote branch pushed before the GitHub PR handoff.",
    };
  }

  if (context.qualityReady && context.prCandidateReady) {
    return {
      badgeTone: "warning",
      badgeLabel: "Promote",
      title: "Export PR candidate",
      detail:
        "The run is promotable. Export the PR candidate for an explicit promotion artifact, or use Create draft PR for the full handoff.",
    };
  }

  if (context.planningReady) {
    return {
      badgeTone: "warning",
      badgeLabel: "Ready",
      title: "Start execution",
      detail:
        "The orchestrator already has the plan and routing contract. The next meaningful change is task execution.",
    };
  }

  return {
    badgeTone: "neutral",
    badgeLabel: "Waiting",
    title: "Submit a brief",
    detail:
      "No durable run state is available yet. Start from brief intake so the orchestrator can materialize a backlog and policy contract.",
  };
}

function guideHeadline(context) {
  if (context.githubPrReady) {
    return "The orchestrator already completed the GitHub handoff for this run.";
  }
  if (context.executionBlocked) {
    return "This run is blocked in execution and needs investigation before promotion can continue.";
  }
  if (context.runningTaskCount > 0 || context.runDetail.status === "executing") {
    return "The orchestrator is actively executing claimed work for this run.";
  }
  if (context.queuedTaskCount > 0 || context.runDetail.status === "queued") {
    return "Planning is done; the orchestrator is waiting for the next task to be claimed and executed.";
  }
  if (context.executionComplete && !context.qualityReady) {
    return "Task execution is complete, and the next control-plane decision is the quality gate.";
  }
  if (context.publicationReady) {
    return "The branch is already published. The remaining orchestrator handoff is the GitHub draft PR.";
  }
  if (context.prExportReady) {
    return "This run already has an export bundle and is waiting for remote publication or direct draft PR creation.";
  }
  if (context.qualityReady) {
    return "The run is promotable and the control plane can now prepare the remote PR handoff.";
  }

  return "The orchestrator has accepted the run and is ready to move it through execution, quality, and PR promotion.";
}

function guideBlockerDetail(context) {
  if (context.githubPrReady) {
    return "Merge approval remains outside the orchestrator in GitHub review and branch protection.";
  }
  if (context.executionBlocked) {
    return "Execution failed, so quality and PR promotion stay blocked until the failure path is understood and corrected.";
  }
  if (!context.executionComplete) {
    return "Quality and PR promotion stay blocked until the run finishes successfully.";
  }
  if (!context.qualityReady) {
    return "Remote promotion should wait for a quality evaluation tied to the latest artifacts.";
  }
  if (!context.prCandidateReady) {
    return "Promotion is still blocked because no pr_candidate artifact is available for this run.";
  }
  if (!context.prExportReady) {
    return "Remote branch publication stays blocked until the PR candidate is exported.";
  }
  if (!context.publicationReady) {
    return "GitHub review does not start until the export is published or the draft-PR flow runs.";
  }

  return "The remaining approval boundary is GitHub review, not another hidden orchestrator step.";
}

function guideTone(state) {
  switch (state) {
    case "complete":
      return "success";
    case "active":
      return "warning";
    case "blocked":
      return "error";
    default:
      return "neutral";
  }
}

function guideBadgeLabel(state) {
  switch (state) {
    case "complete":
      return "Done";
    case "active":
      return "Now";
    case "blocked":
      return "Blocked";
    default:
      return "Next";
  }
}

function renderTasks(tasks) {
  elements.taskHeadline.textContent = `${tasks.length} task(s) materialized`;
  if (!tasks.length) {
    elements.taskTableWrap.innerHTML = '<div class="empty-state compact">No tasks persisted for this run yet.</div>';
    return;
  }

  elements.taskTableWrap.innerHTML = `
    <div class="data-card-list">
      ${tasks
        .map(
          (task) => `
            <article class="data-card" data-ui-task-card="true">
              <div class="data-card-head">
                <div>
                  <span class="badge badge-${escapeHtml(statusTone(task.status))}">${escapeHtml(
                    task.status
                  )}</span>
                  <h4>${escapeHtml(task.title)}</h4>
                </div>
                <span class="mono">${escapeHtml(task.backlog_item_id)}</span>
              </div>
              <div class="data-card-grid">
                ${renderDataCardField(
                  "Agent",
                  escapeHtml(task.assigned_agent ?? "n/a"),
                  escapeHtml(task.orchestrator_model ?? task.agent_execution?.mode ?? "no model hint")
                )}
                ${renderDataCardField(
                  "Provider",
                  escapeHtml(task.execution?.provider ?? "n/a")
                )}
                ${renderDataCardField(
                  "Retry",
                  `${escapeHtml(task.retry_state?.retry_count ?? 0)} / ${escapeHtml(
                    task.retry_state?.max_retry_count ?? 0
                  )}`
                )}
                ${renderDataCardField(
                  "Updated",
                  escapeHtml(
                    formatTimestamp(
                      task.completed_at ?? task.lease_expires_at ?? task.started_at ?? task.created_at
                    )
                  )
                )}
              </div>
            </article>
          `
        )
        .join("")}
    </div>
    <div class="table-scroll">
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
    </div>
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
    <div class="data-card-list">
      ${artifacts
        .map(
          (artifact) => `
            <article class="data-card" data-ui-artifact-card="true">
              <div class="data-card-head">
                <div>
                  <p class="panel-kicker">${escapeHtml(artifact.artifact_type)}</p>
                  <h4>${escapeHtml(artifact.format)}</h4>
                </div>
              </div>
              <div class="data-card-grid data-card-grid-single">
                ${renderDataCardField(
                  "Location",
                  escapeHtml(artifact.location_value),
                  "",
                  "mono"
                )}
                ${renderDataCardField(
                  "Created",
                  escapeHtml(formatTimestamp(artifact.created_at))
                )}
              </div>
            </article>
          `
        )
        .join("")}
    </div>
    <div class="table-scroll">
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
    </div>
  `;
}

function renderDataCardField(label, value, detail = "", valueClass = "") {
  const detailMarkup = detail
    ? `<div class="microcopy">${detail}</div>`
    : "";
  const renderedValueClass = valueClass ? ` data-card-value-${valueClass}` : "";
  return `
    <div class="data-card-field">
      <span class="data-card-label">${escapeHtml(label)}</span>
      <strong class="data-card-value${renderedValueClass}">${value}</strong>
      ${detailMarkup}
    </div>
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

function clearRunSelection(message, title = "No run selected") {
  state.selectedRunId = null;
  state.selectedRunDetail = null;
  state.selectedRunEvents = [];
  elements.selectedRunLabel.textContent = "No run selected.";
  elements.detailEmptyState.innerHTML = renderDetailEmptyStateMarkup({
    title,
    message,
    steps:
      title === "Run detail unavailable"
        ? [
            "Refresh the dashboard if the run should still exist.",
            "Open another run from the ledger if this selection is stale.",
            "Re-submit the brief if the run was never materialized successfully.",
          ]
        : undefined,
  });
  elements.detailEmptyState.classList.remove("hidden");
  elements.runDetailShell.classList.add("hidden");
  renderRunGuide(null, []);
  renderRunActionHighlights(null);
  syncRunActionDraftInputs(null);
  syncRunActionControlsWithState();
  syncUiUrlState();
}

function renderDetailEmptyStateMarkup(options = {}) {
  const title = options.title ?? "Start with a brief, then drive the run forward";
  const message =
    options.message ??
    "Load a starter brief or paste YAML, validate it, submit it, then open the new run from Recent runs.";
  const steps = Array.isArray(options.steps)
    ? options.steps
    : [
        "Confirm pack, routing, and policy during validation.",
        "Submit the brief to materialize the run, backlog, and artifacts.",
        "Use the selected-run guide to advance execution, quality, and PR handoff.",
      ];
  const actions = Array.isArray(options.actions)
    ? options.actions
    : steps.length
      ? [
          {
            href: "#brief-intake",
            label: "Jump to brief intake",
            variant: "primary",
          },
          {
            href: "#run-ledger",
            label: "Jump to recent runs",
            variant: "ghost",
          },
        ]
      : [];

  return renderEmptyStateMarkup({
    kicker: "Operator flow",
    title,
    message,
    steps,
    actions,
    includeContainer: false,
  });
}

function renderSectionEmptyState(kicker, title, message, options = {}) {
  return renderEmptyStateMarkup({
    kicker,
    title,
    message,
    compact: true,
    steps: options.steps,
    actions: options.actions,
  });
}

function renderEmptyStateMarkup(options = {}) {
  const kicker = options.kicker ?? "";
  const title = options.title ?? "";
  const message = options.message ?? "";
  const compact = options.compact === true;
  const includeContainer = options.includeContainer !== false;
  const steps = Array.isArray(options.steps) ? options.steps : [];
  const actions = Array.isArray(options.actions) ? options.actions : [];

  const body = `
    ${kicker ? `<p class="panel-kicker">${escapeHtml(kicker)}</p>` : ""}
    ${title ? `<h3>${escapeHtml(title)}</h3>` : ""}
    ${message ? `<p>${escapeHtml(message)}</p>` : ""}
    ${
      steps.length
        ? `
          <ol class="empty-state-steps">
            ${steps.map((step) => `<li>${escapeHtml(step)}</li>`).join("")}
          </ol>
        `
        : ""
    }
    ${
      actions.length
        ? `
          <div class="empty-state-actions">
            ${actions
              .map((action) => {
                const variant =
                  action.variant === "primary"
                    ? "button-primary"
                    : action.variant === "secondary"
                      ? "button-secondary"
                      : "button-ghost";
                return `
                  <a class="button ${variant} button-link" href="${escapeHtml(action.href ?? "#")}">
                    ${escapeHtml(action.label ?? "Open")}
                  </a>
                `;
              })
              .join("")}
          </div>
        `
        : ""
    }
  `;

  return includeContainer
    ? `<div class="empty-state${compact ? " compact" : ""}">${body}</div>`
    : body;
}

function setBadge(target, tone, text) {
  target.className = `badge badge-${tone}`;
  target.textContent = text;
}

function setConsolePayload(target, payload) {
  target.innerHTML = renderConsolePayload(payload);
}

function writeConsole(target, badge, tone, payload) {
  setConsolePayload(target, payload);
  setBadge(badge, tone, toneLabel(tone));
}

function writeEnvelopeConsole(target, badge, envelope) {
  if (envelope?.ok) {
    setConsolePayload(target, envelope.data ?? "No payload returned.");
  } else {
    setConsolePayload(target, formatEnvelopeError(envelope));
  }
  const presentation = envelopeBadgePresentation(envelope);
  setBadge(badge, presentation.tone, presentation.label);
}

function writeBusyConsole(target, badge, badgeText, payload) {
  setConsolePayload(target, payload);
  setBadge(badge, "warning", badgeText);
}

function renderConsolePayload(payload) {
  if (typeof payload === "string") {
    return renderConsoleString(payload);
  }

  if (payload == null) {
    return renderConsoleString("No payload returned.");
  }

  if (Array.isArray(payload)) {
    return renderConsoleStructuredPayload(
      payload,
      [{ label: "Items", value: String(payload.length), mono: false }],
      payload.length <= 4
    );
  }

  if (typeof payload !== "object") {
    return renderConsoleString(String(payload));
  }

  const summaryEntries = consoleSummaryEntries(payload);
  return renderConsoleStructuredPayload(
    payload,
    summaryEntries,
    summaryEntries.length === 0 && JSON.stringify(payload, null, 2).length <= 640
  );
}

function renderConsoleString(value) {
  return `<pre class="console-raw console-raw-plain">${escapeHtml(value)}</pre>`;
}

function renderConsoleSummaryGrid(entries) {
  return `
    <div class="console-summary-grid">
      ${entries
        .map(
          (entry) => `
            <article class="console-summary-item">
              <span class="console-summary-label">${escapeHtml(entry.label)}</span>
              <strong class="console-summary-value${entry.mono ? " console-summary-value-mono" : ""}">
                ${escapeHtml(entry.value)}
              </strong>
            </article>
          `
        )
        .join("")}
    </div>
  `;
}

function renderConsoleStructuredPayload(payload, summaryEntries, expanded) {
  const rawPayload = escapeHtml(JSON.stringify(payload, null, 2));
  const openAttribute = expanded ? " open" : "";

  return `
    <div class="console-stack">
      ${summaryEntries.length ? renderConsoleSummaryGrid(summaryEntries) : ""}
      <details class="console-details"${openAttribute}>
        <summary>Raw JSON</summary>
        <pre class="console-raw">${rawPayload}</pre>
      </details>
    </div>
  `;
}

function consoleSummaryEntries(payload) {
  const entries = [];
  pushConsoleSummaryEntry(entries, "Run", payload.run_id, { mono: true, short: true });
  pushConsoleSummaryEntry(entries, "Brief", payload.brief_id, { mono: true, short: true });
  pushConsoleSummaryEntry(entries, "Task", payload.task_id, { mono: true, short: true });
  pushConsoleSummaryEntry(entries, "Signal", payload.signal_id, { mono: true, short: true });
  pushConsoleSummaryEntry(entries, "Delivery", payload.delivery_id, { mono: true, short: true });
  pushConsoleSummaryEntry(entries, "PR", payload.pr_number);
  pushConsoleSummaryEntry(entries, "Status", payload.status ?? payload.outcome ?? payload.routing_status);
  pushConsoleSummaryEntry(entries, "Pack", payload.target_pack ?? payload.pack ?? payload.repo_pack);
  pushConsoleSummaryEntry(entries, "Title", payload.title);
  pushConsoleSummaryEntry(entries, "Repository", consoleRepositoryName(payload));
  pushConsoleSummaryEntry(entries, "Action", payload.action ?? payload.source_action);
  pushConsoleSummaryEntry(entries, "Event", payload.event);
  pushConsoleSummaryEntry(entries, "Trigger", payload.trigger ?? payload.proposed_run_trigger);
  pushConsoleSummaryEntry(entries, "Branch", payload.branch_name ?? payload.ref_name);
  pushConsoleSummaryEntry(entries, "Tasks", consoleCollectionCount(payload.tasks, payload.total_task_count));
  pushConsoleSummaryEntry(
    entries,
    "Artifacts",
    consoleCollectionCount(payload.artifacts, payload.persisted_artifact_count)
  );
  pushConsoleSummaryEntry(entries, "Message", payload.message);

  return entries.slice(0, 6);
}

function pushConsoleSummaryEntry(entries, label, value, options = {}) {
  const formattedValue = formatConsoleSummaryValue(value, options);
  if (!formattedValue || entries.some((entry) => entry.label === label)) {
    return;
  }

  entries.push({
    label,
    value: formattedValue,
    mono: options.mono ?? false,
  });
}

function formatConsoleSummaryValue(value, options = {}) {
  if (value == null) {
    return null;
  }

  if (typeof value === "string") {
    const normalized = value.trim();
    if (!normalized) {
      return null;
    }
    return options.short ? shortId(normalized) : normalized;
  }

  if (typeof value === "number") {
    return String(value);
  }

  if (typeof value === "boolean") {
    return value ? "true" : "false";
  }

  return null;
}

function consoleRepositoryName(payload) {
  if (typeof payload.repository_full_name === "string" && payload.repository_full_name.trim()) {
    return payload.repository_full_name.trim();
  }

  const repository = payload.repository;
  if (!repository || typeof repository !== "object" || Array.isArray(repository)) {
    return null;
  }

  const owner = typeof repository.owner === "string" ? repository.owner.trim() : "";
  const name = typeof repository.name === "string" ? repository.name.trim() : "";
  if (owner && name) {
    return `${owner}/${name}`;
  }

  return null;
}

function consoleCollectionCount(value, fallback) {
  if (Array.isArray(value)) {
    return value.length;
  }

  return fallback ?? null;
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

function renderActionSummaryCard(item) {
  const safeHref = safeExternalUrl(item.href);
  const primaryClass = item.mono ? "summary-card-primary summary-card-primary-code" : "summary-card-primary";
  const secondary = item.secondary?.trim()
    ? `<p>${escapeHtml(item.secondary)}</p>`
    : '<p class="microcopy">No additional detail captured.</p>';
  const link = safeHref && item.linkLabel
    ? `
      <p class="microcopy">
        <a
          class="summary-card-link"
          href="${escapeHtml(safeHref)}"
          target="_blank"
          rel="noreferrer noopener"
        >
          ${escapeHtml(item.linkLabel)}
        </a>
      </p>
    `
    : "";

  return `
    <article class="summary-card">
      <p class="panel-kicker">${escapeHtml(item.title)}</p>
      <h3 class="${escapeHtml(primaryClass)}">${escapeHtml(item.primary)}</h3>
      ${secondary}
      ${link}
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

function envelopeBadgePresentation(envelope) {
  if (!envelope?.ok) {
    return {
      tone: "error",
      label: toneLabel("error"),
    };
  }

  return payloadBadgePresentation(envelope.data);
}

function payloadBadgePresentation(payload) {
  if (!payload || typeof payload !== "object" || Array.isArray(payload)) {
    return {
      tone: "success",
      label: toneLabel("success"),
    };
  }

  if (
    payload.outcome === "idle" ||
    payload.status === "idle" ||
    payload.runnable_request_found === false ||
    payload.pending_signal_found === false
  ) {
    return {
      tone: "neutral",
      label: toneLabel("neutral"),
    };
  }

  if (payload.valid === false) {
    return {
      tone: "warning",
      label: "Invalid",
    };
  }

  if (payload.passed === false || Number(payload.failed_check_count ?? 0) > 0) {
    return {
      tone: "warning",
      label: "Blocked",
    };
  }

  return {
    tone: "success",
    label: toneLabel("success"),
  };
}

function restoreBriefDraft() {
  const saved = window.localStorage.getItem(BRIEF_STORAGE_KEY);
  if (saved) {
    elements.briefEditor.value = saved;
  }
  elements.runStatusFilter.value = state.selectedRunStatus;
  elements.runSearchInput.value = state.runSearchQuery;
  state.runActionDrafts = restoreRunActionDrafts();
  state.autoRefresh = elements.autoRefreshToggle.checked;
  renderDashboardLoadingState();
  renderBriefExampleHint();
  renderQueueInspectorEmpty();
  renderRunGuide(null, []);
  renderRunActionHighlights(null);
  syncRunActionDraftInputs(null);
  syncRunActionControlsWithState();
}

function renderDashboardLoadingState() {
  elements.lastRefresh.textContent = "Loading first snapshot...";
  elements.packChips.innerHTML = '<span class="chip chip-loading">Loading pack catalog...</span>';
  elements.capabilityGrid.innerHTML = CAPABILITY_LOADING_CARD_TITLES
    .map(
      (title) => `
        <article class="status-card status-card-neutral status-card-loading">
          <div class="status-card-head">
            <p class="panel-kicker">${escapeHtml(title)}</p>
            <span class="badge badge-neutral">Loading</span>
          </div>
          <h3>Resolving capability...</h3>
          <p>Waiting for the first operator-readiness snapshot.</p>
          <p class="microcopy">The UI will summarize what you can do before showing raw diagnostics.</p>
        </article>
      `
    )
    .join("");
  elements.statusGrid.innerHTML = DASHBOARD_LOADING_CARD_TITLES
    .map(
      (title) => `
        <article class="status-card status-card-neutral status-card-loading">
          <div class="status-card-head">
            <p class="panel-kicker">${escapeHtml(title)}</p>
            <span class="badge badge-neutral">Loading</span>
          </div>
          <h3>Resolving status...</h3>
          <p>Waiting for the first control-plane snapshot.</p>
          <p class="microcopy">The UI will keep the current view once data arrives.</p>
        </article>
      `
    )
    .join("");
  elements.runsList.innerHTML = loadingOrEmptyState("Loading recent runs...");
  elements.webhookActionsList.innerHTML = loadingOrEmptyState("Loading webhook actions...");
  elements.repositorySignalsList.innerHTML = loadingOrEmptyState("Loading repository signals...");
  elements.webhookDeliveriesList.innerHTML = loadingOrEmptyState("Loading webhook deliveries...");
  elements.webhookActionCount.textContent = "…";
  elements.signalCount.textContent = "…";
  elements.deliveryCount.textContent = "…";

  if (state.selectedRunId) {
    elements.selectedRunLabel.textContent = `Loading run ${shortId(state.selectedRunId)}...`;
    elements.detailEmptyState.innerHTML = renderDetailEmptyStateMarkup({
      title: "Loading selected run",
      message: "Loading the selected run snapshot, tasks, artifacts, and events.",
      steps: [],
    });
  } else {
    elements.selectedRunLabel.textContent = "Loading latest run...";
    elements.detailEmptyState.innerHTML = renderDetailEmptyStateMarkup({
      title: "Loading latest run",
      message: "Loading the latest run snapshot, tasks, artifacts, and events.",
      steps: [],
    });
  }
}

function renderQueueInspectorEmpty() {
  state.selectedQueueRunId = null;
  elements.queueInspectorHeadline.textContent =
    "Queue items show what reached the orchestrator before or around run creation. Open one to inspect signed ingress, routed actions, or automation handoff state.";
  elements.queueInspectorSummary.innerHTML =
    renderSectionEmptyState(
      "Queue inspector",
      "No queue item selected",
      "Pick a delivery, action request, or repository signal to see what happened before a run was created or advanced."
    );
  elements.queueInspectorActions.classList.add("hidden");
  setConsolePayload(elements.queueInspectorConsole, "No queue item selected.");
  setConsolePayload(
    elements.queueInspectorLinkedConsole,
    "No linked receipt, report, or payload loaded."
  );
  setBadge(elements.queueInspectorPrimaryStatus, "neutral", "Idle");
  setBadge(elements.queueInspectorDetailStatus, "neutral", "Idle");
  setBadge(elements.queueInspectorLinkedStatus, "neutral", "Idle");
}

function loadingOrEmptyState(message) {
  const loadingMessage = message.includes("Loading");
  return `
    <div class="empty-state compact${loadingMessage ? " is-loading" : ""}">
      <p>${escapeHtml(message)}</p>
    </div>
  `;
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
    case "executing":
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

function displayRunStatus(value) {
  switch (value) {
    case "executing":
      return "Executing";
    case "queued":
      return "Queued";
    case "succeeded":
      return "Succeeded";
    case "failed":
      return "Failed";
    default:
      return String(value ?? "unknown");
  }
}

function displayRunActionLabel(actionId) {
  switch (actionId) {
    case "tasks-next":
      return "Run next task";
    case "worker-once":
      return "Worker once";
    case "evaluate-policy":
      return "Evaluate policy";
    case "evaluate-quality":
      return "Evaluate quality";
    case "export-pr":
      return "Export PR candidate";
    case "publish-pr":
      return "Publish PR export";
    case "draft-pr":
      return "Create draft PR";
    default:
      return String(actionId ?? "Run action");
  }
}

function actionHighlightItems(actionId, envelope) {
  if (!envelope?.ok) {
    return failedActionHighlightItems(envelope);
  }

  const payload = envelope.data;
  if (!payload || typeof payload !== "object" || Array.isArray(payload)) {
    return genericActionHighlightItems(payload);
  }

  switch (actionId) {
    case "tasks-next":
    case "worker-once":
      return taskExecutionHighlightItems(payload);
    case "evaluate-policy":
      return policyEvaluationHighlightItems(payload);
    case "evaluate-quality":
      return qualityEvaluationHighlightItems(payload);
    case "export-pr":
      return exportPrHighlightItems(payload);
    case "publish-pr":
      return publishPrHighlightItems(payload);
    case "draft-pr":
      return draftPrHighlightItems(payload);
    default:
      return genericActionHighlightItems(payload);
  }
}

function failedActionHighlightItems(envelope) {
  const items = [];
  pushActionHighlightItem(
    items,
    "Result",
    envelope.status ? `HTTP ${envelope.status}` : "Request failed",
    formatEnvelopeError(envelope)
  );
  if (envelope.statusText) {
    pushActionHighlightItem(items, "Status text", envelope.statusText, "Transport-level failure");
  }
  return items;
}

function taskExecutionHighlightItems(payload) {
  const items = [];

  if (payload.outcome === "idle") {
    pushActionHighlightItem(
      items,
      "Result",
      "Idle",
      payload.run_status ? `Run status: ${displayRunStatus(payload.run_status)}` : "No runnable task found."
    );
    if (payload.run_id) {
      pushActionHighlightItem(items, "Run", shortId(payload.run_id), "No runnable task remained for the selected run.", { mono: true });
    }
    return items;
  }

  const taskTitle = payload.task?.title ?? payload.task?.kind ?? payload.task?.task_id;
  const taskSubtitle = payload.task?.task_id
    ? `${payload.task.task_id} · ${payload.task?.status ?? payload.execution_status ?? "unknown"}`
    : payload.task?.status ?? payload.execution_status ?? "unknown";
  pushActionHighlightItem(items, "Task", taskTitle, taskSubtitle, { mono: !payload.task?.title });
  pushActionHighlightItem(
    items,
    "Execution",
    payload.execution_status ?? "unknown",
    `Exit ${payload.exit_code ?? "n/a"} · retry ${payload.retry_scheduled ? "scheduled" : "not scheduled"}`
  );
  pushActionHighlightItem(
    items,
    "Runtime",
    payload.provider ?? "unknown",
    payload.image ?? `Run status: ${displayRunStatus(payload.run_status)}`
  );
  pushActionHighlightItem(
    items,
    "Artifacts",
    String(Array.isArray(payload.artifacts) ? payload.artifacts.length : 0),
    "Artifacts persisted from this execution step."
  );
  return items;
}

function policyEvaluationHighlightItems(payload) {
  const items = [];
  const checkCount = Array.isArray(payload.checks) ? payload.checks.length : 0;
  pushActionHighlightItem(
    items,
    "Result",
    payload.passed ? "Passed" : "Blocked",
    `${payload.failed_check_count ?? 0} failing of ${checkCount} check(s)`
  );
  pushActionHighlightItem(
    items,
    "Run status",
    displayRunStatus(payload.run_status),
    payload.policy_present ? "Explicit policy was evaluated." : "No explicit policy block was present."
  );
  pushActionHighlightItem(
    items,
    "Target pack",
    payload.target_pack ?? "unassigned",
    "Policy checks were resolved against this pack contract."
  );
  if (payload.artifact?.artifact_id) {
    pushActionHighlightItem(
      items,
      "Report artifact",
      shortId(payload.artifact.artifact_id),
      payload.artifact.artifact_type ?? "policy_report",
      { mono: true }
    );
  }
  return items;
}

function qualityEvaluationHighlightItems(payload) {
  const items = [];
  const checkCount = Array.isArray(payload.checks) ? payload.checks.length : 0;
  pushActionHighlightItem(
    items,
    "Result",
    payload.passed ? "Passed" : "Blocked",
    `${payload.failed_check_count ?? 0} failing of ${checkCount} check(s)`
  );
  pushActionHighlightItem(
    items,
    "Run status",
    displayRunStatus(payload.run_status),
    payload.target_pack ?? "No target pack recorded"
  );
  if (payload.source_pr_candidate_artifact_id) {
    pushActionHighlightItem(
      items,
      "Promotion source",
      shortId(payload.source_pr_candidate_artifact_id),
      "Latest promotable pr_candidate artifact covered by this quality gate.",
      { mono: true }
    );
  }
  if (payload.artifact?.artifact_id) {
    pushActionHighlightItem(
      items,
      "Report artifact",
      shortId(payload.artifact.artifact_id),
      payload.artifact.artifact_type ?? "quality_report",
      { mono: true }
    );
  }
  return items;
}

function exportPrHighlightItems(payload) {
  const items = [];
  pushActionHighlightItem(items, "Branch", payload.branch_name, payload.commit_sha ?? "Branch resolved for PR export.");
  pushActionHighlightItem(items, "Commit", payload.commit_sha, "Head commit captured in the exported PR bundle.", { mono: true });
  if (payload.artifact?.location_value) {
    pushActionHighlightItem(
      items,
      "Export bundle",
      payload.artifact.artifact_type ?? "pr_export",
      payload.artifact.location_value
    );
  }
  if (payload.source_quality_report_artifact_id) {
    pushActionHighlightItem(
      items,
      "Quality gate",
      shortId(payload.source_quality_report_artifact_id),
      "PR export is anchored to this passed quality report.",
      { mono: true }
    );
  }
  return items;
}

function publishPrHighlightItems(payload) {
  const items = [];
  pushActionHighlightItem(
    items,
    "Publication",
    payload.push_status ?? "unknown",
    payload.base_branch ? `Targets ${payload.base_branch}` : "Publication state recorded."
  );
  pushActionHighlightItem(items, "Head branch", payload.head_branch, payload.base_branch ?? "Base branch unavailable.");
  pushActionHighlightItem(items, "Remote", payload.remote_url, "Remote used for publication.", { mono: true });
  if (payload.artifact?.location_value) {
    pushActionHighlightItem(
      items,
      "Publication artifact",
      payload.artifact.artifact_type ?? "pr_publication",
      payload.artifact.location_value
    );
  }
  return items;
}

function draftPrHighlightItems(payload) {
  const items = [];
  pushActionHighlightItem(
    items,
    "Draft PR",
    payload.pr_number != null ? `#${payload.pr_number}` : payload.resolution ?? "created",
    payload.resolution ?? "GitHub draft PR result",
    {
      href: payload.pr_url,
      linkLabel: payload.pr_url ? "Open pull request" : "",
    }
  );
  pushActionHighlightItem(items, "Branch", payload.branch_name, payload.commit_sha ?? "Branch exported and published for the draft PR.");
  pushActionHighlightItem(
    items,
    "Publication",
    payload.push_status ?? "unknown",
    payload.base_branch ? `Targets ${payload.base_branch}` : payload.remote_url ?? "Remote unavailable."
  );
  pushActionHighlightItem(items, "Remote", payload.remote_url, "Remote used for draft PR publication.", { mono: true });
  return items;
}

function genericActionHighlightItems(payload) {
  const items = [];
  if (payload && typeof payload === "object" && !Array.isArray(payload)) {
    if (payload.status) {
      pushActionHighlightItem(items, "Status", String(payload.status), "Latest structured status returned by the control plane.");
    }
    if (payload.outcome) {
      pushActionHighlightItem(items, "Outcome", String(payload.outcome), "Latest structured outcome returned by the control plane.");
    }
    if (payload.run_id) {
      pushActionHighlightItem(items, "Run", shortId(payload.run_id), "Selected run referenced by this action result.", { mono: true });
    }
    if (payload.artifact?.artifact_id) {
      pushActionHighlightItem(
        items,
        "Artifact",
        shortId(payload.artifact.artifact_id),
        payload.artifact.artifact_type ?? "artifact",
        { mono: true }
      );
    }
  }

  if (items.length === 0) {
    pushActionHighlightItem(
      items,
      "Result",
      "Captured",
      "The full structured payload remains available in Action Output."
    );
  }
  return items;
}

function pushActionHighlightItem(items, title, primary, secondary, options = {}) {
  if (primary == null) {
    return;
  }

  const renderedPrimary = String(primary).trim();
  if (!renderedPrimary) {
    return;
  }

  items.push({
    title,
    primary: renderedPrimary,
    secondary: String(secondary ?? "").trim(),
    href: options.href ?? "",
    linkLabel: options.linkLabel ?? "",
    mono: options.mono === true,
  });
}

function safeExternalUrl(value) {
  if (typeof value !== "string" || !value.trim()) {
    return null;
  }

  try {
    const parsed = new URL(value);
    if (parsed.protocol !== "http:" && parsed.protocol !== "https:") {
      return null;
    }
    return parsed.toString();
  } catch (_error) {
    return null;
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

function buttonIdleLabel(button) {
  return button?.dataset.idleLabel ?? button?.textContent ?? "";
}

function setButtonBusyState(button, busyLabel, busy) {
  if (!button) {
    return;
  }

  if (!button.dataset.idleLabel) {
    button.dataset.idleLabel = button.textContent;
  }

  button.disabled = busy;
  button.textContent = busy ? busyLabel : buttonIdleLabel(button);
}

function runActionButtons() {
  return Array.from(elements.runDetailShell.querySelectorAll("[data-run-action]"));
}

function setBriefControlsBusyState(mode, busy) {
  const activeButton =
    mode === "submit" ? elements.submitBriefButton : elements.validateBriefButton;
  const otherButton =
    mode === "submit" ? elements.validateBriefButton : elements.submitBriefButton;

  setButtonBusyState(activeButton, BRIEF_BUSY_LABELS[mode], busy);
  if (otherButton) {
    otherButton.disabled = busy;
  }
  if (elements.clearBriefButton) {
    elements.clearBriefButton.disabled = busy;
  }
}

function setAutomationControlsBusyState(activeButton, busyLabel, busy) {
  setButtonBusyState(activeButton, busyLabel, busy);
  for (const button of [
    elements.runNextWebhookButton,
    elements.submitNextSignalButton,
    elements.runRepositoryAutomationButton,
  ]) {
    if (button && button !== activeButton) {
      button.disabled = busy;
    }
  }
  elements.automationActionFilter.disabled = busy;
  elements.automationSignalKindInput.disabled = busy;
}

function runActionBusyLabel(actionId) {
  return RUN_ACTION_BUSY_LABELS[actionId] ?? "Running action...";
}

function setRunActionControlsBusyState(actionId, busy) {
  const activeButton = elements.runDetailShell.querySelector(
    `[data-run-action="${actionId}"]`
  );

  if (!busy) {
    syncRunActionControlsWithState();
    return;
  }

  for (const button of runActionButtons()) {
    setButtonBusyState(
      button,
      button === activeButton ? runActionBusyLabel(actionId) : buttonIdleLabel(button),
      button === activeButton
    );
    if (button !== activeButton) {
      button.disabled = true;
    }
    button.title = "";
  }

  elements.branchNameInput.disabled = true;
  elements.remoteUrlInput.disabled = true;
  elements.publishPushToggle.disabled = true;
  elements.resetRunActionDraftButton.disabled = true;
  elements.branchNameInput.title = "";
  elements.remoteUrlInput.title = "";
  elements.publishPushToggle.title = "";
  elements.resetRunActionDraftButton.title = "";
}

function disabledRunAction(reason) {
  return {
    enabled: false,
    reason,
  };
}

function enabledRunAction() {
  return {
    enabled: true,
    reason: "",
  };
}

function runArtifactTypes(runDetail) {
  return new Set(
    (runDetail?.artifacts ?? runDetail?.artifact_highlights ?? []).map(
      (artifact) => artifact.artifact_type
    )
  );
}

function prCandidateAvailability(runDetail, artifactTypes, actionLabel) {
  if (runDetail.status !== "succeeded") {
    return disabledRunAction(
      `Run status is ${displayRunStatus(runDetail.status)}; ${actionLabel} unlocks after the run succeeds.`
    );
  }

  if (!artifactTypes.has(PR_CANDIDATE_ARTIFACT_TYPE)) {
    return disabledRunAction(
      `${actionLabel} requires a pr_candidate artifact. Complete the run before promoting it.`
    );
  }

  return enabledRunAction();
}

function runActionAvailability(runDetail) {
  if (!runDetail) {
    return {
      "tasks-next": disabledRunAction("Select a run to use operator actions."),
      "worker-once": disabledRunAction("Select a run to use operator actions."),
      "evaluate-policy": disabledRunAction("Select a run to use operator actions."),
      "evaluate-quality": disabledRunAction("Select a run to use operator actions."),
      "export-pr": disabledRunAction("Select a run to use operator actions."),
      "publish-pr": disabledRunAction("Select a run to use operator actions."),
      "draft-pr": disabledRunAction("Select a run to use operator actions."),
    };
  }

  const queuedTaskCount = Number(runDetail.task_counts?.queued ?? 0);
  const artifactTypes = runArtifactTypes(runDetail);

  return {
    "tasks-next":
      queuedTaskCount > 0
        ? enabledRunAction()
        : disabledRunAction("No queued tasks remain for this run."),
    "worker-once":
      queuedTaskCount > 0
        ? enabledRunAction()
        : disabledRunAction("No queued tasks remain for this run."),
    "evaluate-policy": enabledRunAction(),
    "evaluate-quality": enabledRunAction(),
    "export-pr": prCandidateAvailability(runDetail, artifactTypes, "PR export"),
    "publish-pr":
      runDetail.status !== "succeeded"
        ? disabledRunAction(
            `Run status is ${displayRunStatus(runDetail.status)}; PR publication unlocks after the run succeeds.`
          )
        : !artifactTypes.has(PR_EXPORT_ARTIFACT_TYPE)
          ? disabledRunAction(
              "PR publication requires a pr_export artifact. Export the PR candidate first."
            )
          : enabledRunAction(),
    "draft-pr": prCandidateAvailability(runDetail, artifactTypes, "Draft PR creation"),
  };
}

function runActionHintText(runDetail, availability) {
  if (!runDetail) {
    return "Select a run to inspect which operator actions are currently available.";
  }

  const artifactTypes = runArtifactTypes(runDetail);
  const eventTypes = new Set(
    (state.selectedRunEvents ?? []).map((event) => event.event_type)
  );
  const runningTaskCount = Number(runDetail.task_counts?.running ?? 0);
  const queuedTaskCount = Number(runDetail.task_counts?.queued ?? 0);
  const qualityReady =
    artifactTypes.has(QUALITY_REPORT_ARTIFACT_TYPE) ||
    eventTypes.has(RUN_QUALITY_EVALUATED_EVENT_TYPE);
  const prPublicationReady =
    artifactTypes.has(PR_PUBLICATION_ARTIFACT_TYPE) ||
    eventTypes.has(PR_EXPORT_PUBLISHED_EVENT_TYPE);
  const githubPrReady =
    artifactTypes.has(GITHUB_PULL_REQUEST_ARTIFACT_TYPE) ||
    eventTypes.has(GITHUB_PR_OPENED_EVENT_TYPE);

  if (runDetail.status === "failed") {
    return "This run is blocked. Inspect Tasks and Run events before retrying or promoting anything further.";
  }

  if (runningTaskCount > 0 || runDetail.status === "executing") {
    return "A task is currently running. Let it finish, then refresh or use Worker once if you are manually draining the queue.";
  }

  if (queuedTaskCount > 0 || runDetail.status === "queued") {
    return "Backlog, routing, and policy are ready. Use Run next task for one controlled step or Worker once to advance the queue.";
  }

  if (runDetail.status !== "succeeded") {
    return "PR promotion actions unlock only after execution succeeds.";
  }

  if (!qualityReady) {
    return "Execution is complete. Evaluate quality before exporting or publishing PR material.";
  }

  if (!artifactTypes.has(PR_CANDIDATE_ARTIFACT_TYPE)) {
    return "Promotion stays blocked until the run has a pr_candidate artifact.";
  }

  if (githubPrReady) {
    return "The orchestrator already opened or reused the draft PR. Remaining approval now sits in GitHub review.";
  }

  if (prPublicationReady) {
    return "Branch publication is done. Create the draft PR next when you want GitHub review to start.";
  }

  if (!artifactTypes.has(PR_EXPORT_ARTIFACT_TYPE)) {
    return "Quality is available. Export the PR candidate next, or use Create draft PR for the full GitHub handoff.";
  }

  if (!availability["publish-pr"].enabled) {
    return availability["publish-pr"].reason || "PR publication is not ready yet.";
  }

  return "Promotion controls are available. Publish the export or create the draft PR when you are ready for the GitHub handoff.";
}

function syncRunActionControlsWithState() {
  const availability = runActionAvailability(state.selectedRunDetail);
  syncRunActionDraftInputs(state.selectedRunDetail);

  for (const button of runActionButtons()) {
    const actionId = button.dataset.runAction;
    const actionState = availability[actionId] ?? disabledRunAction("Action unavailable.");

    if (!button.dataset.idleLabel) {
      button.dataset.idleLabel = button.textContent;
    }
    button.textContent = buttonIdleLabel(button);
    button.disabled = state.runActionInFlight || !actionState.enabled;
    button.title = actionState.enabled ? "" : actionState.reason;
  }

  const branchActionsEnabled =
    availability["export-pr"].enabled || availability["draft-pr"].enabled;
  const remoteActionsEnabled =
    availability["publish-pr"].enabled || availability["draft-pr"].enabled;

  elements.branchNameInput.disabled = state.runActionInFlight || !branchActionsEnabled;
  elements.remoteUrlInput.disabled = state.runActionInFlight || !remoteActionsEnabled;
  elements.publishPushToggle.disabled =
    state.runActionInFlight || !availability["publish-pr"].enabled;
  elements.resetRunActionDraftButton.disabled =
    state.runActionInFlight || !state.selectedRunDetail;
  elements.branchNameInput.title = branchActionsEnabled
    ? ""
    : availability["draft-pr"].reason || availability["export-pr"].reason;
  elements.remoteUrlInput.title = remoteActionsEnabled
    ? ""
    : availability["draft-pr"].reason || availability["publish-pr"].reason;
  elements.publishPushToggle.title = availability["publish-pr"].enabled
    ? ""
    : availability["publish-pr"].reason;
  elements.resetRunActionDraftButton.title = state.selectedRunDetail
    ? ""
    : "Select a run before resetting promotion defaults.";
  elements.runActionHint.textContent = runActionHintText(
    state.selectedRunDetail,
    availability
  );
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
