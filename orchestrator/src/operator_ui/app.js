const BRIEF_STORAGE_KEY = "catalystContinuum.operatorUi.brief";
const RUN_ACTION_DRAFTS_STORAGE_KEY = "catalystContinuum.operatorUi.runActionDrafts";
const AUTO_REFRESH_STORAGE_KEY = "catalystContinuum.operatorUi.autoRefresh";
const AUTOMATION_DISCLOSURES_STORAGE_KEY =
  "catalystContinuum.operatorUi.automationDisclosures";
const MISSION_TAB_STORAGE_KEY = "catalystContinuum.operatorUi.missionTab";
const AUTO_REFRESH_INTERVAL_MS = 15000;
const REALTIME_RECONNECT_DELAY_MS = 1500;
const REALTIME_BOOTSTRAP_FALLBACK_DELAY_MS = 2500;
const PENDING_AUTOMATION_STATUS = "pending";
const DEFAULT_GRAFANA_PORT = "3000";
const DEFAULT_PROMETHEUS_PORT = "9090";
const DEFAULT_LOKI_PORT = "3100";
const DEFAULT_TEMPO_PORT = "3200";
const DEFAULT_LITELLM_PORT = "4000";
const MISSION_SURFACE_EMBED_KEYS = ["grafana", "litellm"];
const GRAFANA_OVERVIEW_DASHBOARD_PATH =
  "/d/catalyst-continuum-overview/catalyst-continuum-overview?orgId=1&refresh=10s&kiosk";
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
  "Real repository guard",
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
const RUN_SUBMITTED_EVENT_TYPE = "run_submitted";
const RUN_STATUS_CHANGED_EVENT_TYPE = "run_status_changed";
const RUN_POLICY_EVALUATED_EVENT_TYPE = "run_policy_evaluated";
const RUN_QUALITY_EVALUATED_EVENT_TYPE = "run_quality_evaluated";
const PR_CANDIDATE_EXPORTED_EVENT_TYPE = "pr_candidate_exported";
const PR_EXPORT_PUBLISHED_EVENT_TYPE = "pr_export_published";
const GITHUB_PR_OPENED_EVENT_TYPE = "github_pr_opened";
const TASK_STARTED_EVENT_TYPE = "task_started";
const TASK_WORKSPACE_PREPARED_EVENT_TYPE = "task_workspace_prepared";
const TASK_HEARTBEAT_EVENT_TYPE = "task_heartbeat";
const TASK_SUCCEEDED_EVENT_TYPE = "task_succeeded";
const TASK_FAILED_EVENT_TYPE = "task_failed";
const TASK_REQUEUED_EVENT_TYPE = "task_requeued";
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
const RUN_ACTION_SEQUENCE = [
  "tasks-next",
  "worker-once",
  "evaluate-policy",
  "evaluate-quality",
  "export-pr",
  "publish-pr",
  "draft-pr",
];
const RUN_EVENT_FILTERS = [
  {
    id: "all",
    label: "All",
    types: null,
  },
  {
    id: "tasks",
    label: "Tasks",
    types: [
      TASK_STARTED_EVENT_TYPE,
      TASK_WORKSPACE_PREPARED_EVENT_TYPE,
      TASK_HEARTBEAT_EVENT_TYPE,
      TASK_SUCCEEDED_EVENT_TYPE,
      TASK_FAILED_EVENT_TYPE,
      TASK_REQUEUED_EVENT_TYPE,
    ],
  },
  {
    id: "quality",
    label: "Policy & quality",
    types: [
      RUN_POLICY_EVALUATED_EVENT_TYPE,
      RUN_QUALITY_EVALUATED_EVENT_TYPE,
    ],
  },
  {
    id: "handoff",
    label: "PR handoff",
    types: [
      PR_CANDIDATE_EXPORTED_EVENT_TYPE,
      PR_EXPORT_PUBLISHED_EVENT_TYPE,
      GITHUB_PR_OPENED_EVENT_TYPE,
    ],
  },
  {
    id: "run-state",
    label: "Run state",
    types: [
      RUN_SUBMITTED_EVENT_TYPE,
      RUN_STATUS_CHANGED_EVENT_TYPE,
    ],
  },
];
const initialUiUrl = new URL(window.location.href);

const state = {
  selectedRunId: normalizeRunIdQueryParam(initialUiUrl.searchParams.get("run")),
  selectedRunStatus: normalizeRunStatusFilter(initialUiUrl.searchParams.get("status")),
  runSearchQuery: normalizeRunSearchQuery(initialUiUrl.searchParams.get("run_query")),
  selectedRunEventFilter: "all",
  selectedRunDetail: null,
  selectedRunEvents: [],
  dashboardSnapshot: {
    readyz: null,
    aiGateway: null,
    surfaces: null,
    config: null,
    packs: null,
  },
  briefExamples: [],
  activeBriefExampleId: null,
  autoRefresh: false,
  refreshInFlight: false,
  refreshAnimationsEnabled: false,
  lastHttpRefreshAt: null,
  lastRealtimeSnapshotAt: null,
  realtimeSupported: typeof window.WebSocket === "function",
  realtimeSocket: null,
  realtimeSocketToken: 0,
  realtimeReconnectTimer: null,
  realtimeConnected: false,
  realtimeConnecting: false,
  realtimeHasConnected: false,
  briefRequestInFlight: false,
  automationRequestInFlight: false,
  runActionInFlight: false,
  runActionBusyActionId: null,
  latestRuns: [],
  latestWebhookActions: [],
  latestRepositorySignals: [],
  latestWebhookDeliveries: [],
  runActionDrafts: {},
  runActionResults: {},
  automationDisclosurePreferences: {},
  selectedQueueItem: null,
  selectedQueueRunId: null,
  activeMissionTab: restoreMissionTabPreference(),
  missionSurfaceEmbeds: {
    grafana: false,
    litellm: false,
  },
  selectedAgentActivityId: "all",
  selectedAgentReportArtifactId: null,
  selectedAgentLogArtifactId: null,
  agentReportDetails: {},
  agentReportLoadsInFlight: {},
  agentLogDetails: {},
  agentLogLoadsInFlight: {},
  agentLinkedArtifactDetails: {},
  agentLinkedArtifactLoadsInFlight: {},
  agentPanelRenderQueued: false,
};

const elements = {};

document.addEventListener("DOMContentLoaded", () => {
  cacheElements();
  bindEvents();
  syncMissionTabSelection();
  restoreBriefDraft();
  if (supportsRealtimeUpdates()) {
    connectRealtime();
    scheduleRealtimeBootstrapFallback();
  } else {
    refreshDashboard().catch(reportBootstrapRefreshFailure);
  }
  loadBriefExamples().catch((error) => {
    console.error("brief example load failed", error);
    renderBriefExamplesError(error.message);
  });
  if (!supportsRealtimeUpdates()) {
    window.setInterval(() => {
      if (!state.autoRefresh || state.refreshInFlight) {
        return;
      }

      refreshDashboard().catch((error) => {
        console.error("operator UI auto refresh failed", error);
      });
    }, AUTO_REFRESH_INTERVAL_MS);
  }
  window.addEventListener("beforeunload", () => {
    cleanupRealtimeSocket();
  });
});

function reportBootstrapRefreshFailure(error) {
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
}

function scheduleRealtimeBootstrapFallback() {
  window.setTimeout(() => {
    if (state.lastRealtimeSnapshotAt || state.refreshInFlight) {
      return;
    }

    refreshDashboard().catch((error) => {
      console.error("operator UI bootstrap HTTP fallback failed", error);
    });
  }, REALTIME_BOOTSTRAP_FALLBACK_DELAY_MS);
}

function cacheElements() {
  const ids = [
    "actionConsole",
    "actionConsoleStatus",
    "actionHighlights",
    "actionSummaryHeadline",
    "artifactEvidenceStrip",
    "artifactHeadline",
    "artifactHighlights",
    "artifactTableWrap",
    "automationActionFilter",
    "automationConsole",
    "automationConsoleStatus",
    "automationSignalKindInput",
    "autoRefreshToggle",
    "autoRefreshToggleShell",
    "branchNameInput",
    "briefConsole",
    "briefConsoleStatus",
    "briefEditor",
    "briefExampleHint",
    "briefExamples",
    "briefReadinessBadge",
    "briefReadinessList",
    "capabilityGrid",
    "clearBriefButton",
    "deliveryCount",
    "detailEmptyState",
    "eventFilterBar",
    "eventHeadline",
    "eventTimeline",
    "lastRefresh",
    "missionAgentsPanel",
    "missionFlowPanel",
    "missionGrafanaPanel",
    "missionLitellmPanel",
    "missionShell",
    "missionTabAgentsBadge",
    "missionTabBar",
    "missionTabFlowBadge",
    "missionTabGrafanaBadge",
    "missionTabHint",
    "missionTabLitellmBadge",
    "operatorDockSummary",
    "packChips",
    "pulseFeed",
    "pulseSummary",
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
    "repositoryTargetSelect",
    "repositorySignalsList",
    "runActionHint",
    "runActionDraftHint",
    "runGuideBadge",
    "runGuideActionButton",
    "runGuideActionHint",
    "runGuideBlockers",
    "runGuideCurrentStage",
    "runGuideCurrentStageDetail",
    "runGuideHeadline",
    "runGuideNextAction",
    "runGuideProgressMeter",
    "runGuideProgressSummary",
    "runGuideRecommendation",
    "runGuideStages",
    "runOutcomeBanner",
    "runSectionNav",
    "runLedgerHint",
    "runSearchInput",
    "runNextWebhookButton",
    "runControlReadinessBoard",
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
    "webhookActionsDisclosure",
    "taskTableWrap",
    "validateBriefButton",
    "repositorySignalsDisclosure",
    "webhookActionCount",
    "webhookActionsList",
    "webhookDeliveriesDisclosure",
    "webhookDeliveriesList",
    "queueInspectorDisclosure",
    "resetRunActionDraftButton",
  ];

  for (const id of ids) {
    elements[id] = document.getElementById(id);
  }

  elements.pageShell = document.getElementById("pageShell");
}

function bindEvents() {
  elements.refreshButton.addEventListener("click", () => {
    refreshDashboard().catch((error) => {
      console.error("manual refresh failed", error);
    });
  });

  elements.missionTabBar.addEventListener("click", (event) => {
    const tabButton = event.target.closest("[data-mission-tab]");
    if (!tabButton) {
      return;
    }

    setActiveMissionTab(tabButton.dataset.missionTab);
  });

  elements.missionShell.addEventListener("click", (event) => {
    const embedButton = event.target.closest("[data-mission-surface-embed]");
    if (embedButton) {
      setMissionSurfaceEmbed(embedButton.dataset.missionSurfaceEmbed);
      return;
    }

    const missionActionButton = event.target.closest("[data-run-action]");
    if (missionActionButton) {
      executeRunAction(missionActionButton.dataset.runAction).catch((error) => {
        console.error("mission run action failed", error);
      });
      return;
    }

    const agentFilterButton = event.target.closest("[data-agent-filter]");
    if (agentFilterButton) {
      setSelectedAgentActivity(agentFilterButton.dataset.agentFilter);
      return;
    }

    const reportButton = event.target.closest("[data-agent-report-artifact-id]");
    if (reportButton) {
      setSelectedAgentReport(reportButton.dataset.agentReportArtifactId);
      return;
    }

    const logButton = event.target.closest("[data-agent-log-artifact-id]");
    if (logButton) {
      setSelectedAgentLog(logButton.dataset.agentLogArtifactId);
    }
  });

  elements.operatorDockSummary.addEventListener("click", (event) => {
    const actionButton = event.target.closest("[data-run-action]");
    if (!actionButton) {
      return;
    }

    executeRunAction(actionButton.dataset.runAction).catch((error) => {
      console.error("operator dock run action failed", error);
    });
  });

  elements.autoRefreshToggle.addEventListener("change", () => {
    state.autoRefresh = elements.autoRefreshToggle.checked;
    persistAutoRefreshPreference();
    renderLastRefreshStatus();
  });

  elements.runStatusFilter.addEventListener("change", () => {
    state.selectedRunStatus = elements.runStatusFilter.value;
    syncUiUrlState();
    refreshFromPreferredSource({ forceRealtime: true }).catch((error) => {
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

    selectRun(runButton.dataset.runId, {
      revealDetail: isCompactViewport(),
    }).catch((error) => {
      console.error("run selection failed", error);
    });
  });

  elements.briefEditor.addEventListener("input", () => {
    window.localStorage.setItem(BRIEF_STORAGE_KEY, elements.briefEditor.value);
    renderBriefReadiness();
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
    renderBriefReadiness();
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

    selectRun(state.selectedQueueRunId, { revealDetail: true }).catch((error) => {
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

  elements.eventFilterBar.addEventListener("click", (event) => {
    const filterButton = event.target.closest("[data-run-event-filter]");
    if (!filterButton) {
      return;
    }

    setSelectedRunEventFilter(filterButton.dataset.runEventFilter);
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

  elements.repositoryTargetSelect.addEventListener("change", () => {
    updateSelectedRunActionDraft({
      repositoryTargetId: elements.repositoryTargetSelect.value,
    });
    applyRepositoryTargetDefaultsToInputs(state.selectedRunDetail);
  });

  elements.publishPushToggle.addEventListener("change", () => {
    updateSelectedRunActionDraft({
      push: elements.publishPushToggle.checked,
    });
  });

  elements.resetRunActionDraftButton.addEventListener("click", () => {
    resetSelectedRunActionDraft();
  });

  for (const disclosure of automationDisclosureElements()) {
    disclosure.addEventListener("toggle", () => {
      state.automationDisclosurePreferences[disclosure.id] = disclosure.open;
      persistAutomationDisclosurePreferences();
    });
  }
}

function isCompactViewport() {
  return window.matchMedia("(max-width: 960px)").matches;
}

function automationDisclosureElements() {
  return [
    elements.webhookActionsDisclosure,
    elements.repositorySignalsDisclosure,
    elements.webhookDeliveriesDisclosure,
    elements.queueInspectorDisclosure,
  ].filter(Boolean);
}

function restoreAutomationDisclosurePreferences() {
  const raw = window.localStorage.getItem(AUTOMATION_DISCLOSURES_STORAGE_KEY);
  if (!raw) {
    return {};
  }

  try {
    const parsed = JSON.parse(raw);
    return sanitizeAutomationDisclosurePreferences(parsed);
  } catch (error) {
    console.error("failed to restore automation disclosure preferences", error);
    return {};
  }
}

function sanitizeAutomationDisclosurePreferences(value) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return {};
  }

  return Object.fromEntries(
    Object.entries(value).filter(([, open]) => typeof open === "boolean")
  );
}

function persistAutomationDisclosurePreferences() {
  window.localStorage.setItem(
    AUTOMATION_DISCLOSURES_STORAGE_KEY,
    JSON.stringify(state.automationDisclosurePreferences)
  );
}

function disclosureDefaultOpenState(disclosureId) {
  switch (disclosureId) {
    case "webhookActionsDisclosure":
      return state.latestWebhookActions.length > 0;
    case "repositorySignalsDisclosure":
      return state.latestRepositorySignals.length > 0;
    case "webhookDeliveriesDisclosure":
      return false;
    case "queueInspectorDisclosure":
      return Boolean(state.selectedQueueItem);
    default:
      return false;
  }
}

function syncAutomationDisclosures() {
  for (const disclosure of automationDisclosureElements()) {
    const saved = state.automationDisclosurePreferences[disclosure.id];
    const shouldOpen =
      typeof saved === "boolean" ? saved : disclosureDefaultOpenState(disclosure.id);
    if (disclosure.open !== shouldOpen) {
      disclosure.open = shouldOpen;
    }
  }
}

function openAutomationDisclosure(disclosure) {
  if (!disclosure) {
    return;
  }

  if (!disclosure.open) {
    disclosure.open = true;
  }
  state.automationDisclosurePreferences[disclosure.id] = true;
  persistAutomationDisclosurePreferences();
}

function revealSelectedRunDetail() {
  document
    .querySelector(".detail-panel")
    ?.scrollIntoView({ behavior: "smooth", block: "start" });
}

function supportsRealtimeUpdates() {
  return state.realtimeSupported;
}

function realtimeSocketUrl() {
  const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
  const url = new URL(`${protocol}//${window.location.host}/ui/ws`);
  if (state.selectedRunId) {
    url.searchParams.set("run", state.selectedRunId);
  }
  if (state.selectedRunStatus) {
    url.searchParams.set("status", state.selectedRunStatus);
  }
  return url.toString();
}

function clearRealtimeReconnect() {
  if (state.realtimeReconnectTimer) {
    window.clearTimeout(state.realtimeReconnectTimer);
    state.realtimeReconnectTimer = null;
  }
}

function cleanupRealtimeSocket() {
  clearRealtimeReconnect();
  if (state.realtimeSocket) {
    state.realtimeSocket.onopen = null;
    state.realtimeSocket.onmessage = null;
    state.realtimeSocket.onerror = null;
    state.realtimeSocket.onclose = null;
    if (
      state.realtimeSocket.readyState === window.WebSocket.OPEN ||
      state.realtimeSocket.readyState === window.WebSocket.CONNECTING
    ) {
      state.realtimeSocket.close();
    }
  }
  state.realtimeSocket = null;
  state.realtimeConnecting = false;
  state.realtimeConnected = false;
}

function scheduleRealtimeReconnect() {
  if (!supportsRealtimeUpdates() || state.realtimeReconnectTimer) {
    return;
  }

  state.realtimeReconnectTimer = window.setTimeout(() => {
    state.realtimeReconnectTimer = null;
    connectRealtime({ force: true });
  }, REALTIME_RECONNECT_DELAY_MS);
}

function connectRealtime(options = {}) {
  if (!supportsRealtimeUpdates()) {
    return;
  }

  const nextUrl = realtimeSocketUrl();
  const forceReconnect = options.force === true;

  if (
    !forceReconnect &&
    state.realtimeSocket &&
    state.realtimeSocket.readyState === window.WebSocket.OPEN &&
    state.realtimeSocket.url === nextUrl
  ) {
    return;
  }

  clearRealtimeReconnect();
  cleanupRealtimeSocket();

  const token = state.realtimeSocketToken + 1;
  state.realtimeSocketToken = token;
  let socket;
  try {
    socket = new window.WebSocket(nextUrl);
  } catch (error) {
    state.realtimeSupported = false;
    state.realtimeConnecting = false;
    state.realtimeConnected = false;
    console.error("operator UI websocket could not start", error);
    syncRefreshModeControls();
    refreshDashboard().catch((refreshError) => {
      console.error("operator UI websocket fallback refresh failed", refreshError);
    });
    renderLastRefreshStatus();
    return;
  }

  state.realtimeSocket = socket;
  state.realtimeConnecting = true;
  state.realtimeConnected = false;
  renderLastRefreshStatus();

  socket.onopen = () => {
    if (token !== state.realtimeSocketToken) {
      socket.close();
      return;
    }

    state.realtimeConnecting = false;
    state.realtimeConnected = true;
    state.realtimeHasConnected = true;
    renderLastRefreshStatus();
  };

  socket.onmessage = (event) => {
    if (token !== state.realtimeSocketToken) {
      return;
    }

    handleRealtimeMessage(event.data);
  };

  socket.onerror = (error) => {
    if (token !== state.realtimeSocketToken) {
      return;
    }

    console.error("operator UI websocket error", error);
  };

  socket.onclose = () => {
    if (token !== state.realtimeSocketToken) {
      return;
    }

    state.realtimeSocket = null;
    state.realtimeConnecting = false;
    state.realtimeConnected = false;
    renderLastRefreshStatus();
    scheduleRealtimeReconnect();
  };
}

function handleRealtimeMessage(rawMessage) {
  let message;
  try {
    message = JSON.parse(rawMessage);
  } catch (error) {
    console.error("failed to parse operator UI websocket message", error, rawMessage);
    return;
  }
  if (!message || typeof message !== "object") {
    console.warn("operator UI websocket message was not an object", message);
    return;
  }

  switch (message.type) {
    case "hello":
    case "heartbeat":
      return;
    case "dashboard_snapshot":
      state.lastRealtimeSnapshotAt = new Date().toISOString();
      renderLastRefreshStatus();
      applyRealtimeDashboardSnapshot(message.snapshot);
      return;
    case "runs_snapshot":
      state.lastRealtimeSnapshotAt = new Date().toISOString();
      renderLastRefreshStatus();
      applyRealtimeRunsSnapshot(message.response);
      return;
    case "automation_snapshot":
      state.lastRealtimeSnapshotAt = new Date().toISOString();
      renderLastRefreshStatus();
      applyRealtimeAutomationSnapshot(message.response);
      return;
    case "run_detail_snapshot":
      state.lastRealtimeSnapshotAt = new Date().toISOString();
      renderLastRefreshStatus();
      applyRealtimeRunDetailSnapshot(message.response);
      return;
    case "selected_run_missing":
      state.lastRealtimeSnapshotAt = new Date().toISOString();
      renderLastRefreshStatus();
      if (message.run_id === state.selectedRunId) {
        clearRunSelection(message.error, "Run detail unavailable");
        connectRealtime({ force: true });
      }
      return;
    default:
      console.warn("unknown operator UI websocket message", message);
  }
}

function applyRealtimeDashboardSnapshot(snapshot = {}) {
  renderStatusGrid({
    readyz: snapshot.readyz ?? failedEnvelope(new Error("missing readyz snapshot")),
    aiGateway:
      snapshot.ai_gateway ?? failedEnvelope(new Error("missing AI gateway snapshot")),
    surfaces:
      snapshot.surfaces ?? failedEnvelope(new Error("missing surface snapshot")),
    config: snapshot.config ?? failedEnvelope(new Error("missing config snapshot")),
    packs: snapshot.packs ?? failedEnvelope(new Error("missing packs snapshot")),
  });
  renderPackChips(snapshot.packs?.data);
}

function applyRealtimeRunsSnapshot(response) {
  const runs = Array.isArray(response?.runs) ? response.runs : [];
  state.latestRuns = runs;
  renderRuns({ runs });

  if (runs.length === 0 && !state.selectedRunId) {
    clearRunSelection(
      "Load a starter brief, validate it, submit it, then open the new run from this ledger.",
      "No runs materialized yet"
    );
    return;
  }

  if (!state.selectedRunId && runs[0]?.run_id) {
    void selectRun(runs[0].run_id);
    return;
  }

  if (state.selectedRunId && !runs.some((run) => run.run_id === state.selectedRunId)) {
    clearRunSelection(
      "Refresh the dashboard or open another run from the ledger if the previous selection is no longer present.",
      "Run detail unavailable"
    );
  }
}

function applyRealtimeAutomationSnapshot(response) {
  renderAutomationRail({
    webhookActions: response?.webhook_actions,
    repositorySignals: response?.repository_signals,
    webhookDeliveries: response?.webhook_deliveries,
  });
}

function applyRealtimeRunDetailSnapshot(response) {
  if (!response?.run || response.run.run_id !== state.selectedRunId) {
    return;
  }

  renderRunDetail(response.run, { events: response.events });
}

function refreshFromPreferredSource(options = {}) {
  if (supportsRealtimeUpdates()) {
    connectRealtime({ force: options.forceRealtime === true });
    return Promise.resolve();
  }

  return refreshDashboard();
}

async function refreshDashboard() {
  state.refreshInFlight = true;
  setDashboardRefreshState(true);
  elements.refreshButton.disabled = true;
  elements.refreshButton.textContent = "Refreshing...";

  try {
    const dashboardEnvelopePromise = fetchJsonEnvelope("/ui/dashboard");
    const [
      runsEnvelope,
      webhookActionsEnvelope,
      signalsEnvelope,
      deliveriesEnvelope,
    ] = await Promise.all([
      fetchJsonEnvelope(buildRunsPath()),
      fetchJsonEnvelope(buildPendingWebhookActionsPath()),
      fetchJsonEnvelope(buildPendingRepositorySignalsPath()),
      fetchJsonEnvelope("/github/webhooks?limit=8"),
    ]);
    const runs = Array.isArray(runsEnvelope.data?.runs) ? runsEnvelope.data.runs : [];
    state.latestRuns = runs;
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
      await loadRunDetail(state.selectedRunId, { background: true });
    } else {
      await selectRun(runs[0].run_id);
    }

    const dashboardEnvelope = await dashboardEnvelopePromise;
    const readyzEnvelope =
      dashboardEnvelope.data?.readyz ??
      failedEnvelope(
        new Error(
          dashboardEnvelope.ok
            ? "missing readyz snapshot"
            : formatEnvelopeError(dashboardEnvelope)
        )
      );
    const aiGatewayEnvelope =
      dashboardEnvelope.data?.ai_gateway ??
      failedEnvelope(
        new Error(
          dashboardEnvelope.ok
            ? "missing AI gateway snapshot"
            : formatEnvelopeError(dashboardEnvelope)
        )
      );
    const surfacesEnvelope =
      dashboardEnvelope.data?.surfaces ??
      failedEnvelope(
        new Error(
          dashboardEnvelope.ok
            ? "missing surface snapshot"
            : formatEnvelopeError(dashboardEnvelope)
        )
      );
    const configEnvelope =
      dashboardEnvelope.data?.config ??
      failedEnvelope(
        new Error(
          dashboardEnvelope.ok
            ? "missing config snapshot"
            : formatEnvelopeError(dashboardEnvelope)
        )
      );
    const packsEnvelope =
      dashboardEnvelope.data?.packs ??
      failedEnvelope(
        new Error(
          dashboardEnvelope.ok
            ? "missing packs snapshot"
            : formatEnvelopeError(dashboardEnvelope)
        )
      );

    renderStatusGrid({
      readyz: readyzEnvelope,
      aiGateway: aiGatewayEnvelope,
      surfaces: surfacesEnvelope,
      config: configEnvelope,
      packs: packsEnvelope,
    });
    renderPackChips(packsEnvelope.data);

    state.lastHttpRefreshAt = new Date().toISOString();
    renderLastRefreshStatus();
    state.refreshAnimationsEnabled = true;
  } finally {
    state.refreshInFlight = false;
    setDashboardRefreshState(false);
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

function buildPendingWebhookActionsPath() {
  const params = new URLSearchParams({
    limit: "8",
    status: PENDING_AUTOMATION_STATUS,
  });
  return `/github/webhook-actions?${params.toString()}`;
}

function buildPendingRepositorySignalsPath() {
  const params = new URLSearchParams({
    limit: "8",
    status: PENDING_AUTOMATION_STATUS,
  });
  return `/repository-signals?${params.toString()}`;
}

function normalizeRunStatusFilter(value) {
  const candidate = String(value ?? "");
  return RUN_STATUS_FILTER_VALUES.has(candidate) ? candidate : "";
}

function normalizeRunIdQueryParam(value) {
  const candidate = String(value ?? "").trim();
  return /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(
    candidate
  )
    ? candidate
    : "";
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
  connectRealtime({ force: true });
  if (options.revealDetail) {
    revealSelectedRunDetail();
  }
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

async function loadRunDetail(runId, options = {}) {
  const backgroundRefresh =
    options.background === true && state.selectedRunDetail?.run_id === runId;
  if (!backgroundRefresh) {
    state.selectedRunDetail = null;
    syncRunActionControlsWithState();
  }
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
      if (supportsRealtimeUpdates()) {
        syncUiUrlState();
        await loadRunDetail(envelope.data.run_id);
        connectRealtime({ force: true });
      } else {
        await refreshDashboard();
      }
      revealSelectedRunDetail();
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

    await refreshFromPreferredSource({ forceRealtime: true });
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

    await refreshFromPreferredSource({ forceRealtime: true });
    if (envelope.ok && submittedRunId) {
      revealSelectedRunDetail();
    }
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

    await refreshFromPreferredSource({ forceRealtime: true });
    if (envelope.ok && submittedRunId) {
      revealSelectedRunDetail();
    }
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
  state.runActionBusyActionId = actionId;
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

    await refreshFromPreferredSource({ forceRealtime: true });
  } finally {
    state.runActionInFlight = false;
    state.runActionBusyActionId = null;
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
  const repositoryTargetId = elements.repositoryTargetSelect.value.trim();
  const push = elements.publishPushToggle.checked;

  switch (actionId) {
    case "export-pr":
      return {
        ...(branchName ? { branch_name: branchName } : {}),
        ...(repositoryTargetId ? { repository_target_id: repositoryTargetId } : {}),
      };
    case "publish-pr":
      return {
        ...(remoteUrl ? { remote_url: remoteUrl } : {}),
        ...(repositoryTargetId ? { repository_target_id: repositoryTargetId } : {}),
        push,
      };
    case "draft-pr":
      return {
        ...(remoteUrl ? { remote_url: remoteUrl } : {}),
        ...(branchName ? { branch_name: branchName } : {}),
        ...(repositoryTargetId ? { repository_target_id: repositoryTargetId } : {}),
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
      repositoryTargetId: "",
      push: false,
    };
  }

  return {
    branchName: typeof draft.branchName === "string" ? draft.branchName : "",
    remoteUrl: typeof draft.remoteUrl === "string" ? draft.remoteUrl : "",
    repositoryTargetId:
      typeof draft.repositoryTargetId === "string" ? draft.repositoryTargetId : "",
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

function repositoryTargetsConfig() {
  return state.dashboardSnapshot.config?.data?.repository_targets ?? {};
}

function enabledRepositoryTargets() {
  const targets = repositoryTargetsConfig().targets;
  return Array.isArray(targets) ? targets.filter((target) => target.enabled) : [];
}

function repositoryTargetMatchesRun(target, runDetail) {
  const repository = runDetail?.repository ?? {};
  const host = repository.host ?? "github";
  return (
    target?.host === host &&
    target?.owner === repository.owner &&
    target?.name === repository.name &&
    (!repository.default_branch || target.default_branch === repository.default_branch)
  );
}

function matchingRepositoryTargetsForRun(runDetail) {
  return enabledRepositoryTargets().filter((target) =>
    repositoryTargetMatchesRun(target, runDetail)
  );
}

function repositoryLabel(runDetail) {
  return runDetail?.repository?.owner && runDetail?.repository?.name
    ? `${runDetail.repository.owner}/${runDetail.repository.name}`
    : "";
}

function repositoryTargetGuardSummary(runDetail) {
  const repository = repositoryLabel(runDetail);
  const enforcementEnabled = repositoryTargetsConfig().enforcement_enabled === true;
  const matchingTargets = matchingRepositoryTargetsForRun(runDetail);

  if (!repository) {
    return {
      tone: "neutral",
      title: "No repository metadata",
      detail:
        "This run has no repository metadata yet, so real GitHub publication cannot resolve an allowlisted target. PR export can still stay local until a matching repository target exists.",
    };
  }

  if (!enforcementEnabled) {
    return {
      tone: "warning",
      title: "Guard inactive",
      detail:
        `${repository} can still use repository-derived or manual remotes, but real GitHub publication is not pinned to an allowlist.`,
    };
  }

  if (matchingTargets.length === 1) {
    const target = matchingTargets[0];
    return {
      tone: "success",
      title: `Matching target ${target.target_id}`,
      detail:
        `${repository} -> ${target.default_branch} with branch prefix ${target.branch_prefix || "continuum/"}.`,
    };
  }

  if (matchingTargets.length > 1) {
    return {
      tone: "warning",
      title: "Multiple matching targets",
      detail:
        `${repository} matches ${matchingTargets.length} allowlist entries, so choose one repository target before remote publication or draft PR.`,
    };
  }

  return {
    tone: "error",
    title: "No matching repository target",
    detail:
      `${repository} is outside the active allowlist, so PR export can still create a local promotion artifact, but publish and draft PR stay blocked until repository targets are updated.`,
  };
}

function repositoryTargetById(targetId) {
  return enabledRepositoryTargets().find((target) => target.target_id === targetId) ?? null;
}

function defaultRepositoryTargetId(runDetail) {
  const matches = matchingRepositoryTargetsForRun(runDetail);
  return matches.length === 1 ? matches[0].target_id : "";
}

function repositoryTargetRemoteUrl(target) {
  if (!target) {
    return "";
  }
  const remotes = Array.isArray(target.allowed_remote_urls)
    ? target.allowed_remote_urls.filter(Boolean)
    : [];
  if (remotes.length > 0) {
    return remotes[0];
  }
  return target.host === "github" && target.owner && target.name
    ? `https://github.com/${target.owner}/${target.name}.git`
    : "";
}

function repositoryTargetBranchName(target, runId) {
  if (!target || !runId) {
    return "";
  }
  const prefix = target.branch_prefix || "continuum/";
  return `${prefix}run-${String(runId).slice(0, 8)}`;
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
  const repositoryTargetId = defaultRepositoryTargetId(runDetail);
  const repositoryTarget = repositoryTargetById(repositoryTargetId);
  return {
    branchName:
      repositoryTargetBranchName(repositoryTarget, runDetail?.run_id) ||
      defaultPromotionBranchName(runDetail?.run_id),
    remoteUrl: repositoryTargetRemoteUrl(repositoryTarget) || inferredRemoteUrl(runDetail),
    repositoryTargetId,
    push: false,
  };
}

function runActionDraftForRun(runDetail) {
  if (!runDetail?.run_id) {
    return defaultRunActionDraft(null);
  }

  const defaults = defaultRunActionDraft(runDetail);
  const stored = sanitizeRunActionDraft(state.runActionDrafts[runDetail.run_id]);
  const storedTarget = repositoryTargetById(stored.repositoryTargetId);
  const storedTargetMatches =
    stored.repositoryTargetId && repositoryTargetMatchesRun(storedTarget, runDetail);

  return {
    branchName: stored.branchName || defaults.branchName,
    remoteUrl: stored.remoteUrl || defaults.remoteUrl,
    repositoryTargetId: storedTargetMatches
      ? stored.repositoryTargetId
      : defaults.repositoryTargetId,
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

function applyRepositoryTargetDefaultsToInputs(runDetail) {
  if (!runDetail?.run_id) {
    return;
  }
  const repositoryTargetId = elements.repositoryTargetSelect.value;
  const repositoryTarget = repositoryTargetById(repositoryTargetId);
  if (!repositoryTarget) {
    return;
  }

  const nextDraft = {
    ...runActionDraftForRun(runDetail),
    repositoryTargetId,
  };
  const branchName = repositoryTargetBranchName(repositoryTarget, runDetail.run_id);
  const remoteUrl = repositoryTargetRemoteUrl(repositoryTarget);
  if (branchName) {
    nextDraft.branchName = branchName;
  }
  if (remoteUrl) {
    nextDraft.remoteUrl = remoteUrl;
  }

  saveRunActionDraft(runDetail.run_id, nextDraft);
  syncRunActionDraftInputs(runDetail);
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
  if (typeof payload.repository_target_id === "string" && payload.repository_target_id.trim()) {
    nextDraft.repositoryTargetId = payload.repository_target_id;
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
    renderRepositoryTargetOptions(null, "");
    elements.publishPushToggle.checked = false;
    renderRunActionDraftHint(null);
    return;
  }

  const draft = runActionDraftForRun(runDetail);
  renderRepositoryTargetOptions(runDetail, draft.repositoryTargetId);
  elements.branchNameInput.value = draft.branchName;
  elements.remoteUrlInput.value = draft.remoteUrl;
  elements.repositoryTargetSelect.value = draft.repositoryTargetId;
  elements.publishPushToggle.checked = draft.push;
  renderRunActionDraftHint(runDetail);
}

function renderRepositoryTargetOptions(runDetail, selectedTargetId) {
  const targets = matchingRepositoryTargetsForRun(runDetail);
  const options = [
    '<option value="">Use run repository or manual remote</option>',
    ...targets.map((target) => {
      const label = `${target.target_id} · ${target.owner}/${target.name}:${target.default_branch}`;
      return `<option value="${escapeHtml(target.target_id)}">${escapeHtml(label)}</option>`;
    }),
  ];
  setRenderedHtml(elements.repositoryTargetSelect, options.join(""), { markUpdated: false });
  elements.repositoryTargetSelect.value =
    targets.some((target) => target.target_id === selectedTargetId) ? selectedTargetId : "";
}

function renderRunActionDraftHint(runDetail) {
  if (!runDetail?.run_id) {
    elements.runActionDraftHint.textContent =
      "Promotion defaults appear once a run is selected.";
    return;
  }

  const defaults = defaultRunActionDraft(runDetail);
  const draft = runActionDraftForRun(runDetail);
  const enforcementEnabled = repositoryTargetsConfig().enforcement_enabled === true;
  const matchingTargets = matchingRepositoryTargetsForRun(runDetail);
  const messages = [];

  if (draft.repositoryTargetId) {
    messages.push(`Repository target ${draft.repositoryTargetId} is selected.`);
  } else if (enforcementEnabled && matchingTargets.length === 1) {
    messages.push(
      `Matching repository target ${matchingTargets[0].target_id} is available; select it to lock promotion defaults.`
    );
  } else if (enforcementEnabled && matchingTargets.length > 1) {
    messages.push(
      "Multiple repository targets match this run; choose one before remote publication or draft PR. PR export can still stay local."
    );
  } else if (enforcementEnabled) {
    messages.push(
      "No matching repository target exists for this run; PR export can still stay local, but publish and draft PR will stay blocked until the allowlist is updated."
    );
  } else {
    messages.push(
      "Repository-target guard is inactive; repository-derived or manual remotes remain available for smoke and trusted dev flows."
    );
  }

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
  state.dashboardSnapshot = {
    readyz: payload.readyz ?? null,
    aiGateway: payload.aiGateway ?? null,
    surfaces: payload.surfaces ?? null,
    config: payload.config ?? null,
    packs: payload.packs ?? null,
  };
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
  const repositoryTargets = config.repository_targets ?? {};
  const githubApp = config.github_app ?? {};
  const capabilityCards = [
    renderStatusCard(briefRunCapabilityCard(payload.readyz?.ok, packs.pack_count ?? 0)),
    renderStatusCard(runtimeExecutionCapabilityCard(enabledRuntimeCount, runtimeStatuses)),
    renderStatusCard(modelGatewayCapabilityCard(gateway)),
    renderStatusCard(repositoryTargetCapabilityCard(repositoryTargets)),
    renderStatusCard(githubHandoffCapabilityCard(githubApp)),
  ];

  setRenderedHtml(elements.capabilityGrid, capabilityCards.join(""));

  setRenderedHtml(elements.statusGrid, [
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
      title: "Repository targets",
      statusClass: repositoryTargets.enforcement_enabled
        ? repositoryTargets.targets?.some((target) => target.enabled) ? "success" : "error"
        : "warning",
      badge: repositoryTargets.enforcement_enabled ? "allowlist" : "unrestricted",
      primary: repositoryTargets.enforcement_enabled
        ? `${repositoryTargets.targets?.filter((target) => target.enabled).length ?? 0} enabled target(s)`
        : "No real-repo allowlist active",
      secondary: summarizeRepositoryTargets(repositoryTargets),
      detail: `Config ${friendlySourcePath(repositoryTargets.source_path, "not configured")}`,
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
  ].join(""));

  if (state.selectedRunDetail?.run_id) {
    renderRunDetail(state.selectedRunDetail, { events: state.selectedRunEvents });
    return;
  }

  renderMissionControl();
}

function renderOperatorDock() {
  if (!elements.operatorDockSummary) {
    return;
  }

  const cards = buildOperatorDockCards();
  setRenderedHtml(
    elements.operatorDockSummary,
    cards.map(renderOperatorDockCard).join(""),
    { markUpdated: false }
  );
}

function buildOperatorDockCards() {
  if (state.selectedRunDetail) {
    return buildSelectedRunDockCards(state.selectedRunDetail);
  }

  const focusCard = buildOperatorPulseFocusCard();
  const transportCard = liveTransportPulseCard();
  const estateCard = runEstatePulseCard();
  const automationCard = automationBacklogPulseCard();

  return [
    operatorDockCard({
      actionHref: focusCard.actionHref,
      actionLabel: focusCard.actionLabel,
      detail: focusCard.detail,
      key: "start",
      kicker: focusCard.kicker,
      title: focusCard.title,
      tone: focusCard.tone,
    }),
    operatorDockCard({
      detail: transportCard.summary,
      key: "transport",
      kicker: transportCard.kicker,
      title: transportCard.title,
      tone: transportCard.tone,
    }),
    operatorDockCard({
      actionHref: estateCard.actionHref,
      actionLabel: estateCard.actionLabel,
      detail: estateCard.summary,
      key: "run-estate",
      kicker: estateCard.kicker,
      title: estateCard.title,
      tone: estateCard.tone,
    }),
    operatorDockCard({
      actionHref: automationCard.actionHref,
      actionLabel: automationCard.actionLabel,
      detail: automationCard.summary,
      key: "automation",
      kicker: automationCard.kicker,
      title: automationCard.title,
      tone: automationCard.tone,
    }),
  ];
}

function buildSelectedRunDockCards(runDetail) {
  const guide = buildRunGuide(runDetail, state.selectedRunEvents);
  const taskCounts = normalizedTaskCounts(runDetail.task_counts);
  const agentCount = runAgents(runDetail).length;
  const handoffItems = buildReviewHandoffItems(runDetail);
  const handoffReadyCount = handoffItems.filter((item) => item.ready).length;
  const actionState = guide.nextActionControlId
    ? runActionAvailability(runDetail)[guide.nextActionControlId]
    : null;
  const actionEnabled =
    actionState?.enabled === true &&
    state.runActionInFlight !== true;

  return [
    operatorDockCard({
      actionHref: "#run-detail",
      actionLabel: "Open guide",
      detail: `${shortId(runDetail.run_id)} · ${displayRunStatus(runDetail.status)} · ${repositoryLabel(runDetail) || "No repository target"}`,
      key: "selected-run",
      kicker: "Selected run",
      title: runDetail.title,
      tone: statusTone(runDetail.status),
    }),
    operatorDockCard({
      actionDisabled: !actionEnabled,
      actionHint: actionState?.reason || guide.nextActionDetail,
      actionId: guide.nextActionControlId,
      actionLabel: guide.nextActionControlId
        ? displayRunActionLabel(guide.nextActionControlId)
        : "",
      detail: guide.nextActionDetail,
      key: "next-step",
      kicker: "Next step",
      title: guide.nextActionTitle,
      tone: guide.badgeTone,
    }),
    operatorDockCard({
      actionHref: "#mission-control",
      actionLabel: "Open agents",
      detail: `${agentCount} agent lane(s) · ${taskCounts.running} running · ${taskCounts.failed} failed`,
      key: "execution",
      kicker: "Execution",
      title: `${taskCounts.succeeded}/${taskCounts.total} task(s) complete`,
      tone:
        taskCounts.failed > 0
          ? "error"
          : taskCounts.running > 0 || taskCounts.queued > 0
            ? "warning"
            : taskCounts.total > 0
              ? "success"
              : "neutral",
    }),
    operatorDockCard({
      actionHref: "#run-detail",
      actionLabel: "Review evidence",
      detail: guide.progressSummary,
      key: "handoff",
      kicker: "Review handoff",
      title: `${handoffReadyCount}/${handoffItems.length} handoff checks ready`,
      tone: handoffReadyCount === handoffItems.length ? "success" : "warning",
    }),
  ];
}

function operatorDockCard({
  actionDisabled = false,
  actionHint = "",
  actionHref = "",
  actionId = "",
  actionLabel = "",
  detail,
  key,
  kicker,
  title,
  tone,
}) {
  return {
    actionDisabled,
    actionHint,
    actionHref,
    actionId,
    actionLabel,
    detail,
    key,
    kicker,
    title,
    tone: normalizePulseTone(tone),
  };
}

function renderOperatorDockCard(card) {
  const action = renderOperatorDockCardAction(card);

  return `
    <article
      class="operator-dock-card operator-dock-card-${escapeHtml(card.tone)}"
      data-operator-dock-card="${escapeHtml(card.key)}"
    >
      <div class="operator-dock-card-copy">
        <div class="mission-feed-head">
          <p class="panel-kicker">${escapeHtml(card.kicker)}</p>
          <span class="badge badge-${escapeHtml(card.tone)}">${escapeHtml(
            toneLabel(card.tone)
          )}</span>
        </div>
        <h3>${escapeHtml(card.title)}</h3>
        <p>${escapeHtml(card.detail)}</p>
        ${
          card.actionHint
            ? `<p class="microcopy">${escapeHtml(card.actionHint)}</p>`
            : ""
        }
      </div>
      ${action}
    </article>
  `;
}

function renderOperatorDockCardAction(card) {
  if (card.actionId) {
    const busy =
      state.runActionInFlight && state.runActionBusyActionId === card.actionId;
    const disabledAttr = card.actionDisabled || busy ? " disabled" : "";
    const titleAttr = card.actionHint ? ` title="${escapeHtml(card.actionHint)}"` : "";

    return `
      <button
        class="button ${escapeHtml(card.tone === "warning" ? "button-primary" : "button-ghost")}"
        type="button"
        data-run-action="${escapeHtml(card.actionId)}"
        ${disabledAttr}${titleAttr}
      >
        ${escapeHtml(busy ? runActionBusyLabel(card.actionId) : card.actionLabel)}
      </button>
    `;
  }

  if (card.actionHref && card.actionLabel) {
    return `
      <a class="button button-ghost button-link" href="${escapeHtml(card.actionHref)}">
        ${escapeHtml(card.actionLabel)}
      </a>
    `;
  }

  return "";
}

function renderOperatorPulse() {
  if (!elements.pulseSummary || !elements.pulseFeed) {
    return;
  }

  renderOperatorDock();

  const cards = [
    buildOperatorPulseFocusCard(),
    liveTransportPulseCard(),
    runEstatePulseCard(),
    automationBacklogPulseCard(),
  ];
  const feedItems = buildOperatorPulseFeedItems();

  setRenderedHtml(
    elements.pulseSummary,
    cards.map(renderPulseCard).join("")
  );
  setRenderedHtml(
    elements.pulseFeed,
    feedItems.length
      ? feedItems.map(renderPulseFeedItem).join("")
      : renderSectionEmptyState(
          "Live activity",
          "Waiting for orchestration movement",
          "Once a run, queue item, or run event changes, the latest movement appears here."
        )
  );
}

function liveTransportPulseCard() {
  const fallbackMode = supportsRealtimeUpdates()
    ? "Manual HTTP refresh remains the fallback path when live transport drops."
    : state.autoRefresh
      ? `HTTP polling every ${AUTO_REFRESH_INTERVAL_MS / 1000}s stays ready between manual refreshes.`
      : "Manual HTTP refresh remains the active refresh path.";

  if (state.realtimeConnected) {
    return {
      key: "transport",
      tone: "success",
      badge: "Live",
      kicker: "Transport",
      title: "WebSocket stream is connected",
      summary: "Selected-run, run-ledger, and queue surfaces update in place when state changes.",
      detail: `Selected-run, run-ledger, and queue surfaces now update in place. ${fallbackMode}`,
    };
  }

  if (state.realtimeConnecting) {
    return {
      key: "transport",
      tone: "warning",
      badge: "Reconnect",
      kicker: "Transport",
      title: "Live transport is reconnecting",
      summary: "The browser is trying to restore the persistent control-plane stream.",
      detail: fallbackMode,
    };
  }

  if (supportsRealtimeUpdates()) {
    return {
      key: "transport",
      tone: state.lastHttpRefreshAt ? "warning" : "neutral",
      badge: state.lastHttpRefreshAt ? "Fallback" : "Waiting",
      kicker: "Transport",
      title: state.lastHttpRefreshAt
        ? "Fell back to HTTP refresh"
        : "Waiting for the live stream",
      summary: state.lastHttpRefreshAt
        ? `Last HTTP snapshot ${new Date(state.lastHttpRefreshAt).toLocaleTimeString()}.`
        : "No live snapshot has been received yet.",
      detail: fallbackMode,
    };
  }

  return {
    key: "transport",
    tone: "neutral",
    badge: "HTTP",
    kicker: "Transport",
    title: "Browser is using the HTTP control surface",
    summary: state.lastHttpRefreshAt
      ? `Last refresh ${new Date(state.lastHttpRefreshAt).toLocaleTimeString()}.`
      : "No snapshot has been loaded yet.",
    detail: fallbackMode,
  };
}

function runEstatePulseCard() {
  const counts = loadedRunStatusCounts(state.latestRuns);
  const inFlightCount = counts.queued + counts.executing;

  if (!counts.total) {
    return {
      key: "run-estate",
      tone: "neutral",
      badge: "Idle",
      kicker: "Run estate",
      title: "No loaded runs yet",
      summary: "A run appears here only after brief submission materializes backlog, policy, and routing state.",
      detail: "Start from brief intake, validate the brief, then submit it to create the first durable run.",
      actionLabel: "Jump to brief intake",
      actionHref: "#brief-intake",
      actionVariant: "primary",
    };
  }

  return {
    key: "run-estate",
    tone: inFlightCount > 0 ? "warning" : counts.failed > 0 ? "error" : "success",
    badge: `${counts.total} loaded`,
    kicker: "Run estate",
    title:
      inFlightCount > 0
        ? `${inFlightCount} loaded run(s) need attention`
        : counts.failed > 0
          ? `${counts.failed} loaded run(s) are blocked`
          : "Loaded run estate is calm",
    summary: `${counts.executing} executing · ${counts.queued} queued · ${counts.succeeded} succeeded · ${counts.failed} failed`,
    detail: state.selectedRunStatus
      ? `Current ledger filter: ${displayRunStatus(state.selectedRunStatus)}. Loaded runs stay scoped to the current HTTP/WebSocket query.`
      : "These counts reflect the currently loaded run ledger, not hidden or paged-out runs.",
    actionLabel: "Open run ledger",
    actionHref: "#run-ledger",
    actionVariant: "ghost",
  };
}

function automationBacklogPulseCard() {
  const webhookActionCount = state.latestWebhookActions.length;
  const signalCount = state.latestRepositorySignals.length;
  const deliveryCount = state.latestWebhookDeliveries.length;
  const automationCount = webhookActionCount + signalCount;

  if (!automationCount && !deliveryCount) {
    return {
      key: "automation",
      tone: "neutral",
      badge: "Quiet",
      kicker: "Automation",
      title: "Automation queues are quiet",
      summary: "No loaded webhook actions, repository signals, or inbound deliveries need operator attention right now.",
      detail: "The right rail stays optional until GitHub ingress or repository-signal automation should drive work.",
      actionLabel: "Open automation rail",
      actionHref: "#automation-rail",
      actionVariant: "ghost",
    };
  }

  return {
    key: "automation",
    tone: automationCount > 0 ? "warning" : "success",
    badge: `${automationCount} pending`,
    kicker: "Automation",
    title:
      automationCount > 0
        ? "Automation backlog is waiting"
        : "Ingress is arriving without queued follow-up",
    summary: `${webhookActionCount} pending webhook action(s) · ${signalCount} pending signal(s) · ${deliveryCount} loaded deliver${deliveryCount === 1 ? "y" : "ies"}`,
    detail:
      automationCount > 0
        ? "Open the automation rail when GitHub-driven runs should be advanced or materialized without the manual brief-first path."
        : "Inbound deliveries are already preserved for audit, even though no follow-up queue item is waiting right now.",
    actionLabel: "Inspect automation rail",
    actionHref: "#automation-rail",
    actionVariant: automationCount > 0 ? "primary" : "ghost",
  };
}

function buildOperatorPulseFocusCard() {
  if (state.selectedRunDetail) {
    const guide = buildRunGuide(state.selectedRunDetail, state.selectedRunEvents);
    return {
      key: "current-focus",
      emphasis: true,
      tone: normalizePulseTone(guide.badgeTone),
      badge: guide.badgeLabel,
      kicker: "Current focus",
      title: guide.nextActionTitle,
      summary: guide.nextActionDetail,
      detail: `${guide.currentStageTitle} · ${guide.progressSummary}.`,
      actionLabel: guide.nextActionControlId ? "Jump to selected run guide" : "Review selected run guide",
      actionHref: "#run-detail",
      actionVariant: guide.nextActionControlId ? "primary" : "ghost",
    };
  }

  const activeRun = state.latestRuns.find((run) => run.status === "executing");
  if (activeRun) {
    return {
      key: "current-focus",
      emphasis: true,
      tone: "warning",
      badge: "Running",
      kicker: "Current focus",
      title: `Monitor ${activeRun.title}`,
      summary: "The orchestrator already has active work in flight. Open the run to inspect the current stage and live events.",
      detail: `${shortId(activeRun.run_id)} · ${activeRun.target_pack ?? "no pack"} · ${activeRun.trigger}`,
      actionLabel: "Open run ledger",
      actionHref: "#run-ledger",
      actionVariant: "primary",
    };
  }

  const queuedRun = state.latestRuns.find((run) => run.status === "queued");
  if (queuedRun) {
    return {
      key: "current-focus",
      emphasis: true,
      tone: "warning",
      badge: "Queued",
      kicker: "Current focus",
      title: `Start ${queuedRun.title}`,
      summary: "Planning is complete. The next useful operator step is to claim or execute the next queued task.",
      detail: `${shortId(queuedRun.run_id)} · ${queuedRun.target_pack ?? "no pack"} · ${queuedRun.trigger}`,
      actionLabel: "Open selected run area",
      actionHref: "#run-detail",
      actionVariant: "primary",
    };
  }

  if (state.latestWebhookActions.length || state.latestRepositorySignals.length) {
    return {
      key: "current-focus",
      emphasis: true,
      tone: "warning",
      badge: "Automation",
      kicker: "Current focus",
      title: "Decide whether automation should materialize the next run",
      summary: "GitHub-driven queue items are present. Use the automation rail only if this repository should advance from webhook and signal state instead of manual brief submission.",
      detail: `${state.latestWebhookActions.length} webhook action(s) · ${state.latestRepositorySignals.length} signal(s) loaded`,
      actionLabel: "Open automation rail",
      actionHref: "#automation-rail",
      actionVariant: "primary",
    };
  }

  return {
    key: "current-focus",
    emphasis: true,
    tone: "neutral",
    badge: "Start",
    kicker: "Current focus",
    title: "Submit the next brief",
    summary: "No run is selected and no automation backlog needs immediate attention, so the fastest path is still a fresh brief-driven run.",
    detail: "Use a starter brief or paste YAML, then validate before submission so routing and policy remain explicit.",
    actionLabel: "Jump to brief intake",
    actionHref: "#brief-intake",
    actionVariant: "primary",
  };
}

function renderPulseCard(card) {
  const classes = [
    "pulse-card",
    `pulse-card-${card.tone}`,
    card.emphasis ? "pulse-card-emphasis" : "",
  ]
    .filter(Boolean)
    .join(" ");

  return `
    <article class="${escapeHtml(classes)}" data-pulse-card="${escapeHtml(card.key)}">
      <div class="pulse-card-head">
        <p class="panel-kicker">${escapeHtml(card.kicker)}</p>
        <span class="badge badge-${escapeHtml(card.tone)}">${escapeHtml(card.badge)}</span>
      </div>
      <h3>${escapeHtml(card.title)}</h3>
      <p>${escapeHtml(card.summary)}</p>
      <p class="microcopy">${escapeHtml(card.detail)}</p>
      ${
        card.actionLabel && card.actionHref
          ? `
            <div class="pulse-card-actions">
              <a
                class="button ${escapeHtml(
                  card.actionVariant === "ghost" ? "button-ghost" : "button-primary"
                )} button-link"
                href="${escapeHtml(card.actionHref)}"
              >
                ${escapeHtml(card.actionLabel)}
              </a>
            </div>
          `
          : ""
      }
    </article>
  `;
}

function buildOperatorPulseFeedItems() {
  if (state.selectedRunDetail) {
    const selectedRunItems = buildSelectedRunPulseFeedItems();
    if (selectedRunItems.length) {
      return selectedRunItems;
    }
  }

  return buildGlobalPulseFeedItems();
}

function buildSelectedRunPulseFeedItems() {
  if (state.selectedRunEvents.length) {
    return state.selectedRunEvents
      .slice(0, 6)
      .map((event) => runEventPresentation(event, state.selectedRunDetail));
  }

  if (!state.selectedRunDetail) {
    return [];
  }

  const taskCounts = normalizedTaskCounts(state.selectedRunDetail.task_counts);
  const guide = buildRunGuide(state.selectedRunDetail, []);
  return [
    {
      id: state.selectedRunDetail.run_id,
      tone: normalizePulseTone(statusTone(state.selectedRunDetail.status)),
      badge: displayRunStatus(state.selectedRunDetail.status),
      kicker: "Selected run",
      title: state.selectedRunDetail.title,
      summary: `${taskCounts.total} task(s) · ${state.selectedRunDetail.artifact_count ?? 0} artifact(s) · ${guide.currentStageTitle}`,
      detail: `${formatTimestamp(state.selectedRunDetail.created_at)} · ${shortId(state.selectedRunDetail.run_id)}`,
      sortTime: sortableTimestamp(state.selectedRunDetail.created_at),
    },
  ];
}

function runEventPresentation(event, runDetail) {
  const eventType = String(event.event_type ?? "event");
  const task = taskForRunEvent(runDetail, event);
  const taskLabel =
    task?.title ??
    task?.backlog_item_id ??
    (event.task_id ? `Task ${shortId(event.task_id)}` : "Run-level event");

  return {
    id: event.event_id,
    tone: normalizePulseTone(statusTone(event.status ?? event.scope)),
    badge: displayRunEventBadge(event),
    kicker: runEventKicker(eventType, event),
    title: displayRunEventType(eventType),
    summary: event.summary || runEventFallbackSummary(eventType),
    detail: [
      taskLabel,
      formatTimestamp(event.created_at),
      `raw: ${eventType}`,
      shortId(event.task_id ?? event.event_id),
    ].join(" · "),
    sortTime: sortableTimestamp(event.created_at),
  };
}

function displayRunEventType(eventType) {
  switch (eventType) {
    case RUN_SUBMITTED_EVENT_TYPE:
      return "Brief accepted";
    case RUN_STATUS_CHANGED_EVENT_TYPE:
      return "Run status changed";
    case RUN_POLICY_EVALUATED_EVENT_TYPE:
      return "Policy evaluated";
    case RUN_QUALITY_EVALUATED_EVENT_TYPE:
      return "Quality gate evaluated";
    case PR_CANDIDATE_EXPORTED_EVENT_TYPE:
      return "PR candidate exported";
    case PR_EXPORT_PUBLISHED_EVENT_TYPE:
      return "PR export published";
    case GITHUB_PR_OPENED_EVENT_TYPE:
      return "Draft PR opened";
    case TASK_STARTED_EVENT_TYPE:
      return "Task started";
    case TASK_WORKSPACE_PREPARED_EVENT_TYPE:
      return "Workspace prepared";
    case TASK_HEARTBEAT_EVENT_TYPE:
      return "Agent heartbeat";
    case TASK_SUCCEEDED_EVENT_TYPE:
      return "Task succeeded";
    case TASK_FAILED_EVENT_TYPE:
      return "Task failed";
    case TASK_REQUEUED_EVENT_TYPE:
      return "Task requeued";
    default:
      return humanizeIdentifier(eventType);
  }
}

function runEventKicker(eventType, event) {
  switch (eventType) {
    case RUN_POLICY_EVALUATED_EVENT_TYPE:
    case RUN_QUALITY_EVALUATED_EVENT_TYPE:
      return "Quality checkpoint";
    case PR_CANDIDATE_EXPORTED_EVENT_TYPE:
    case PR_EXPORT_PUBLISHED_EVENT_TYPE:
    case GITHUB_PR_OPENED_EVENT_TYPE:
      return "Promotion checkpoint";
    case TASK_STARTED_EVENT_TYPE:
    case TASK_WORKSPACE_PREPARED_EVENT_TYPE:
    case TASK_HEARTBEAT_EVENT_TYPE:
    case TASK_SUCCEEDED_EVENT_TYPE:
    case TASK_FAILED_EVENT_TYPE:
    case TASK_REQUEUED_EVENT_TYPE:
      return "Agent/task checkpoint";
    default:
      return event?.scope === "task" || event?.task_id
        ? "Agent/task checkpoint"
        : "Control-plane checkpoint";
  }
}

function displayRunEventBadge(event) {
  return humanizeIdentifier(event.status ?? event.scope ?? "event");
}

function runEventFallbackSummary(eventType) {
  return `The control plane recorded ${humanizeIdentifier(eventType).toLowerCase()} for this run.`;
}

function taskForRunEvent(runDetail, event) {
  if (!event?.task_id) {
    return null;
  }

  return (runDetail?.tasks || []).find((task) => task.task_id === event.task_id) ?? null;
}

function buildGlobalPulseFeedItems() {
  const items = [];

  state.latestRuns.slice(0, 4).forEach((run) => {
    const taskCounts = normalizedTaskCounts(run.task_counts);
    items.push({
      id: run.run_id,
      tone: normalizePulseTone(statusTone(run.status)),
      badge: displayRunStatus(run.status),
      kicker: "Run",
      title: run.title,
      summary: `${taskCounts.running} running · ${taskCounts.queued} queued · ${taskCounts.failed} failed · ${run.target_pack ?? "no pack"}`,
      detail: `${formatTimestamp(run.created_at)} · ${shortId(run.run_id)}`,
      sortTime: sortableTimestamp(run.created_at),
    });
  });

  state.latestWebhookActions.slice(0, 2).forEach((item) => {
    items.push({
      id: item.request_id,
      tone: normalizePulseTone(statusTone(item.status)),
      badge: item.status ?? "unknown",
      kicker: "Webhook action",
      title: item.action,
      summary: item.repository_full_name ?? item.delivery_id,
      detail: `${formatTimestamp(item.updated_at ?? item.created_at)} · ${shortId(item.request_id)}`,
      sortTime: sortableTimestamp(item.updated_at ?? item.created_at),
    });
  });

  state.latestRepositorySignals.slice(0, 2).forEach((item) => {
    items.push({
      id: item.signal_id,
      tone: normalizePulseTone(statusTone(item.status)),
      badge: item.status ?? "unknown",
      kicker: "Repository signal",
      title: item.signal_kind,
      summary: item.repository_full_name ?? item.signal_id,
      detail: `${formatTimestamp(item.updated_at ?? item.created_at)} · ${shortId(item.signal_id)}`,
      sortTime: sortableTimestamp(item.updated_at ?? item.created_at),
    });
  });

  state.latestWebhookDeliveries.slice(0, 2).forEach((item) => {
    items.push({
      id: item.delivery_id,
      tone: normalizePulseTone(statusTone(item.routing_status)),
      badge: item.routing_status ?? "unknown",
      kicker: "Ingress delivery",
      title: item.event,
      summary: item.repository_full_name ?? item.delivery_id,
      detail: `${formatTimestamp(item.updated_at ?? item.created_at)} · ${shortId(item.delivery_id)}`,
      sortTime: sortableTimestamp(item.updated_at ?? item.created_at),
    });
  });

  return items
    .sort((left, right) => right.sortTime - left.sortTime)
    .slice(0, 8);
}

function renderPulseFeedItem(item) {
  return `
    <article class="pulse-feed-item">
      <div class="pulse-feed-item-head">
        <p class="panel-kicker">${escapeHtml(item.kicker)}</p>
        <span class="badge badge-${escapeHtml(item.tone)}">${escapeHtml(item.badge)}</span>
      </div>
      <h3>${escapeHtml(item.title)}</h3>
      <p>${escapeHtml(item.summary)}</p>
      <div class="pulse-feed-meta">
        <span>${escapeHtml(item.detail)}</span>
      </div>
    </article>
  `;
}

function loadedRunStatusCounts(runs) {
  return runs.reduce(
    (counts, run) => {
      counts.total += 1;
      switch (run.status) {
        case "queued":
          counts.queued += 1;
          break;
        case "executing":
          counts.executing += 1;
          break;
        case "succeeded":
          counts.succeeded += 1;
          break;
        case "failed":
          counts.failed += 1;
          break;
        default:
          break;
      }
      return counts;
    },
    {
      total: 0,
      queued: 0,
      executing: 0,
      succeeded: 0,
      failed: 0,
    }
  );
}

function normalizePulseTone(value) {
  switch (value) {
    case "success":
    case "warning":
    case "error":
      return value;
    default:
      return "neutral";
  }
}

function sortableTimestamp(value) {
  if (!value) {
    return 0;
  }

  const parsed = Date.parse(value);
  return Number.isFinite(parsed) ? parsed : 0;
}

function restoreMissionTabPreference() {
  const saved = window.localStorage.getItem(MISSION_TAB_STORAGE_KEY);
  return normalizeMissionTab(saved);
}

function persistMissionTabPreference() {
  window.localStorage.setItem(MISSION_TAB_STORAGE_KEY, state.activeMissionTab);
}

function normalizeMissionTab(value) {
  switch (value) {
    case "agents":
    case "grafana":
    case "litellm":
      return value;
    default:
      return "flow";
  }
}

function setMissionSurfaceEmbed(surface, enabled = true) {
  const trimmedSurface = String(surface ?? "").trim();
  if (!MISSION_SURFACE_EMBED_KEYS.includes(trimmedSurface)) {
    return;
  }

  if (state.missionSurfaceEmbeds[trimmedSurface] === enabled) {
    return;
  }

  state.missionSurfaceEmbeds[trimmedSurface] = enabled;

  if (state.activeMissionTab === trimmedSurface) {
    renderActiveMissionPanel();
  }
}

function setActiveMissionTab(tab) {
  const nextTab = normalizeMissionTab(tab);
  if (state.activeMissionTab === nextTab) {
    return;
  }

  state.activeMissionTab = nextTab;
  persistMissionTabPreference();
  syncMissionTabSelection();
  renderMissionControl();
}

function syncMissionTabSelection() {
  if (!elements.missionShell || !elements.missionTabBar) {
    return;
  }

  const activeTab = normalizeMissionTab(state.activeMissionTab);
  state.activeMissionTab = activeTab;
  elements.missionShell.dataset.activeTab = activeTab;

  const panelIds = {
    flow: "missionFlowPanel",
    agents: "missionAgentsPanel",
    grafana: "missionGrafanaPanel",
    litellm: "missionLitellmPanel",
  };

  elements.missionTabBar
    .querySelectorAll("[data-mission-tab]")
    .forEach((button) => {
      const tab = normalizeMissionTab(button.dataset.missionTab);
      const isActive = tab === activeTab;
      button.classList.toggle("is-active", isActive);
      button.setAttribute("aria-selected", isActive ? "true" : "false");
      button.setAttribute("tabindex", isActive ? "0" : "-1");
    });

  Object.entries(panelIds).forEach(([tab, panelId]) => {
    const panel = elements[panelId];
    if (!panel) {
      return;
    }

    const isActive = tab === activeTab;
    panel.classList.toggle("hidden", !isActive);
    panel.setAttribute("aria-hidden", isActive ? "false" : "true");
  });
}

function renderMissionControl() {
  syncMissionTabSelection();
  renderMissionTabBadges();
  renderMissionTabHint();
  renderActiveMissionPanel();
}

function renderMissionTabBadges() {
  renderMissionFlowTabBadge();
  renderMissionAgentsTabBadge();
  renderMissionGrafanaTabBadge();
  renderMissionLitellmTabBadge();
}

function renderMissionFlowTabBadge() {
  if (!state.selectedRunDetail) {
    setMissionTabBadge(elements.missionTabFlowBadge, "neutral", "No run");
    return;
  }

  const runStatus = state.selectedRunDetail.status ?? "unknown";
  setMissionTabBadge(
    elements.missionTabFlowBadge,
    statusTone(runStatus),
    displayRunStatus(runStatus)
  );
}

function renderMissionAgentsTabBadge() {
  if (!state.selectedRunDetail) {
    setMissionTabBadge(elements.missionTabAgentsBadge, "neutral", "Open run");
    return;
  }

  const agents = runAgents(state.selectedRunDetail);
  if (!agents.length) {
    setMissionTabBadge(elements.missionTabAgentsBadge, "neutral", "No agents");
    return;
  }

  const externalTaskCount = (state.selectedRunDetail.tasks ?? []).filter(
    (task) => task.agent_execution?.mode === "external_agent"
  ).length;
  setMissionTabBadge(
    elements.missionTabAgentsBadge,
    externalTaskCount > 0 ? "success" : "warning",
    `${agents.length} agent${agents.length === 1 ? "" : "s"}`
  );
}

function renderMissionGrafanaTabBadge() {
  const grafanaSurface = missionSurfaceStatus("grafana");
  setMissionTabBadge(
    elements.missionTabGrafanaBadge,
    missionSurfaceTone(grafanaSurface),
    missionSurfaceBadge(grafanaSurface, "Ready")
  );
}

function renderMissionLitellmTabBadge() {
  const aiGateway = state.dashboardSnapshot.aiGateway?.data ?? {};
  const litellmUiSurface = missionSurfaceStatus("litellm_ui");

  if (missionSurfaceReady(litellmUiSurface)) {
    setMissionTabBadge(elements.missionTabLitellmBadge, "success", "UI ready");
    return;
  }

  if (aiGateway.ready === true) {
    setMissionTabBadge(elements.missionTabLitellmBadge, "warning", "Gateway");
    return;
  }

  setMissionTabBadge(
    elements.missionTabLitellmBadge,
    statusTone(aiGateway.status),
    aiGateway.status ?? "Gateway"
  );
}

function setMissionTabBadge(target, tone, text) {
  if (!target) {
    return;
  }

  setBadge(target, normalizePulseTone(tone), text);
  const tabButton = target.closest("[data-mission-tab]");
  if (tabButton) {
    const label = tabButton.querySelector(".mission-tab-label")?.textContent?.trim() ?? "Tab";
    tabButton.setAttribute("aria-label", `${label}: ${text}`);
    tabButton.title = `${label}: ${text}`;
  }
}

function renderActiveMissionPanel() {
  switch (state.activeMissionTab) {
    case "agents":
      renderMissionAgentsPanel();
      return;
    case "grafana":
      renderMissionGrafanaPanel();
      return;
    case "litellm":
      renderMissionLitellmPanel();
      return;
    default:
      renderMissionFlowPanel();
  }
}

function renderMissionTabHint() {
  let message;
  switch (state.activeMissionTab) {
    case "agents":
      message = state.selectedRunDetail
        ? "Filter by assigned agent to compare live task state, task events, and persisted agent reports for the selected run."
        : "Open a run first, then compare multiple agents side by side from the same selected-run context.";
      break;
    case "grafana":
      message =
        "The embedded Grafana view is stack-wide. Use it for metrics, logs, traces, and the provisioned Catalyst Continuum overview dashboard.";
      break;
    case "litellm":
      message =
        "The LiteLLM tab keeps gateway status, native UI entrypoints, and model visibility close to the operator workflow.";
      break;
    default:
      message = state.selectedRunDetail
        ? "This view compresses the selected run lifecycle, latest artifacts, and recent movement into one flow-oriented operator read."
        : "Open a run to watch the brief-to-plan-to-execution-to-quality-to-draft-PR lifecycle from one surface.";
      break;
  }

  setTextContent(elements.missionTabHint, message, { markUpdated: false });
}

function renderMissionFlowPanel() {
  if (!state.selectedRunDetail) {
    setRenderedHtml(
      elements.missionFlowPanel,
      renderSectionEmptyState(
        "Mission flow",
        "Open a run to unlock the full delivery map",
        "The flow tab becomes valuable once one concrete run exists, because it ties stages, artifacts, and the latest run movement together."
      ),
      { markUpdated: false }
    );
    return;
  }

  const runDetail = state.selectedRunDetail;
  const guide = buildRunGuide(runDetail, state.selectedRunEvents);
  const feedItems = buildSelectedRunPulseFeedItems();
  const highlightArtifacts = Array.isArray(runDetail.artifact_highlights)
    ? runDetail.artifact_highlights
    : [];
  const publicationGuard = repositoryTargetGuardSummary(runDetail);

  setRenderedHtml(
    elements.missionFlowPanel,
    `
      <div class="mission-flow-layout">
        ${renderMissionContextRibbon(runDetail, guide, publicationGuard)}
        <div class="mission-stage-strip">
          ${guide.stages.map((stage, index) => renderMissionStageCard(stage, index)).join("")}
        </div>
        <div class="mission-mini-grid">
          ${renderMissionMiniCard(
            "Selected run",
            runDetail.title,
            `${shortId(runDetail.run_id)} · ${displayRunStatus(runDetail.status)}`,
            "warning"
          )}
          ${renderMissionMiniCard(
            "Recommended step",
            guide.nextActionTitle,
            guide.nextActionDetail,
            guide.badgeTone
          )}
          ${renderMissionMiniCard(
            "Publication guard",
            publicationGuard.title,
            publicationGuard.detail,
            publicationGuard.tone
          )}
          ${renderMissionMiniCard(
            "Current estate",
            `${runDetail.task_counts?.total ?? 0} task(s) · ${runDetail.artifact_count ?? 0} artifact(s)`,
            `${runDetail.task_counts?.running ?? 0} running · ${runDetail.task_counts?.queued ?? 0} queued · ${runDetail.task_counts?.failed ?? 0} failed`,
            statusTone(runDetail.status)
          )}
        </div>
        ${renderMissionJourneyTimeline(runDetail, guide)}
        ${renderMissionActionStrip(runDetail, guide)}
        ${renderMissionControlReadinessBoard(runDetail, guide)}
        ${renderReviewHandoffChecklist(runDetail)}
        ${renderMissionEvidenceFreshnessBoard(runDetail)}
        ${renderMissionEvidenceMap(runDetail)}
        <section class="mission-feed-shell">
          <div class="detail-section-head">
            <div>
              <p class="panel-kicker">Recent movement</p>
              <h3>Selected-run flow feed</h3>
            </div>
            <span class="badge badge-${escapeHtml(statusTone(runDetail.status))}">${escapeHtml(
              displayRunStatus(runDetail.status)
            )}</span>
          </div>
          <div class="mission-feed-list">
            ${
              feedItems.length
                ? feedItems.map(renderMissionFeedItem).join("")
                : renderSectionEmptyState(
                    "Flow feed",
                    "No run movement recorded yet",
                    "Once task, quality, or promotion events are persisted, they show up here."
                  )
            }
          </div>
        </section>
        <section class="mission-feed-shell">
          <div class="detail-section-head">
            <div>
              <p class="panel-kicker">Promotion evidence</p>
              <h3>Latest highlighted artifacts</h3>
            </div>
            <span class="badge badge-neutral">${escapeHtml(String(highlightArtifacts.length))}</span>
          </div>
          <div class="mission-feed-list">
            ${
              highlightArtifacts.length
                ? highlightArtifacts.map(renderMissionArtifactItem).join("")
                : renderSectionEmptyState(
                    "Highlighted artifacts",
                    "No highlight artifacts are available yet",
                    "Backlog, quality, PR candidate, and PR publication artifacts appear here as the run advances."
                  )
            }
          </div>
        </section>
      </div>
    `,
    { markUpdated: false }
  );
}

function renderMissionContextRibbon(runDetail, guide, publicationGuard) {
  const repository = repositoryLabel(runDetail) || "No repository target";
  const currentStage = guide.currentStageTitle || "Current stage unknown";
  const recommendedAction = guide.nextActionTitle || "No recommended action";

  return `
    <section class="mission-context-ribbon" aria-label="Selected run context">
      ${renderMissionContextCard(
        "Selected run",
        runDetail.title,
        `${shortId(runDetail.run_id)} · ${displayRunStatus(runDetail.status)} · ${formatTimestamp(runDetail.created_at)}`,
        "warning"
      )}
      ${renderMissionContextCard(
        "Repository target",
        repository,
        `${runDetail.repository?.default_branch ?? "default branch unknown"} · ${publicationGuard.title}`,
        publicationGuard.tone
      )}
      ${renderMissionContextCard(
        "Pack and routing",
        runDetail.target_pack ?? "unassigned",
        `${runDetail.trigger} trigger · ${runAgents(runDetail).length} agent lane(s)`,
        "neutral"
      )}
      ${renderMissionContextCard(
        currentStage,
        recommendedAction,
        "The orchestrator prepares evidence; GitHub remains the human approval boundary.",
        guide.badgeTone
      )}
    </section>
  `;
}

function renderMissionContextCard(kicker, title, detail, tone) {
  const normalizedTone = normalizePulseTone(tone);

  return `
    <article class="mission-context-card" data-mission-context-card="true">
      <div class="mission-feed-head">
        <p class="panel-kicker">${escapeHtml(kicker)}</p>
        <span class="badge badge-${escapeHtml(normalizedTone)}">${escapeHtml(
          toneLabel(normalizedTone)
        )}</span>
      </div>
      <h3>${escapeHtml(title)}</h3>
      <p>${escapeHtml(detail)}</p>
    </article>
  `;
}

function renderMissionJourneyTimeline(runDetail, guide) {
  const items = buildMissionJourneyItems(runDetail, guide);
  const activeItem = items.find((item) => item.state === "blocked" || item.state === "active");

  return `
    <section class="mission-journey-shell">
      <div class="detail-section-head">
        <div>
          <p class="panel-kicker">Run journey</p>
          <h3>How this run reached the current stage</h3>
        </div>
        <span class="badge badge-${escapeHtml(activeItem ? guideTone(activeItem.state) : "success")}">
          ${escapeHtml(activeItem ? activeItem.badge : "Complete")}
        </span>
      </div>
      <div class="mission-journey-rail">
        ${items.map(renderMissionJourneyItem).join("")}
      </div>
    </section>
  `;
}

function buildMissionJourneyItems(runDetail, guide) {
  const artifacts = runArtifacts(runDetail);
  const events = Array.isArray(state.selectedRunEvents) ? state.selectedRunEvents : [];
  const artifactTypes = runArtifactTypes(runDetail);
  const eventTypes = new Set(events.map((event) => event.event_type));
  const taskCounts = normalizedTaskCounts(runDetail.task_counts);
  const planningArtifactTypes = [
    BACKLOG_ARTIFACT_TYPE,
    POLICY_REPORT_ARTIFACT_TYPE,
    DISPATCH_PLAN_ARTIFACT_TYPE,
  ];
  const prEvidenceTypes = [
    PR_CANDIDATE_ARTIFACT_TYPE,
    PR_EXPORT_ARTIFACT_TYPE,
    PR_PUBLICATION_ARTIFACT_TYPE,
    GITHUB_PULL_REQUEST_ARTIFACT_TYPE,
  ];
  const planningArtifactCount = planningArtifactTypes.filter((type) =>
    artifactTypes.has(type)
  ).length;
  const prEvidenceCount = prEvidenceTypes.filter((type) => artifactTypes.has(type)).length;
  const qualityReady =
    artifactTypes.has(QUALITY_REPORT_ARTIFACT_TYPE) ||
    eventTypes.has(RUN_QUALITY_EVALUATED_EVENT_TYPE);
  const githubPrReady =
    artifactTypes.has(GITHUB_PULL_REQUEST_ARTIFACT_TYPE) ||
    eventTypes.has(GITHUB_PR_OPENED_EVENT_TYPE);
  const stages = Array.isArray(guide.stages) ? guide.stages : [];

  return [
    missionJourneyItem({
      stage: stages[0],
      fallbackTitle: "Brief intake",
      timestamp:
        firstRunEventTimestamp([RUN_SUBMITTED_EVENT_TYPE], events) ?? runDetail.created_at,
      evidence: `${runDetail.trigger} · run ${shortId(runDetail.run_id)}`,
      detail: "The operator request was accepted and materialized as durable run state.",
    }),
    missionJourneyItem({
      stage: stages[1],
      fallbackTitle: "Plan and routing",
      timestamp:
        latestRunEventTimestamp([RUN_POLICY_EVALUATED_EVENT_TYPE], events) ??
        latestArtifactTimestamp(planningArtifactTypes, artifacts),
      evidence: `${planningArtifactCount}/${planningArtifactTypes.length} planning artifacts · ${taskCounts.total} task(s)`,
      detail:
        planningArtifactCount === planningArtifactTypes.length
          ? "Backlog, policy, and dispatch routing are inspectable."
          : "Planning evidence is still incomplete or has not been highlighted yet.",
    }),
    missionJourneyItem({
      stage: stages[2],
      fallbackTitle: "Task execution",
      timestamp:
        latestRunEventTimestamp(
          [
            TASK_STARTED_EVENT_TYPE,
            TASK_WORKSPACE_PREPARED_EVENT_TYPE,
            TASK_HEARTBEAT_EVENT_TYPE,
            TASK_SUCCEEDED_EVENT_TYPE,
            TASK_FAILED_EVENT_TYPE,
            TASK_REQUEUED_EVENT_TYPE,
          ],
          events
        ) ?? latestTaskTimestamp(runDetail),
      evidence: `${taskCounts.succeeded} succeeded · ${taskCounts.running} running · ${taskCounts.queued} queued · ${taskCounts.failed} failed`,
      detail: "Execution status comes from persisted task state and agent/task events.",
    }),
    missionJourneyItem({
      stage: stages[3],
      fallbackTitle: "Quality gate",
      timestamp:
        latestRunEventTimestamp([RUN_QUALITY_EVALUATED_EVENT_TYPE], events) ??
        latestArtifactTimestamp([QUALITY_REPORT_ARTIFACT_TYPE], artifacts),
      evidence: qualityReady ? "Quality evidence recorded" : "No quality evidence yet",
      detail: qualityReady
        ? "Quality can be re-evaluated after artifact-changing work."
        : "Promotion should stay blocked until a quality report is recorded.",
    }),
    missionJourneyItem({
      stage: stages[4],
      fallbackTitle: "GitHub handoff",
      timestamp:
        latestRunEventTimestamp(
          [
            PR_CANDIDATE_EXPORTED_EVENT_TYPE,
            PR_EXPORT_PUBLISHED_EVENT_TYPE,
            GITHUB_PR_OPENED_EVENT_TYPE,
          ],
          events
        ) ?? latestArtifactTimestamp(prEvidenceTypes, artifacts),
      evidence: githubPrReady
        ? "Draft PR recorded"
        : `${prEvidenceCount}/${prEvidenceTypes.length} promotion artifacts`,
      detail: githubPrReady
        ? "The remaining decision belongs to GitHub review."
        : "The orchestrator is still preparing the review handoff evidence.",
    }),
  ];
}

function missionJourneyItem({ detail, evidence, fallbackTitle, stage, timestamp }) {
  const state = stage?.state ?? "pending";
  const tone = guideTone(state);

  return {
    badge: guideBadgeLabel(state),
    detail,
    evidence,
    state,
    timestamp: timestamp ? formatTimestamp(timestamp) : "not recorded yet",
    title: stage?.title ?? fallbackTitle,
    tone,
  };
}

function renderMissionJourneyItem(item) {
  return `
    <article class="mission-journey-item mission-journey-item-${escapeHtml(item.state)}">
      <div class="mission-journey-marker">
        <span></span>
      </div>
      <div class="mission-journey-card">
        <div class="mission-feed-head">
          <div>
            <p class="panel-kicker">${escapeHtml(item.timestamp)}</p>
            <h4>${escapeHtml(item.title)}</h4>
          </div>
          <span class="badge badge-${escapeHtml(item.tone)}">${escapeHtml(item.badge)}</span>
        </div>
        <p>${escapeHtml(item.detail)}</p>
        <div class="mission-feed-meta">
          <span>${escapeHtml(item.evidence)}</span>
        </div>
      </div>
    </article>
  `;
}

function renderMissionActionStrip(runDetail, guide) {
  const actionId = guide.nextActionControlId;
  const availability = runActionAvailability(runDetail);
  const actionState = actionId
    ? availability[actionId] ?? disabledRunAction("Recommended action is unavailable.")
    : disabledRunAction("No direct control-plane action is recommended for this stage.");
  const actionLabel = actionId ? displayRunActionLabel(actionId) : "No direct action";
  const disabledAttr = actionState.enabled ? "" : " disabled";
  const titleAttr = actionState.reason ? ` title="${escapeHtml(actionState.reason)}"` : "";

  return `
    <section class="mission-action-strip">
      <article class="mission-action-card mission-action-primary">
        <div>
          <p class="panel-kicker">Recommended control</p>
          <h3>${escapeHtml(guide.nextActionTitle)}</h3>
          <p>${escapeHtml(guide.nextActionDetail)}</p>
          ${
            actionState.reason
              ? `<p class="microcopy mission-action-reason">${escapeHtml(actionState.reason)}</p>`
              : ""
          }
        </div>
        <div class="mission-action-buttons">
          <button
            class="button button-primary"
            type="button"
            data-run-action="${escapeHtml(actionId ?? "")}"
            ${disabledAttr}${titleAttr}
          >
            ${escapeHtml(actionLabel)}
          </button>
          <a class="button button-ghost button-link" href="#run-detail">Open selected-run guide</a>
        </div>
      </article>
      <article class="mission-action-card">
        <p class="panel-kicker">Why this matters</p>
        <h3>One safe step at a time</h3>
        <p>
          Mission Control mirrors the same guarded run controls below. Actions stay disabled until
          the selected run has the required task, artifact, quality, or promotion prerequisites.
        </p>
      </article>
    </section>
  `;
}

function renderMissionControlReadinessBoard(runDetail, guide) {
  const items = buildMissionControlReadinessItems(runDetail, guide);
  const availableCount = items.filter((item) => item.enabled).length;

  return `
    <section class="mission-control-board">
      <div class="detail-section-head">
        <div>
          <p class="panel-kicker">Control readiness</p>
          <h3>Which orchestrator actions are safe right now</h3>
        </div>
        <span class="badge badge-${escapeHtml(availableCount ? "warning" : "neutral")}">
          ${escapeHtml(`${availableCount}/${items.length} available`)}
        </span>
      </div>
      <div class="mission-control-grid">
        ${items.map(renderMissionControlReadinessItem).join("")}
      </div>
    </section>
  `;
}

function buildMissionControlReadinessItems(runDetail, guide) {
  const availability = runActionAvailability(runDetail);
  const recommendedActionId = guide.nextActionControlId;
  return RUN_ACTION_SEQUENCE.map((actionId) => {
    const actionState = availability[actionId] ?? disabledRunAction("Action unavailable.");
    return {
      actionId,
      description: runActionDescription(actionId),
      enabled: actionState.enabled === true,
      label: displayRunActionLabel(actionId),
      recommended: actionId === recommendedActionId,
      reason: actionState.reason || "",
    };
  });
}

function renderMissionControlReadinessItem(item) {
  const busy = state.runActionInFlight && state.runActionBusyActionId === item.actionId;
  const tone = item.recommended ? "warning" : item.enabled ? "success" : "neutral";
  const status = busy ? "Running" : item.recommended ? "Recommended" : item.enabled ? "Available" : "Locked";
  const disabledAttr = state.runActionInFlight || !item.enabled ? " disabled" : "";
  const titleAttr = item.enabled ? "" : ` title="${escapeHtml(item.reason)}"`;

  return `
    <article class="mission-control-card mission-control-card-${escapeHtml(tone)}">
      <div class="mission-feed-head">
        <div>
          <p class="panel-kicker">${escapeHtml(item.label)}</p>
          <h4>${escapeHtml(status)}</h4>
        </div>
        <span class="badge badge-${escapeHtml(tone)}">${escapeHtml(status)}</span>
      </div>
      <p>${escapeHtml(item.description)}</p>
      <p class="microcopy">
        ${escapeHtml(item.enabled ? "Guard passed. This action can be executed from this run context." : item.reason)}
      </p>
      <div class="mission-control-actions">
        <button
          class="button ${escapeHtml(item.recommended ? "button-primary" : "button-ghost")}"
          type="button"
          data-run-action="${escapeHtml(item.actionId)}"
          ${disabledAttr}${titleAttr}
        >
          ${escapeHtml(busy ? runActionBusyLabel(item.actionId) : item.label)}
        </button>
      </div>
    </article>
  `;
}

function runActionDescription(actionId) {
  switch (actionId) {
    case "tasks-next":
      return "Claims and executes one queued task through the shared command layer.";
    case "worker-once":
      return "Lets the worker drain one available task end-to-end with runtime policy applied.";
    case "evaluate-policy":
      return "Re-checks the run against pack, runtime, retry, and execution policy.";
    case "evaluate-quality":
      return "Creates or refreshes the quality gate evidence before promotion.";
    case "export-pr":
      return "Turns the PR candidate into an explicit branch/export bundle.";
    case "publish-pr":
      return "Publishes the exported branch handoff when remote publication is configured.";
    case "draft-pr":
      return "Creates or reuses the GitHub draft PR while preserving human review.";
    default:
      return "Runs a guarded orchestrator action for the selected run.";
  }
}

function renderReviewHandoffChecklist(runDetail) {
  const items = buildReviewHandoffItems(runDetail);
  const readyCount = items.filter((item) => item.ready).length;

  return `
    <section class="review-handoff-shell">
      <div class="detail-section-head">
        <div>
          <p class="panel-kicker">Review handoff</p>
          <h3>What remains before GitHub review</h3>
        </div>
        <span class="badge badge-${escapeHtml(readyCount === items.length ? "success" : "warning")}">
          ${escapeHtml(`${readyCount}/${items.length} ready`)}
        </span>
      </div>
      <div class="review-handoff-grid">
        ${items.map(renderReviewHandoffItem).join("")}
      </div>
    </section>
  `;
}

function buildReviewHandoffItems(runDetail) {
  const artifactTypes = runArtifactTypes(runDetail);
  const eventTypes = new Set((state.selectedRunEvents ?? []).map((event) => event.event_type));
  const availability = runActionAvailability(runDetail);
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

  return [
    reviewHandoffItem({
      action: availability["evaluate-quality"],
      actionId: "evaluate-quality",
      detail: qualityReady
        ? "The run has explicit quality evidence. Re-run after artifact-changing work."
        : "Quality evidence is required before the handoff can be treated as promotable.",
      kicker: "01 Quality gate",
      ready: qualityReady,
      title: qualityReady ? "Quality evidence is ready" : "Quality evidence is missing",
    }),
    reviewHandoffItem({
      detail: prCandidateReady
        ? "The reviewable PR candidate artifact exists for this run."
        : "The PR candidate appears after successful execution has produced promotable output.",
      kicker: "02 PR candidate",
      ready: prCandidateReady,
      title: prCandidateReady ? "Candidate artifact exists" : "Candidate is not ready",
      waitingReason:
        runDetail.status === "succeeded"
          ? "Inspect execution artifacts if this stays missing."
          : `Run status is ${displayRunStatus(runDetail.status)}.`,
    }),
    reviewHandoffItem({
      action: availability["export-pr"],
      actionId: "export-pr",
      detail: prExportReady
        ? "The PR export bundle has been assembled for branch publication."
        : "Export turns the candidate into a concrete repository branch bundle.",
      kicker: "03 Export bundle",
      ready: prExportReady,
      title: prExportReady ? "Export bundle is ready" : "Export bundle is pending",
    }),
    reviewHandoffItem({
      action: availability["publish-pr"],
      actionId: "publish-pr",
      detail: publicationReady
        ? "Publication evidence exists for the branch handoff."
        : "Publication pushes or prepares the exported branch before GitHub review starts.",
      kicker: "04 Branch publication",
      ready: publicationReady,
      title: publicationReady ? "Branch handoff is published" : "Branch handoff is pending",
    }),
    reviewHandoffItem({
      action: availability["draft-pr"],
      actionId: "draft-pr",
      detail: githubPrReady
        ? "The draft PR is recorded. Human review continues in GitHub."
        : "The final step opens or reuses the draft PR without bypassing review controls.",
      kicker: "05 Draft PR",
      ready: githubPrReady,
      title: githubPrReady ? "GitHub review is ready" : "Draft PR is pending",
    }),
  ];
}

function reviewHandoffItem({ action, actionId, detail, kicker, ready, title, waitingReason }) {
  const actionLabel = actionId ? displayRunActionLabel(actionId) : "";
  const actionAvailable = action?.enabled === true;
  const tone = ready
    ? "success"
    : actionAvailable
      ? "warning"
      : waitingReason || action?.reason
        ? "neutral"
        : "warning";
  const status = ready ? "Ready" : actionAvailable ? "Next" : "Locked";
  const control = ready
    ? "No control needed unless evidence changes."
    : actionAvailable
      ? `${actionLabel} is available from the guarded run controls.`
      : waitingReason || action?.reason || "Waiting for earlier handoff evidence.";

  return {
    control,
    detail,
    kicker,
    ready,
    status,
    title,
    tone,
  };
}

function renderReviewHandoffItem(item) {
  const tone = normalizePulseTone(item.tone);

  return `
    <article class="review-handoff-card review-handoff-card-${escapeHtml(tone)}">
      <div class="mission-feed-head">
        <p class="panel-kicker">${escapeHtml(item.kicker)}</p>
        <span class="badge badge-${escapeHtml(tone)}">${escapeHtml(item.status)}</span>
      </div>
      <h4>${escapeHtml(item.title)}</h4>
      <p>${escapeHtml(item.detail)}</p>
      <p class="microcopy">${escapeHtml(item.control)}</p>
    </article>
  `;
}

function renderMissionEvidenceFreshnessBoard(runDetail) {
  const items = buildMissionEvidenceFreshnessItems(runDetail);
  const latestItem = items
    .filter((item) => item.timestamp)
    .sort((left, right) => sortableTimestamp(right.timestamp) - sortableTimestamp(left.timestamp))[0];
  const latestLabel = latestItem
    ? `${latestItem.kicker} updated ${formatTimestamp(latestItem.timestamp)}`
    : "No timestamped evidence yet";

  return `
    <section class="mission-freshness-board" aria-label="Evidence freshness">
      <div class="detail-section-head">
        <div>
          <p class="panel-kicker">Evidence freshness</p>
          <h3>What changed last, and what must not be stale</h3>
        </div>
        <span
          class="badge badge-${escapeHtml(latestItem ? latestItem.tone : "neutral")}"
          data-mission-freshness-latest="true"
        >
          ${escapeHtml(latestLabel)}
        </span>
      </div>
      <div class="mission-freshness-grid">
        ${items.map(renderMissionEvidenceFreshnessItem).join("")}
      </div>
    </section>
  `;
}

function buildMissionEvidenceFreshnessItems(runDetail) {
  const artifacts = runArtifacts(runDetail);
  const artifactTypes = runArtifactTypes(runDetail);
  const events = Array.isArray(state.selectedRunEvents) ? state.selectedRunEvents : [];
  const eventTypes = new Set(events.map((event) => event.event_type));
  const taskCounts = normalizedTaskCounts(runDetail.task_counts);
  const planningArtifactTypes = [
    BACKLOG_ARTIFACT_TYPE,
    POLICY_REPORT_ARTIFACT_TYPE,
    DISPATCH_PLAN_ARTIFACT_TYPE,
  ];
  const taskEventTypes = [
    TASK_STARTED_EVENT_TYPE,
    TASK_WORKSPACE_PREPARED_EVENT_TYPE,
    TASK_HEARTBEAT_EVENT_TYPE,
    TASK_SUCCEEDED_EVENT_TYPE,
    TASK_FAILED_EVENT_TYPE,
    TASK_REQUEUED_EVENT_TYPE,
  ];
  const promotionEventTypes = [
    PR_CANDIDATE_EXPORTED_EVENT_TYPE,
    PR_EXPORT_PUBLISHED_EVENT_TYPE,
    GITHUB_PR_OPENED_EVENT_TYPE,
  ];
  const promotionArtifactTypes = [
    PR_CANDIDATE_ARTIFACT_TYPE,
    PR_EXPORT_ARTIFACT_TYPE,
    PR_PUBLICATION_ARTIFACT_TYPE,
    GITHUB_PULL_REQUEST_ARTIFACT_TYPE,
  ];
  const planningArtifactCount = planningArtifactTypes.filter((artifactType) =>
    artifactTypes.has(artifactType)
  ).length;
  const promotionArtifactCount = promotionArtifactTypes.filter((artifactType) =>
    artifactTypes.has(artifactType)
  ).length;
  const latestPlanningAt =
    latestArtifactTimestamp(planningArtifactTypes, artifacts) ??
    firstRunEventTimestamp([RUN_SUBMITTED_EVENT_TYPE], events) ??
    runDetail.created_at;
  const latestExecutionAt = latestTimestamp([
    latestRunEventTimestamp(taskEventTypes, events),
    latestTaskTimestamp(runDetail),
    latestArtifactTimestamp(["agent_task_report", "log", "task_workspace_input"], artifacts),
  ]);
  const latestQualityAt = latestTimestamp([
    latestRunEventTimestamp([RUN_QUALITY_EVALUATED_EVENT_TYPE], events),
    latestArtifactTimestamp([QUALITY_REPORT_ARTIFACT_TYPE], artifacts),
  ]);
  const latestPromotionAt = latestTimestamp([
    latestRunEventTimestamp(promotionEventTypes, events),
    latestArtifactTimestamp(promotionArtifactTypes, artifacts),
  ]);
  const planningReady = planningArtifactCount === planningArtifactTypes.length;
  const executionFailed = runDetail.status === "failed" || taskCounts.failed > 0;
  const executionComplete =
    runDetail.status === "succeeded" &&
    taskCounts.total > 0 &&
    taskCounts.queued === 0 &&
    taskCounts.running === 0 &&
    taskCounts.failed === 0;
  const qualityReady =
    artifactTypes.has(QUALITY_REPORT_ARTIFACT_TYPE) ||
    eventTypes.has(RUN_QUALITY_EVALUATED_EVENT_TYPE);
  const qualityFollowsExecution = qualityReady && timestampAtLeast(latestQualityAt, latestExecutionAt);
  const githubPrReady =
    artifactTypes.has(GITHUB_PULL_REQUEST_ARTIFACT_TYPE) ||
    eventTypes.has(GITHUB_PR_OPENED_EVENT_TYPE);
  const promotionFollowsQuality =
    promotionArtifactCount > 0 && timestampAtLeast(latestPromotionAt, latestQualityAt);

  return [
    {
      checkpoint: `${planningArtifactCount}/${planningArtifactTypes.length} planning artifacts`,
      detail:
        "Backlog, policy, and dispatch artifacts should exist before agent execution is treated as explainable.",
      kicker: "Planning",
      timestamp: latestPlanningAt,
      title: planningReady ? "Planning evidence is ready" : "Planning evidence is incomplete",
      tone: planningReady ? "success" : "warning",
    },
    {
      checkpoint: `${taskCounts.succeeded}/${taskCounts.total} task(s) succeeded · ${taskCounts.running} running`,
      detail:
        "Task events, agent reports, and runtime logs are the freshest read on what the agents actually did.",
      kicker: "Execution",
      timestamp: latestExecutionAt,
      title: executionFailed
        ? "Execution evidence needs inspection"
        : executionComplete
          ? "Execution evidence is complete"
          : "Execution evidence is still moving",
      tone: executionFailed ? "error" : executionComplete ? "success" : taskCounts.total ? "warning" : "neutral",
    },
    {
      checkpoint: qualityReady
        ? qualityFollowsExecution
          ? "Quality follows latest execution movement"
          : "Quality may predate latest execution movement"
        : "No quality report or quality event",
      detail:
        "Promotion should use a quality report created after the latest execution-changing event or artifact.",
      kicker: "Quality",
      timestamp: latestQualityAt,
      title: qualityReady
        ? qualityFollowsExecution
          ? "Quality evidence is fresh"
          : "Quality evidence may be stale"
        : "Quality evidence is missing",
      tone: qualityReady
        ? qualityFollowsExecution
          ? "success"
          : "warning"
        : executionComplete
          ? "warning"
          : "neutral",
    },
    {
      checkpoint: `${promotionArtifactCount}/${promotionArtifactTypes.length} handoff artifacts`,
      detail:
        "The orchestrator prepares the branch and draft PR record; GitHub review remains the human decision point.",
      kicker: "Handoff",
      timestamp: latestPromotionAt,
      title: githubPrReady
        ? "Draft PR evidence is recorded"
        : promotionFollowsQuality
          ? "Handoff evidence follows quality"
          : promotionArtifactCount > 0
            ? "Handoff evidence is accumulating"
            : "Handoff evidence has not started",
      tone: githubPrReady ? "success" : promotionArtifactCount > 0 ? "warning" : "neutral",
    },
  ];
}

function renderMissionEvidenceFreshnessItem(item) {
  const tone = normalizePulseTone(item.tone);

  return `
    <article
      class="mission-freshness-card mission-freshness-card-${escapeHtml(tone)}"
      data-mission-freshness-card="true"
    >
      <div class="mission-feed-head">
        <div>
          <p class="panel-kicker">${escapeHtml(item.kicker)}</p>
          <h4>${escapeHtml(item.title)}</h4>
        </div>
        <span class="badge badge-${escapeHtml(tone)}">${escapeHtml(toneLabel(tone))}</span>
      </div>
      <p>${escapeHtml(item.detail)}</p>
      <div class="mission-feed-meta">
        <span>${escapeHtml(item.timestamp ? formatTimestamp(item.timestamp) : "not recorded yet")}</span>
        <span>${escapeHtml(item.checkpoint)}</span>
      </div>
    </article>
  `;
}

function renderMissionEvidenceMap(runDetail) {
  const evidenceItems = buildMissionEvidenceItems(runDetail);

  return `
    <section class="mission-evidence-map">
      <div class="detail-section-head">
        <div>
          <p class="panel-kicker">Evidence map</p>
          <h3>What the orchestrator has proven</h3>
        </div>
        <span class="badge badge-neutral">${escapeHtml(String(evidenceItems.length))} checkpoints</span>
      </div>
      <div class="mission-evidence-grid">
        ${evidenceItems.map(renderMissionEvidenceItem).join("")}
      </div>
    </section>
  `;
}

function buildMissionEvidenceItems(runDetail) {
  const artifactTypes = runArtifactTypes(runDetail);
  const eventTypes = new Set((state.selectedRunEvents ?? []).map((event) => event.event_type));
  const taskCounts = normalizedTaskCounts(runDetail.task_counts);
  const planningArtifactTypes = [
    BACKLOG_ARTIFACT_TYPE,
    POLICY_REPORT_ARTIFACT_TYPE,
    DISPATCH_PLAN_ARTIFACT_TYPE,
  ];
  const planningArtifactCount = planningArtifactTypes.filter((artifactType) =>
    artifactTypes.has(artifactType)
  ).length;
  const reportCount = agentTaskReportArtifacts(runDetail).length;
  const logCount = agentExecutionLogArtifacts(runDetail).length;
  const agentCount = runAgents(runDetail).length;
  const qualityReportReady =
    artifactTypes.has(QUALITY_REPORT_ARTIFACT_TYPE) ||
    eventTypes.has(RUN_QUALITY_EVALUATED_EVENT_TYPE);
  const prEvidenceTypes = [
    PR_CANDIDATE_ARTIFACT_TYPE,
    PR_EXPORT_ARTIFACT_TYPE,
    PR_PUBLICATION_ARTIFACT_TYPE,
    GITHUB_PULL_REQUEST_ARTIFACT_TYPE,
  ];
  const prEvidenceCount = prEvidenceTypes.filter((artifactType) =>
    artifactTypes.has(artifactType)
  ).length;
  const githubPrReady =
    artifactTypes.has(GITHUB_PULL_REQUEST_ARTIFACT_TYPE) ||
    eventTypes.has(GITHUB_PR_OPENED_EVENT_TYPE);
  const executionBlocked = runDetail.status === "failed" || taskCounts.failed > 0;
  const executionComplete =
    runDetail.status === "succeeded" &&
    taskCounts.total > 0 &&
    taskCounts.queued === 0 &&
    taskCounts.running === 0 &&
    taskCounts.failed === 0;

  return [
    {
      kicker: "Planning contract",
      title:
        planningArtifactCount === planningArtifactTypes.length
          ? "Backlog and routing are materialized"
          : "Waiting for full planning evidence",
      tone: planningArtifactCount === planningArtifactTypes.length ? "success" : "warning",
      summary:
        "The run needs a backlog, policy report, and agent dispatch plan before execution is explainable.",
      detail: `${planningArtifactCount}/${planningArtifactTypes.length} planning artifacts · ${taskCounts.total} task(s)`,
    },
    {
      kicker: "Agent execution",
      title: executionBlocked
        ? "Execution needs inspection"
        : executionComplete
          ? "Agent work is complete"
          : "Agent work is still moving",
      tone: executionBlocked ? "error" : executionComplete ? "success" : "warning",
      summary:
        "Reports and logs make agent behavior inspectable instead of hiding work inside a black-box session.",
      detail: `${agentCount || 0} agent(s) · ${reportCount} report(s) · ${logCount} log artifact(s)`,
    },
    {
      kicker: "Quality gate",
      title: qualityReportReady
        ? "Quality evidence is present"
        : runDetail.status === "succeeded"
          ? "Ready for quality evaluation"
          : "Locked until execution succeeds",
      tone: qualityReportReady ? "success" : runDetail.status === "failed" ? "error" : "neutral",
      summary:
        "Promotion stays gated by an explicit quality report so draft-PR handoff is auditable.",
      detail: qualityReportReady
        ? "Quality report or quality event recorded"
        : "No quality evidence recorded yet",
    },
    {
      kicker: "GitHub handoff",
      title: githubPrReady
        ? "Draft PR handoff is recorded"
        : prEvidenceCount > 0
          ? "Promotion evidence is accumulating"
          : "Promotion has not started",
      tone: githubPrReady ? "success" : prEvidenceCount > 0 ? "warning" : "neutral",
      summary:
        "The orchestrator prepares reviewable artifacts, while human approval remains in GitHub.",
      detail: `${prEvidenceCount}/${prEvidenceTypes.length} promotion artifacts`,
    },
  ];
}

function renderMissionEvidenceItem(item) {
  const tone = normalizePulseTone(item.tone);

  return `
    <article class="mission-evidence-card mission-evidence-card-${escapeHtml(tone)}">
      <div class="mission-feed-head">
        <div>
          <p class="panel-kicker">${escapeHtml(item.kicker)}</p>
          <h4>${escapeHtml(item.title)}</h4>
        </div>
        <span class="badge badge-${escapeHtml(tone)}">${escapeHtml(toneLabel(tone))}</span>
      </div>
      <p>${escapeHtml(item.summary)}</p>
      <div class="mission-feed-meta">
        <span>${escapeHtml(item.detail)}</span>
      </div>
    </article>
  `;
}

function renderMissionStageCard(stage, index) {
  return `
    <article class="mission-stage-card mission-stage-card-${escapeHtml(stage.state)}">
      <span class="mission-stage-marker">${escapeHtml(String(index + 1))}</span>
      <div class="mission-feed-head">
        <div>
          <p class="panel-kicker">Stage ${escapeHtml(String(index + 1))}</p>
          <h3>${escapeHtml(stage.title)}</h3>
        </div>
        <span class="badge badge-${escapeHtml(guideTone(stage.state))}">${escapeHtml(
          guideBadgeLabel(stage.state)
        )}</span>
      </div>
      <p class="mission-stage-detail">${escapeHtml(stage.detail)}</p>
    </article>
  `;
}

function renderMissionMiniCard(kicker, title, detail, tone) {
  return `
    <article class="mission-mini-card">
      <div class="mission-feed-head">
        <p class="panel-kicker">${escapeHtml(kicker)}</p>
        <span class="badge badge-${escapeHtml(normalizePulseTone(tone))}">${escapeHtml(
          toneLabel(normalizePulseTone(tone))
        )}</span>
      </div>
      <h3>${escapeHtml(title)}</h3>
      <p>${escapeHtml(detail)}</p>
    </article>
  `;
}

function renderMissionFeedItem(item) {
  return `
    <article class="mission-feed-item">
      <div class="mission-feed-head">
        <div>
          <p class="panel-kicker">${escapeHtml(item.kicker)}</p>
          <h4>${escapeHtml(item.title)}</h4>
        </div>
        <span class="badge badge-${escapeHtml(item.tone)}">${escapeHtml(item.badge)}</span>
      </div>
      <p>${escapeHtml(item.summary)}</p>
      <div class="mission-feed-meta">
        <span>${escapeHtml(item.detail)}</span>
      </div>
    </article>
  `;
}

function renderMissionArtifactItem(artifact) {
  const metadataParts = [];
  metadataParts.push(artifact.format ?? "unknown format");
  if (artifact.created_at) {
    metadataParts.push(formatTimestamp(artifact.created_at));
  }

  return `
    <article class="mission-feed-item">
      <div class="mission-feed-head">
        <div>
          <p class="panel-kicker">Artifact</p>
          <h4>${escapeHtml(artifact.artifact_type)}</h4>
        </div>
        <span class="badge badge-neutral">${escapeHtml(shortId(artifact.artifact_id))}</span>
      </div>
      <p>${escapeHtml(artifact.location_value)}</p>
      <div class="mission-feed-meta">
        <span>${escapeHtml(metadataParts.join(" · "))}</span>
      </div>
    </article>
  `;
}

function syncSelectedAgentActivity(runDetail) {
  const availableAgents = new Set(runAgents(runDetail));
  if (!availableAgents.size) {
    state.selectedAgentActivityId = "all";
    state.selectedAgentReportArtifactId = null;
    state.selectedAgentLogArtifactId = null;
    return;
  }

  if (
    state.selectedAgentActivityId !== "all" &&
    !availableAgents.has(state.selectedAgentActivityId)
  ) {
    state.selectedAgentActivityId = "all";
  }

  const reports = filteredAgentReportArtifacts(
    runDetail,
    state.selectedAgentActivityId,
    state.agentReportDetails
  );
  if (
    state.selectedAgentReportArtifactId &&
    !reports.some((report) => report.artifact.artifact_id === state.selectedAgentReportArtifactId)
  ) {
    state.selectedAgentReportArtifactId = reports[0]?.artifact.artifact_id ?? null;
  } else if (!state.selectedAgentReportArtifactId) {
    state.selectedAgentReportArtifactId = reports[0]?.artifact.artifact_id ?? null;
  }

  const logs = filteredAgentExecutionLogArtifacts(
    runDetail,
    state.selectedAgentActivityId,
    state.agentLogDetails
  );
  if (
    state.selectedAgentLogArtifactId &&
    !logs.some((log) => log.artifact.artifact_id === state.selectedAgentLogArtifactId)
  ) {
    state.selectedAgentLogArtifactId = logs[0]?.artifact.artifact_id ?? null;
  } else if (!state.selectedAgentLogArtifactId) {
    state.selectedAgentLogArtifactId = logs[0]?.artifact.artifact_id ?? null;
  }
}

function setSelectedAgentActivity(agentId) {
  const nextAgentId = typeof agentId === "string" && agentId.trim() ? agentId.trim() : "all";
  if (state.selectedAgentActivityId === nextAgentId) {
    return;
  }

  state.selectedAgentActivityId = nextAgentId;
  if (state.selectedRunDetail) {
    const reports = filteredAgentReportArtifacts(
      state.selectedRunDetail,
      state.selectedAgentActivityId,
      state.agentReportDetails
    );
    const logs = filteredAgentExecutionLogArtifacts(
      state.selectedRunDetail,
      state.selectedAgentActivityId,
      state.agentLogDetails
    );
    state.selectedAgentReportArtifactId = reports[0]?.artifact.artifact_id ?? null;
    state.selectedAgentLogArtifactId = logs[0]?.artifact.artifact_id ?? null;
  } else {
    state.selectedAgentReportArtifactId = null;
    state.selectedAgentLogArtifactId = null;
  }
  renderMissionAgentsPanel();
}

function setSelectedAgentReport(artifactId) {
  if (!artifactId || state.selectedAgentReportArtifactId === artifactId) {
    return;
  }

  state.selectedAgentReportArtifactId = artifactId;
  renderMissionAgentsPanel();
}

function setSelectedAgentLog(artifactId) {
  if (!artifactId || state.selectedAgentLogArtifactId === artifactId) {
    return;
  }

  state.selectedAgentLogArtifactId = artifactId;
  renderMissionAgentsPanel();
}

function scheduleMissionAgentsPanelRender() {
  if (state.activeMissionTab !== "agents" || state.agentPanelRenderQueued) {
    return;
  }

  state.agentPanelRenderQueued = true;
  window.requestAnimationFrame(() => {
    state.agentPanelRenderQueued = false;
    if (state.activeMissionTab !== "agents") {
      return;
    }
    renderMissionAgentsPanel();
  });
}

function runAgents(runDetail) {
  if (!Array.isArray(runDetail?.tasks)) {
    return [];
  }

  return runDetail.tasks
    .map(agentNameForTask)
    .filter(Boolean)
    .filter(uniqueValue);
}

function agentNameForTask(task) {
  return task?.assigned_agent ?? task?.agent_execution?.agent ?? "";
}

function ensureAgentReportDetails(runDetail) {
  const reports = agentTaskReportArtifacts(runDetail);
  if (!reports.length) {
    ensureAgentLogDetails(runDetail);
    return;
  }

  const pending = reports.filter((artifact) => {
    const artifactId = artifact.artifact_id;
    return (
      !state.agentReportDetails[artifactId] &&
      state.agentReportLoadsInFlight[artifactId] !== true
    );
  });

  if (!pending.length) {
    syncSelectedAgentActivity(runDetail);
    ensureAgentLogDetails(runDetail);
    ensureLinkedAgentArtifactDetails(runDetail);
    return;
  }

  pending.forEach((artifact) => {
    const artifactId = artifact.artifact_id;
    state.agentReportLoadsInFlight[artifactId] = true;
    fetchJsonEnvelope(`/artifacts/${encodeURIComponent(artifactId)}`)
      .then((envelope) => {
        state.agentReportDetails[artifactId] = envelope.ok ? envelope.data : envelope;
      })
      .catch((error) => {
        state.agentReportDetails[artifactId] = {
          error: error.message,
        };
      })
      .finally(() => {
        delete state.agentReportLoadsInFlight[artifactId];
        if (state.selectedRunDetail?.run_id === runDetail.run_id) {
          ensureAgentLogDetails(state.selectedRunDetail);
          ensureLinkedAgentArtifactDetails(state.selectedRunDetail);
          syncSelectedAgentActivity(state.selectedRunDetail);
          scheduleMissionAgentsPanelRender();
        }
      });
  });
}

function ensureAgentLogDetails(runDetail) {
  const logs = agentExecutionLogArtifacts(runDetail);
  if (!logs.length) {
    return;
  }

  const pending = logs.filter((artifact) => {
    const artifactId = artifact.artifact_id;
    return (
      !state.agentLogDetails[artifactId] &&
      state.agentLogLoadsInFlight[artifactId] !== true
    );
  });

  if (!pending.length) {
    syncSelectedAgentActivity(runDetail);
    ensureLinkedAgentArtifactDetails(runDetail);
    return;
  }

  pending.forEach((artifact) => {
    const artifactId = artifact.artifact_id;
    state.agentLogLoadsInFlight[artifactId] = true;
    fetchJsonEnvelope(`/artifacts/${encodeURIComponent(artifactId)}`)
      .then((envelope) => {
        state.agentLogDetails[artifactId] = envelope.ok ? envelope.data : envelope;
      })
      .catch((error) => {
        state.agentLogDetails[artifactId] = {
          error: error.message,
        };
      })
      .finally(() => {
        delete state.agentLogLoadsInFlight[artifactId];
        if (state.selectedRunDetail?.run_id === runDetail.run_id) {
          ensureLinkedAgentArtifactDetails(state.selectedRunDetail);
          syncSelectedAgentActivity(state.selectedRunDetail);
          scheduleMissionAgentsPanelRender();
        }
      });
  });
}

function ensureLinkedAgentArtifactDetails(runDetail) {
  const pendingArtifactIds = linkedAgentArtifactIdsForRun(runDetail)
    .filter(Boolean)
    .filter(uniqueValue)
    .filter((artifactId) => {
      return (
        !state.agentLinkedArtifactDetails[artifactId] &&
        state.agentLinkedArtifactLoadsInFlight[artifactId] !== true
      );
    });

  pendingArtifactIds.forEach((artifactId) => {
    state.agentLinkedArtifactLoadsInFlight[artifactId] = true;
    fetchJsonEnvelope(`/artifacts/${encodeURIComponent(artifactId)}`)
      .then((envelope) => {
        state.agentLinkedArtifactDetails[artifactId] = envelope.ok ? envelope.data : envelope;
      })
      .catch((error) => {
        state.agentLinkedArtifactDetails[artifactId] = {
          error: error.message,
        };
      })
      .finally(() => {
        delete state.agentLinkedArtifactLoadsInFlight[artifactId];
        if (state.selectedRunDetail?.run_id === runDetail.run_id) {
          scheduleMissionAgentsPanelRender();
        }
      });
  });
}

function linkedAgentArtifactIdsForRun(runDetail) {
  return [
    ...filteredAgentReportArtifacts(runDetail, "all", state.agentReportDetails).map(
      linkedAgentArtifactIdForReport
    ),
    ...filteredAgentExecutionLogArtifacts(runDetail, "all", state.agentLogDetails).map(
      linkedAgentArtifactIdForLog
    ),
  ];
}

function ensureMissionAgentsShell() {
  if (elements.missionAgentsPanel.dataset.mode === "detail") {
    return;
  }

  setRenderedHtml(
    elements.missionAgentsPanel,
    `
      <div id="missionAgentsLayout" class="mission-flow-layout">
        <div id="agentOverviewStrip" class="mission-mini-grid agent-overview-grid"></div>
        <section class="agent-handoff-shell">
          <div class="detail-section-head">
            <div>
              <p class="panel-kicker">Agent handoff map</p>
              <h3>How work moves through the selected agent scope</h3>
            </div>
            <span id="agentHandoffCount" class="badge badge-neutral">0</span>
          </div>
          <div id="agentHandoffStrip" class="agent-handoff-strip"></div>
        </section>
        <div id="agentFilterRow" class="agent-filter-row"></div>
        <div id="agentLaneGrid" class="agent-lane-grid"></div>
        <section class="agent-log-shell">
          <div class="detail-section-head">
            <div>
              <p class="panel-kicker">Agent event log</p>
              <h3>Task movement for the filtered agent set</h3>
            </div>
            <span id="agentEventCount" class="badge badge-neutral">0</span>
          </div>
          <div id="agentEventLogList" class="agent-log-list"></div>
        </section>
        <section class="agent-execution-shell">
          <div class="detail-section-head">
            <div>
              <p class="panel-kicker">Execution logs</p>
              <h3>Runtime stdout, stderr, and command context</h3>
            </div>
            <span id="agentExecutionCount" class="badge badge-neutral">0</span>
          </div>
          <div id="agentExecutionGrid" class="agent-log-grid"></div>
          <div id="agentExecutionDetail"></div>
        </section>
        <section class="agent-report-shell">
          <div class="detail-section-head">
            <div>
              <p class="panel-kicker">Persisted agent reports</p>
              <h3>Summaries and raw details from completed agent work</h3>
            </div>
            <span id="agentReportCount" class="badge badge-neutral">0</span>
          </div>
          <div id="agentReportGrid" class="agent-report-grid"></div>
          <div id="agentReportDetail"></div>
        </section>
      </div>
    `,
    { markUpdated: false }
  );
  elements.missionAgentsPanel.dataset.mode = "detail";
}

function renderMissionAgentsPanel() {
  const runDetail = state.selectedRunDetail;
  if (!runDetail) {
    elements.missionAgentsPanel.dataset.mode = "empty";
    setRenderedHtml(
      elements.missionAgentsPanel,
      renderSectionEmptyState(
        "Agent activity",
        "Open a run to inspect multi-agent execution",
        "Agent lanes become useful only after one run has tasks, run events, and external-agent reports to compare."
      ),
      { markUpdated: false }
    );
    return;
  }

  const tasks = Array.isArray(runDetail.tasks) ? runDetail.tasks : [];
  const agents = runAgents(runDetail);
  const reports = filteredAgentReportArtifacts(
    runDetail,
    state.selectedAgentActivityId,
    state.agentReportDetails
  );
  const selectedReport = selectedAgentReport(reports);
  const logs = filteredAgentExecutionLogArtifacts(
    runDetail,
    state.selectedAgentActivityId,
    state.agentLogDetails
  );
  const selectedLog = selectedAgentExecutionLog(logs);
  const filteredEvents = filteredAgentEvents(
    runDetail,
    state.selectedRunEvents,
    state.selectedAgentActivityId
  );
  const lanes = buildAgentLanes(runDetail, state.selectedRunEvents, state.agentReportDetails);
  const filteredLanes = lanes.filter((lane) =>
    state.selectedAgentActivityId === "all"
      ? true
      : lane.agentId === state.selectedAgentActivityId
  );

  if (
    !agents.length &&
    !reports.length &&
    !logs.length &&
    !tasks.some((task) => task.agent_execution?.mode === "external_agent")
  ) {
    elements.missionAgentsPanel.dataset.mode = "empty";
    setRenderedHtml(
      elements.missionAgentsPanel,
      renderSectionEmptyState(
        "Agent activity",
        "No external-agent activity is visible for this run",
        "Tasks are present, but none are currently assigned to an external agent or accompanied by persisted agent_task_report artifacts."
      ),
      { markUpdated: false }
    );
    return;
  }

  ensureMissionAgentsShell();

  setRenderedHtml(
    document.getElementById("agentOverviewStrip"),
    buildAgentOverviewCards({
      agents,
      filteredEvents,
      filteredLanes,
      logs,
      reports,
      runDetail,
      selectedLog,
      selectedReport,
    })
      .map((card) => renderMissionMiniCard(card.kicker, card.title, card.detail, card.tone))
      .join(""),
    { markUpdated: false }
  );

  const handoffItems = buildAgentHandoffItems({
    filteredEvents,
    filteredLanes,
    logs,
    reports,
  });
  setTextContent(document.getElementById("agentHandoffCount"), String(handoffItems.length), {
    markUpdated: false,
  });
  setRenderedHtml(
    document.getElementById("agentHandoffStrip"),
    handoffItems.map(renderAgentHandoffItem).join(""),
    { markUpdated: false }
  );

  setRenderedHtml(
    document.getElementById("agentFilterRow"),
    `
      ${renderAgentFilterChip("all", "All agents", state.selectedAgentActivityId === "all")}
      ${agents
        .map((agentId) =>
          renderAgentFilterChip(
            agentId,
            `${agentId}${laneTaskCountSuffix(lanes, agentId)}`,
            state.selectedAgentActivityId === agentId
          )
        )
        .join("")}
    `,
    { markUpdated: false }
  );

  setRenderedHtml(
    document.getElementById("agentLaneGrid"),
    filteredLanes.length
      ? filteredLanes.map(renderAgentLane).join("")
      : renderSectionEmptyState(
          "Agent lanes",
          "No agent lanes are available yet",
          "Assigned-agent task state will appear here once the run records external-agent work."
        ),
    { markUpdated: false }
  );

  setTextContent(document.getElementById("agentEventCount"), String(filteredEvents.length), {
    markUpdated: false,
  });
  setRenderedHtml(
    document.getElementById("agentEventLogList"),
    filteredEvents.length
      ? filteredEvents.map((event) => renderAgentLogItem(runDetail, event)).join("")
      : renderSectionEmptyState(
          "Agent event log",
          "No task events match the current agent filter",
          "Select another agent or wait for a task event, heartbeat, success, failure, or requeue update."
        ),
    { markUpdated: false }
  );

  setTextContent(document.getElementById("agentExecutionCount"), String(logs.length), {
    markUpdated: false,
  });
  setRenderedHtml(
    document.getElementById("agentExecutionGrid"),
    logs.length
      ? logs.map((log) => renderAgentExecutionLogCard(runDetail, log)).join("")
      : renderSectionEmptyState(
          "Execution logs",
          "No runtime log artifacts match the current agent filter",
          "Logs appear when worker or runtime-backed tasks persist execution artifacts for the selected run."
        ),
    { markUpdated: false }
  );
  setRenderedHtml(
    document.getElementById("agentExecutionDetail"),
    selectedLog ? renderSelectedAgentExecutionLog(runDetail, selectedLog) : "",
    { markUpdated: false }
  );

  setTextContent(document.getElementById("agentReportCount"), String(reports.length), {
    markUpdated: false,
  });
  setRenderedHtml(
    document.getElementById("agentReportGrid"),
    reports.length
      ? reports.map(renderAgentReportCard).join("")
      : renderSectionEmptyState(
          "Agent reports",
          "No persisted agent_task_report artifacts match the current filter",
          "Reports appear after an external agent completes or retries a claimed task."
        ),
    { markUpdated: false }
  );
  setRenderedHtml(
    document.getElementById("agentReportDetail"),
    selectedReport ? renderSelectedAgentReportConsole(runDetail, selectedReport) : "",
    { markUpdated: false }
  );
}

function buildAgentOverviewCards({
  agents,
  filteredEvents,
  filteredLanes,
  logs,
  reports,
  runDetail,
  selectedLog,
  selectedReport,
}) {
  const visibleTaskCount = filteredLanes.reduce((count, lane) => count + lane.taskCount, 0);
  const queuedCount = filteredLanes.reduce((count, lane) => count + lane.queuedCount, 0);
  const runningCount = filteredLanes.reduce((count, lane) => count + lane.runningCount, 0);
  const failedCount = filteredLanes.reduce((count, lane) => count + lane.failedCount, 0);
  const succeededCount = filteredLanes.reduce((count, lane) => count + lane.succeededCount, 0);
  const selectedAgent =
    state.selectedAgentActivityId === "all" ? "All agents" : state.selectedAgentActivityId;
  const agentRoster = agents.length
    ? truncateText(agents.join(", "), 90)
    : "No agent assignments yet";
  const reportSummary =
    selectedReport?.detail?.manifest?.summary ??
    selectedReport?.artifact?.metadata?.summary ??
    "No selected agent report yet.";
  const selectedLogTask = selectedLog ? taskForAgentLog(runDetail, selectedLog) : null;
  const logSummary = selectedLog
    ? `${selectedLogTask?.backlog_item_id ?? selectedLog.detail?.manifest?.task_kind ?? "runtime"} · ${executionLogStatus(selectedLog)}`
    : "No selected runtime log yet.";

  return [
    {
      kicker: "Agent scope",
      title: selectedAgent,
      detail: `${filteredLanes.length} visible lane(s) · ${agents.length} agent(s): ${agentRoster}`,
      tone: filteredLanes.length ? "success" : "neutral",
    },
    {
      kicker: "Task pressure",
      title: `${visibleTaskCount} visible task(s)`,
      detail: `${runningCount} running · ${queuedCount} queued · ${succeededCount} succeeded · ${failedCount} failed`,
      tone: failedCount > 0 ? "error" : runningCount > 0 || queuedCount > 0 ? "warning" : "success",
    },
    {
      kicker: "Evidence",
      title: `${reports.length} report(s) · ${logs.length} log(s)`,
      detail: truncateText(reportSummary, 120),
      tone: reports.length || logs.length ? "success" : "neutral",
    },
    {
      kicker: "Latest signal",
      title: `${filteredEvents.length} event(s)`,
      detail: logSummary,
      tone: filteredEvents.length ? "warning" : "neutral",
    },
  ];
}

function buildAgentHandoffItems({ filteredEvents, filteredLanes, logs, reports }) {
  const visibleTaskCount = filteredLanes.reduce((count, lane) => count + lane.taskCount, 0);
  const startedCount = filteredEvents.filter(
    (event) => event.event_type === TASK_STARTED_EVENT_TYPE
  ).length;
  const workspaceCount = filteredEvents.filter(
    (event) => event.event_type === TASK_WORKSPACE_PREPARED_EVENT_TYPE
  ).length;
  const heartbeatCount = filteredEvents.filter(
    (event) => event.event_type === TASK_HEARTBEAT_EVENT_TYPE
  ).length;
  const failedCount = filteredLanes.reduce((count, lane) => count + lane.failedCount, 0);
  const succeededCount = filteredLanes.reduce((count, lane) => count + lane.succeededCount, 0);
  const completedCount = succeededCount + failedCount;
  const activeCount = filteredLanes.reduce(
    (count, lane) => count + lane.runningCount + lane.queuedCount,
    0
  );

  return [
    {
      kicker: "Assignment",
      title: visibleTaskCount ? `${visibleTaskCount} task(s) routed` : "No routed tasks",
      detail: `${filteredLanes.length} visible lane(s) in the current filter`,
      tone: visibleTaskCount ? "success" : "neutral",
    },
    {
      kicker: "Claim / start",
      title: startedCount ? `${startedCount} start event(s)` : "No start event yet",
      detail: "A start event proves a worker or external agent began the task path.",
      tone: startedCount ? "success" : visibleTaskCount ? "warning" : "neutral",
    },
    {
      kicker: "Workspace prep",
      title: workspaceCount ? `${workspaceCount} workspace handoff(s)` : "No workspace handoff",
      detail: "Prepared workspaces bind agent output to a run-scoped input artifact.",
      tone: workspaceCount ? "success" : startedCount ? "warning" : "neutral",
    },
    {
      kicker: "Lease signal",
      title: heartbeatCount ? `${heartbeatCount} heartbeat(s)` : "No heartbeat recorded",
      detail: "Heartbeats prove longer agent sessions are still actively leased.",
      tone: heartbeatCount ? "success" : activeCount ? "warning" : "neutral",
    },
    {
      kicker: "Completion report",
      title: reports.length ? `${reports.length} report(s)` : "No report yet",
      detail: `${succeededCount} succeeded · ${failedCount} failed · ${activeCount} active or queued`,
      tone: failedCount > 0 ? "error" : reports.length || completedCount > 0 ? "success" : "neutral",
    },
    {
      kicker: "Runtime evidence",
      title: logs.length ? `${logs.length} log artifact(s)` : "No runtime logs",
      detail: "Logs preserve command context, stdout, stderr, and workspace linkage.",
      tone: logs.length ? "success" : reports.length ? "warning" : "neutral",
    },
  ];
}

function renderAgentHandoffItem(item) {
  const tone = normalizePulseTone(item.tone);

  return `
    <article class="agent-handoff-card agent-handoff-card-${escapeHtml(tone)}">
      <div class="mission-feed-head">
        <p class="panel-kicker">${escapeHtml(item.kicker)}</p>
        <span class="badge badge-${escapeHtml(tone)}">${escapeHtml(toneLabel(tone))}</span>
      </div>
      <h4>${escapeHtml(item.title)}</h4>
      <p>${escapeHtml(item.detail)}</p>
    </article>
  `;
}

function renderAgentFilterChip(agentId, label, isActive) {
  return `
    <button
      class="agent-filter-chip${isActive ? " is-active" : ""}"
      type="button"
      data-agent-filter="${escapeHtml(agentId)}"
      data-ui-stable-key="agent-filter:${escapeHtml(agentId)}"
      aria-pressed="${isActive ? "true" : "false"}"
    >
      ${escapeHtml(label)}
    </button>
  `;
}

function buildAgentLanes(runDetail, events, reportDetails) {
  return runAgents(runDetail).map((agentId) => {
    const tasks = (runDetail.tasks || []).filter((task) => agentNameForTask(task) === agentId);
    const filteredEvents = filteredAgentEvents(runDetail, events, agentId);
    const reports = filteredAgentReportArtifacts(runDetail, agentId, reportDetails);
    const executors = tasks
      .map((task) => task.agent_execution?.executor_id)
      .filter(Boolean)
      .filter(uniqueValue);
    const statusCounts = loadedRunStatusCounts(
      tasks.map((task) => ({
        status: task.status === "running" ? "executing" : task.status,
      }))
    );
    const latestReport = reports[0] ?? null;
    const latestEvent = filteredEvents[0] ?? null;

    return {
      agentId,
      taskCount: tasks.length,
      queuedCount: statusCounts.queued,
      runningCount: statusCounts.executing,
      failedCount: statusCounts.failed,
      succeededCount: statusCounts.succeeded,
      executors,
      latestEventAt: latestEvent?.created_at ?? null,
      latestEventSummary: latestEvent?.summary ?? "No task event recorded yet.",
      latestReport,
      latestReportStatus:
        latestReport?.detail?.manifest?.task_status ??
        latestReport?.artifact?.metadata?.task_status ??
        "n/a",
    };
  });
}

function laneTaskCountSuffix(lanes, agentId) {
  const lane = lanes.find((candidate) => candidate.agentId === agentId);
  return lane ? ` · ${lane.taskCount} task(s)` : "";
}

function renderAgentLane(lane) {
  const latestReportSummary =
    lane.latestReport?.detail?.manifest?.summary ??
    lane.latestReport?.artifact?.metadata?.summary ??
    "No persisted report yet.";

  return `
    <article class="agent-lane">
      <div class="agent-lane-head">
        <div>
          <p class="panel-kicker">Assigned agent</p>
          <h3>${escapeHtml(lane.agentId)}</h3>
        </div>
        <span class="badge badge-${escapeHtml(
          normalizePulseTone(statusTone(lane.latestReportStatus))
        )}">${escapeHtml(lane.latestReportStatus)}</span>
      </div>
      <div class="agent-lane-stats">
        <div class="agent-lane-stat">
          <span class="agent-lane-stat-label">Tasks</span>
          <span class="agent-lane-stat-value">${escapeHtml(String(lane.taskCount))}</span>
        </div>
        <div class="agent-lane-stat">
          <span class="agent-lane-stat-label">Executors</span>
          <span class="agent-lane-stat-value">${escapeHtml(
            lane.executors.length ? lane.executors.length : 0
          )}</span>
        </div>
        <div class="agent-lane-stat">
          <span class="agent-lane-stat-label">Queued / Running</span>
          <span class="agent-lane-stat-value">${escapeHtml(
            `${lane.queuedCount} / ${lane.runningCount}`
          )}</span>
        </div>
        <div class="agent-lane-stat">
          <span class="agent-lane-stat-label">Ok / Failed</span>
          <span class="agent-lane-stat-value">${escapeHtml(
            `${lane.succeededCount} / ${lane.failedCount}`
          )}</span>
        </div>
      </div>
      <p>${escapeHtml(latestReportSummary)}</p>
      <div class="agent-lane-meta">
        <span>${escapeHtml(
          lane.executors.length ? lane.executors.join(", ") : "No executor id recorded"
        )}</span>
        <span>${escapeHtml(
          lane.latestEventAt ? formatTimestamp(lane.latestEventAt) : "No event timestamp"
        )}</span>
      </div>
    </article>
  `;
}

function filteredAgentEvents(runDetail, events, agentId) {
  const taskIndex = new Map(
    (runDetail?.tasks || []).map((task) => [task.task_id, task])
  );

  return (Array.isArray(events) ? events : [])
    .filter((event) => {
      if (agentId === "all") {
        return event.scope === "task" || Boolean(event.task_id);
      }
      if (!event.task_id) {
        return false;
      }
      return agentNameForTask(taskIndex.get(event.task_id)) === agentId;
    })
    .sort((left, right) => sortableTimestamp(right.created_at) - sortableTimestamp(left.created_at));
}

function renderAgentLogItem(runDetail, event) {
  const task = taskForRunEvent(runDetail, event);
  const agentId = task ? agentNameForTask(task) : "run";
  const taskLabel = task?.title ?? task?.backlog_item_id ?? "Run-level event";
  const eventTone = normalizePulseTone(statusTone(event.status ?? event.scope));

  return `
    <article class="agent-log-item">
      <div class="agent-log-head">
        <div>
          <p class="panel-kicker">${escapeHtml(agentId || "run")}</p>
          <h4>${escapeHtml(displayRunEventType(event.event_type))}</h4>
        </div>
        <span class="badge badge-${escapeHtml(eventTone)}">
          ${escapeHtml(displayRunEventBadge(event))}
        </span>
      </div>
      <p>${escapeHtml(event.summary || runEventFallbackSummary(event.event_type))}</p>
      <div class="agent-log-meta">
        <span>${escapeHtml(taskLabel)}</span>
        <span>${escapeHtml(`raw: ${event.event_type}`)}</span>
        <span>${escapeHtml(formatTimestamp(event.created_at))}</span>
        <span class="mono">${escapeHtml(
          event.task_id ? shortId(event.task_id) : shortId(event.event_id)
        )}</span>
      </div>
    </article>
  `;
}

function agentTaskReportArtifacts(runDetail) {
  return (Array.isArray(runDetail?.artifacts) ? runDetail.artifacts : [])
    .filter((artifact) => artifact.artifact_type === "agent_task_report")
    .sort((left, right) => sortableTimestamp(right.created_at) - sortableTimestamp(left.created_at));
}

function agentExecutionLogArtifacts(runDetail) {
  return (Array.isArray(runDetail?.artifacts) ? runDetail.artifacts : [])
    .filter((artifact) => artifact.artifact_type === "log")
    .sort((left, right) => sortableTimestamp(right.created_at) - sortableTimestamp(left.created_at));
}

function filteredAgentReportArtifacts(runDetail, agentId, reportDetails) {
  return agentTaskReportArtifacts(runDetail)
    .map((artifact) => ({
      artifact,
      detail: reportDetails[artifact.artifact_id] ?? null,
    }))
    .filter((report) => {
      if (agentId === "all") {
        return true;
      }
      return agentIdForReport(report) === agentId;
    });
}

function filteredAgentExecutionLogArtifacts(runDetail, agentId, logDetails) {
  return agentExecutionLogArtifacts(runDetail)
    .map((artifact) => ({
      artifact,
      detail: logDetails[artifact.artifact_id] ?? null,
    }))
    .filter((log) => {
      if (agentId === "all") {
        return true;
      }
      const task = taskForAgentLog(runDetail, log);
      return agentNameForTask(task) === agentId;
    });
}

function agentIdForReport(report) {
  return (
    report?.detail?.manifest?.assigned_agent ??
    report?.detail?.metadata?.assigned_agent ??
    report?.artifact?.metadata?.assigned_agent ??
    ""
  );
}

function selectedAgentReport(reports) {
  if (!reports.length) {
    state.selectedAgentReportArtifactId = null;
    return null;
  }

  const selected =
    reports.find(
      (report) => report.artifact.artifact_id === state.selectedAgentReportArtifactId
    ) ?? reports[0];
  state.selectedAgentReportArtifactId = selected.artifact.artifact_id;
  return selected;
}

function selectedAgentExecutionLog(logs) {
  if (!logs.length) {
    state.selectedAgentLogArtifactId = null;
    return null;
  }

  const selected =
    logs.find((log) => log.artifact.artifact_id === state.selectedAgentLogArtifactId) ??
    logs[0];
  state.selectedAgentLogArtifactId = selected.artifact.artifact_id;
  return selected;
}

function taskForAgentReport(runDetail, report) {
  const taskId =
    report?.detail?.manifest?.task_id ??
    report?.detail?.metadata?.task_id ??
    report?.artifact?.metadata?.task_id;
  if (typeof taskId !== "string" || !taskId.trim()) {
    return null;
  }

  return (runDetail?.tasks || []).find((task) => task.task_id === taskId) ?? null;
}

function taskForAgentLog(runDetail, log) {
  const taskId =
    log?.detail?.manifest?.task_id ??
    log?.detail?.metadata?.task_id ??
    log?.artifact?.metadata?.task_id;
  if (typeof taskId !== "string" || !taskId.trim()) {
    return null;
  }

  return (runDetail?.tasks || []).find((task) => task.task_id === taskId) ?? null;
}

function renderAgentReportCard(report) {
  const manifest = report.detail?.manifest ?? null;
  const summary =
    manifest?.summary ??
    report.artifact.metadata?.summary ??
    "Waiting for the structured agent report to load.";
  const secondary =
    manifest?.details ??
    report.detail?.text_preview ??
    report.detail?.error ??
    "Select this report to inspect its full details.";
  const isSelected = report.artifact.artifact_id === state.selectedAgentReportArtifactId;
  const agentId = agentIdForReport(report) || "unknown";

  return `
    <button
      class="agent-report-card${isSelected ? " is-selected" : ""}"
      type="button"
      data-agent-report-artifact-id="${escapeHtml(report.artifact.artifact_id)}"
      data-ui-stable-key="agent-report:${escapeHtml(report.artifact.artifact_id)}"
      aria-pressed="${isSelected ? "true" : "false"}"
    >
      <div class="surface-link-head">
        <div>
          <p class="panel-kicker">${escapeHtml(agentId)}</p>
          <h4>${escapeHtml(
            manifest?.backlog_item_id ?? report.artifact.metadata?.backlog_item_id ?? shortId(report.artifact.artifact_id)
          )}</h4>
        </div>
        <span class="badge badge-${escapeHtml(
          normalizePulseTone(statusTone(manifest?.task_status ?? "neutral"))
        )}">${escapeHtml(manifest?.task_status ?? "loading")}</span>
      </div>
      <p>${escapeHtml(summary)}</p>
      <div class="agent-lane-meta">
        <span>${escapeHtml(manifest?.executor_id ?? "executor unknown")}</span>
        <span>${escapeHtml(formatTimestamp(report.artifact.created_at))}</span>
      </div>
      <p class="microcopy">${escapeHtml(truncateText(secondary, 180))}</p>
    </button>
  `;
}

function executionLogStatus(log) {
  return (
    log?.detail?.manifest?.status ??
    log?.detail?.metadata?.status ??
    log?.artifact?.metadata?.status ??
    "loading"
  );
}

function executionLogPreview(log) {
  const stdout = log?.detail?.manifest?.stdout ?? "";
  const stderr = log?.detail?.manifest?.stderr ?? "";
  const preview = [stdout, stderr]
    .map((value) => String(value ?? "").trim())
    .find(Boolean);

  return preview || log?.detail?.error || "Select this log to inspect stdout and stderr.";
}

function formatStreamCaptureBytes(byteCount, content) {
  const numeric = Number(byteCount);
  if (Number.isFinite(numeric) && numeric >= 0) {
    return `${numeric.toLocaleString()} bytes`;
  }

  return `${String(content ?? "").length.toLocaleString()} char(s)`;
}

function renderAgentExecutionLogCard(runDetail, log) {
  const task = taskForAgentLog(runDetail, log);
  const manifest = log.detail?.manifest ?? null;
  const status = executionLogStatus(log);
  const isSelected = log.artifact.artifact_id === state.selectedAgentLogArtifactId;
  const agentId = agentNameForTask(task) || manifest?.provider || "runtime";
  const title =
    task?.backlog_item_id ??
    manifest?.task_kind ??
    task?.kind ??
    shortId(log.artifact.artifact_id);
  const secondary = executionLogPreview(log);

  return `
    <button
      class="agent-report-card${isSelected ? " is-selected" : ""}"
      type="button"
      data-agent-log-artifact-id="${escapeHtml(log.artifact.artifact_id)}"
      data-ui-stable-key="agent-log:${escapeHtml(log.artifact.artifact_id)}"
      aria-pressed="${isSelected ? "true" : "false"}"
    >
      <div class="surface-link-head">
        <div>
          <p class="panel-kicker">${escapeHtml(agentId)}</p>
          <h4>${escapeHtml(title)}</h4>
        </div>
        <span class="badge badge-${escapeHtml(
          normalizePulseTone(statusTone(status))
        )}">${escapeHtml(status)}</span>
      </div>
      <p>${escapeHtml(task?.title ?? manifest?.task_title ?? "Runtime execution log")}</p>
      <div class="agent-lane-meta">
        <span>${escapeHtml(manifest?.provider ?? task?.execution?.provider ?? "provider unknown")}</span>
        <span>${escapeHtml(formatTimestamp(log.artifact.created_at))}</span>
      </div>
      <p class="microcopy">${escapeHtml(truncateText(secondary, 180))}</p>
    </button>
  `;
}

function linkedAgentArtifactIdForReport(report) {
  const artifactId =
    report?.detail?.manifest?.task_workspace_input_artifact_id ??
    report?.detail?.metadata?.task_workspace_input_artifact_id ??
    report?.artifact?.metadata?.task_workspace_input_artifact_id;
  return typeof artifactId === "string" && artifactId.trim() ? artifactId.trim() : "";
}

function linkedAgentArtifactDetailForReport(report) {
  const artifactId = linkedAgentArtifactIdForReport(report);
  return artifactId ? state.agentLinkedArtifactDetails[artifactId] ?? null : null;
}

function linkedAgentArtifactLoadingForReport(report) {
  const artifactId = linkedAgentArtifactIdForReport(report);
  return artifactId ? state.agentLinkedArtifactLoadsInFlight[artifactId] === true : false;
}

function linkedAgentArtifactIdForLog(log) {
  const artifactId =
    log?.detail?.manifest?.workspace_input_artifact_id ??
    log?.detail?.metadata?.workspace_input_artifact_id ??
    log?.artifact?.metadata?.workspace_input_artifact_id;
  return typeof artifactId === "string" && artifactId.trim() ? artifactId.trim() : "";
}

function linkedAgentArtifactDetailForLog(log) {
  const artifactId = linkedAgentArtifactIdForLog(log);
  return artifactId ? state.agentLinkedArtifactDetails[artifactId] ?? null : null;
}

function linkedAgentArtifactLoadingForLog(log) {
  const artifactId = linkedAgentArtifactIdForLog(log);
  return artifactId ? state.agentLinkedArtifactLoadsInFlight[artifactId] === true : false;
}

function renderSelectedAgentExecutionLog(runDetail, log) {
  const task = taskForAgentLog(runDetail, log);
  const manifest = log.detail?.manifest ?? {};
  const status = executionLogStatus(log);
  const runtimeParts = [
    manifest.provider ?? task?.execution?.provider ?? "provider unknown",
    manifest.image ?? log.detail?.metadata?.image ?? "image unknown",
    `exit ${manifest.exit_code ?? "n/a"}`,
  ];
  const summaryCards = buildSelectedAgentLogSummaryCards(task, log);

  return `
    <div class="agent-report-actions">
      <span class="badge badge-${escapeHtml(
        normalizePulseTone(statusTone(status))
      )}">${escapeHtml(status)}</span>
      <span class="microcopy">${escapeHtml(runtimeParts.join(" · "))}</span>
    </div>
    <div class="mission-mini-grid agent-report-summary-grid">
      ${summaryCards.map(renderSelectedAgentReportSummaryCard).join("")}
    </div>
    <div class="agent-inspector-stack">
      ${renderAgentExecutionCommandCard(task, log)}
      ${renderAgentExecutionWorkspaceCard(log)}
    </div>
    <div class="agent-stream-grid">
      ${renderAgentExecutionStreamCard(
        "stdout",
        "Captured standard output",
        manifest.stdout,
        "The runtime did not persist stdout for this execution artifact."
      )}
      ${renderAgentExecutionStreamCard(
        "stderr",
        "Captured standard error",
        manifest.stderr,
        "The runtime did not persist stderr for this execution artifact."
      )}
    </div>
  `;
}

function buildSelectedAgentLogSummaryCards(task, log) {
  const manifest = log.detail?.manifest ?? {};
  const workspaceArtifactId = linkedAgentArtifactIdForLog(log);
  const timedOut = manifest.timed_out === true;
  const exitCode =
    manifest.exit_code == null ? "n/a" : String(manifest.exit_code);
  const outputTruncated =
    manifest.stdout_truncated === true || manifest.stderr_truncated === true;

  return [
    {
      kicker: "Task",
      title:
        task?.backlog_item_id ??
        manifest.task_kind ??
        task?.kind ??
        shortId(log.artifact.artifact_id),
      detail: task?.title ?? manifest.task_title ?? "Runtime execution log",
      tone: statusTone(task?.status ?? manifest.status ?? "neutral"),
    },
    {
      kicker: "Runtime",
      title: manifest.provider ?? task?.execution?.provider ?? "provider unknown",
      detail: manifest.image ?? log.detail?.metadata?.image ?? "image not recorded",
      tone: statusTone(manifest.status ?? "neutral"),
    },
    {
      kicker: "Exit posture",
      title: timedOut ? "Timed out" : `Exit ${exitCode}`,
      detail: `timeout ${manifest.timeout_seconds ?? "n/a"}s · ${manifest.status ?? "status unknown"}`,
      tone: timedOut ? "warning" : manifest.exit_code === 0 ? "success" : statusTone(manifest.status ?? "neutral"),
    },
    {
      kicker: "Workspace handoff",
      title: workspaceArtifactId ? shortId(workspaceArtifactId) : "Not captured",
      detail:
        manifest.workspace_path ??
        manifest.workspace_bundle_path ??
        "No prepared workspace artifact was linked",
      tone: workspaceArtifactId ? "warning" : "neutral",
    },
    {
      kicker: "Output capture",
      title: outputTruncated ? "Truncated" : "Bounded",
      detail: `stdout ${formatStreamCaptureBytes(
        manifest.stdout_bytes,
        manifest.stdout
      )} · stderr ${formatStreamCaptureBytes(manifest.stderr_bytes, manifest.stderr)}`,
      tone: outputTruncated ? "warning" : "success",
    },
  ];
}

function renderAgentExecutionCommandCard(task, log) {
  const manifest = log.detail?.manifest ?? {};
  const metadata = log.detail?.metadata ?? {};
  const command = Array.isArray(metadata.command)
    ? metadata.command
    : Array.isArray(manifest.command)
      ? manifest.command
      : [];
  const summaryEntries = [];
  pushConsoleSummaryEntry(summaryEntries, "Task", manifest.task_id ?? task?.task_id, {
    mono: true,
    short: true,
  });
  pushConsoleSummaryEntry(summaryEntries, "Provider", manifest.provider ?? task?.execution?.provider);
  pushConsoleSummaryEntry(summaryEntries, "Image", manifest.image ?? metadata.image);
  pushConsoleSummaryEntry(summaryEntries, "Timeout", manifest.timeout_seconds, {});
  pushConsoleSummaryEntry(summaryEntries, "Timed out", manifest.timed_out === true ? "yes" : "no");

  return renderAgentInspectorCard(
    "Execution command",
    "Runtime invocation",
    renderConsoleStructuredPayload(
      {
        command,
        provider: manifest.provider ?? task?.execution?.provider ?? null,
        image: manifest.image ?? metadata.image ?? null,
        workingDirectory:
          metadata.working_directory ??
          manifest.workspace_path ??
          metadata.workspace_path ??
          null,
        sandboxProfile: manifest.sandbox_profile ?? metadata.sandbox_profile ?? null,
        sandboxFlags: manifest.sandbox_flags ?? metadata.sandbox_flags ?? [],
        networkMode: manifest.network_mode ?? metadata.network_mode ?? null,
        timeoutSeconds: manifest.timeout_seconds ?? null,
        timedOut: manifest.timed_out === true,
        stdoutBytes: manifest.stdout_bytes ?? null,
        stderrBytes: manifest.stderr_bytes ?? null,
        stdoutTruncated: manifest.stdout_truncated === true,
        stderrTruncated: manifest.stderr_truncated === true,
      },
      summaryEntries,
      false
    )
  );
}

function renderAgentExecutionWorkspaceCard(log) {
  const manifest = log.detail?.manifest ?? {};
  const linkedDetail = linkedAgentArtifactDetailForLog(log);
  const linkedManifest = linkedDetail?.manifest ?? {};
  const workspaceArtifactId = linkedAgentArtifactIdForLog(log);

  if (!workspaceArtifactId) {
    return renderAgentInspectorCard(
      "Prepared workspace",
      "No linked prepared workspace artifact",
      '<div class="empty-state compact">This execution log does not reference a persisted task workspace input artifact.</div>'
    );
  }

  if (linkedAgentArtifactLoadingForLog(log)) {
    return renderAgentInspectorCard(
      "Prepared workspace",
      "Loading linked workspace artifact",
      '<div class="empty-state compact is-loading"><p>Inspecting the prepared workspace handoff...</p></div>'
    );
  }

  if (!linkedDetail || linkedDetail.error || !linkedDetail.artifact) {
    return renderAgentInspectorCard(
      "Prepared workspace",
      "Linked workspace artifact unavailable",
      renderSectionEmptyState(
        "Prepared workspace",
        "The linked task workspace input artifact could not be loaded",
        linkedDetail?.error ??
          formatEnvelopeError(linkedDetail) ??
          "No artifact detail payload was returned."
      )
    );
  }

  const summaryEntries = [];
  pushConsoleSummaryEntry(summaryEntries, "Artifact", workspaceArtifactId, {
    mono: true,
    short: true,
  });
  pushConsoleSummaryEntry(summaryEntries, "Workspace", manifest.workspace_path ?? linkedManifest.workspace_root, {
    mono: true,
  });
  pushConsoleSummaryEntry(summaryEntries, "Bundle", manifest.workspace_bundle_path ?? linkedDetail.resolved_path, {
    mono: true,
  });
  pushConsoleSummaryEntry(
    summaryEntries,
    "Source",
    linkedManifest.source_artifact_id ?? manifest.workspace_source_artifact_id,
    { mono: true, short: true }
  );

  return renderAgentInspectorCard(
    "Prepared workspace",
    linkedDetail.artifact.artifact_type ?? "task_workspace_input",
    renderConsoleStructuredPayload(
      {
        workspaceArtifactId,
        workspacePath: manifest.workspace_path ?? null,
        workspaceBundlePath: manifest.workspace_bundle_path ?? null,
        workspaceSourceArtifactId: manifest.workspace_source_artifact_id ?? null,
        workspaceInputArtifactId: manifest.workspace_input_artifact_id ?? null,
        preparedWorkspace: {
          sourceKind: linkedManifest.source_kind ?? null,
          sourceArtifactId: linkedManifest.source_artifact_id ?? null,
          workspaceRoot: linkedManifest.workspace_root ?? linkedDetail.resolved_path ?? null,
          bundleEntryCount: linkedManifest.bundle_entry_count ?? null,
          bundleByteCount: linkedManifest.bundle_byte_count ?? null,
          fileCount: linkedManifest.file_count ?? null,
        },
      },
      summaryEntries,
      false
    )
  );
}

function renderAgentExecutionStreamCard(kicker, title, value, emptyMessage) {
  const rendered = String(value ?? "");

  return renderAgentInspectorCard(
    kicker,
    title,
    rendered.trim()
      ? `<pre class="agent-report-pre">${escapeHtml(rendered)}</pre>`
      : `<div class="empty-state compact"><p>${escapeHtml(emptyMessage)}</p></div>`
  );
}

function renderSelectedAgentReportConsole(runDetail, report) {
  const manifest = report.detail?.manifest ?? null;
  const task = taskForAgentReport(runDetail, report);
  const details =
    manifest?.details ??
    report.detail?.text_preview ??
    report.detail?.error ??
    "No structured details were persisted for this report.";
  const metadataParts = [
    manifest?.assigned_agent ?? report.artifact.metadata?.assigned_agent ?? "unknown agent",
    manifest?.reported_status ?? report.artifact.metadata?.reported_status ?? "unknown status",
    manifest?.workspace_root ?? report.detail?.resolved_path ?? "workspace not recorded",
  ];
  const summaryCards = buildSelectedAgentReportSummaryCards(manifest, task, report);

  return `
    <div class="agent-report-actions">
      <span class="badge badge-${escapeHtml(
        normalizePulseTone(statusTone(manifest?.task_status ?? "neutral"))
      )}">${escapeHtml(manifest?.task_status ?? "unknown")}</span>
      <span class="microcopy">${escapeHtml(metadataParts.join(" · "))}</span>
    </div>
    <div class="mission-mini-grid agent-report-summary-grid">
      ${summaryCards.map(renderSelectedAgentReportSummaryCard).join("")}
    </div>
    <div class="agent-inspector-stack">
      ${renderSelectedAgentTaskContext(task, manifest)}
      ${renderSelectedAgentWorkspaceInputCard(report)}
      ${renderSelectedAgentManifestCard(report)}
    </div>
    <pre class="agent-report-pre">${escapeHtml(details)}</pre>
  `;
}

function buildSelectedAgentReportSummaryCards(manifest, task, report) {
  const workspaceArtifactId = linkedAgentArtifactIdForReport(report);
  const retryState = task?.retry_state ?? {};

  return [
    {
      kicker: "Backlog item",
      title:
        manifest?.backlog_item_id ??
        task?.backlog_item_id ??
        report.artifact.metadata?.backlog_item_id ??
        shortId(report.artifact.artifact_id),
      detail: task?.title ?? task?.kind ?? "No task title recorded",
      tone: statusTone(manifest?.task_status ?? task?.status ?? "neutral"),
    },
    {
      kicker: "Executor",
      title:
        manifest?.executor_id ??
        task?.agent_execution?.executor_id ??
        "Executor pending",
      detail:
        task?.agent_execution?.last_status ??
        manifest?.reported_status ??
        "No execution heartbeat recorded",
      tone: statusTone(manifest?.reported_status ?? task?.status ?? "neutral"),
    },
    {
      kicker: "Workspace handoff",
      title: workspaceArtifactId ? shortId(workspaceArtifactId) : "Not captured",
      detail:
        manifest?.workspace_root ??
        report.detail?.resolved_path ??
        "No workspace root recorded",
      tone: workspaceArtifactId ? "warning" : "neutral",
    },
    {
      kicker: "Retry posture",
      title: manifest?.retry_scheduled ? "Retry scheduled" : "No retry queued",
      detail: `claims ${manifest?.claim_count ?? task?.agent_execution?.claim_count ?? 0} · retry ${retryState.retry_count ?? 0}/${retryState.max_retry_count ?? 0}`,
      tone: manifest?.retry_scheduled ? "warning" : "success",
    },
  ];
}

function renderSelectedAgentReportSummaryCard(card) {
  return renderMissionMiniCard(card.kicker, card.title, card.detail, card.tone);
}

function renderSelectedAgentTaskContext(task, manifest) {
  if (!task) {
    return renderAgentInspectorCard(
      "Task context",
      "Task record is not present in the selected run snapshot",
      '<div class="empty-state compact">The persisted agent report is available, but the matching task record was not found in the current run detail payload.</div>'
    );
  }

  const lifecycleTimestamp =
    task.completed_at ?? task.lease_expires_at ?? task.started_at ?? task.created_at;

  return renderAgentInspectorCard(
    "Task context",
    task.title ?? task.backlog_item_id ?? "Untitled task",
    `
      <div class="data-card-grid">
        ${renderDataCardField(
          "Status",
          escapeHtml(displayRunStatus(task.status)),
          escapeHtml(task.kind ?? "kind unknown")
        )}
        ${renderDataCardField(
          "Assigned agent",
          escapeHtml(task.assigned_agent ?? manifest?.assigned_agent ?? "n/a"),
          escapeHtml(task.orchestrator_model ?? task.agent_execution?.mode ?? "no model hint")
        )}
        ${renderDataCardField(
          "Executor",
          escapeHtml(task.agent_execution?.executor_id ?? manifest?.executor_id ?? "unclaimed"),
          escapeHtml(task.agent_execution?.last_status ?? manifest?.reported_status ?? "no status heartbeat")
        )}
        ${renderDataCardField(
          "Retry",
          escapeHtml(`${task.retry_state?.retry_count ?? 0} / ${task.retry_state?.max_retry_count ?? 0}`),
          escapeHtml(manifest?.retry_scheduled ? "retry is currently queued" : "no retry queued")
        )}
        ${renderDataCardField(
          "Workspace root",
          escapeHtml(manifest?.workspace_root ?? "not recorded"),
          "",
          "mono"
        )}
        ${renderDataCardField(
          "Last task movement",
          escapeHtml(formatTimestamp(lifecycleTimestamp)),
          escapeHtml(task.task_id),
          "mono"
        )}
      </div>
    `
  );
}

function renderSelectedAgentWorkspaceInputCard(report) {
  const artifactId = linkedAgentArtifactIdForReport(report);
  if (!artifactId) {
    return renderAgentInspectorCard(
      "Prepared workspace",
      "No task workspace input artifact was linked",
      '<div class="empty-state compact">This report did not reference a persisted prepared workspace handoff artifact.</div>'
    );
  }

  if (linkedAgentArtifactLoadingForReport(report)) {
    return renderAgentInspectorCard(
      "Prepared workspace",
      "Loading task workspace input artifact",
      '<div class="empty-state compact is-loading"><p>Inspecting the prepared workspace handoff...</p></div>'
    );
  }

  const detail = linkedAgentArtifactDetailForReport(report);
  if (!detail || detail.error || !detail.artifact) {
    return renderAgentInspectorCard(
      "Prepared workspace",
      "Task workspace input artifact unavailable",
      renderSectionEmptyState(
        "Prepared workspace",
        "The linked task workspace input artifact could not be loaded",
        detail?.error ?? formatEnvelopeError(detail) ?? "No artifact detail payload was returned."
      )
    );
  }

  const manifest = detail.manifest ?? {};
  const directoryEntries = Array.isArray(detail.directory_entries)
    ? detail.directory_entries
    : [];
  const warnings = Array.isArray(detail.warnings)
    ? detail.warnings.filter(Boolean)
    : [];
  const summaryEntries = [];
  pushConsoleSummaryEntry(summaryEntries, "Task", manifest.task_id ?? detail.metadata?.task_id, {
    mono: true,
    short: true,
  });
  pushConsoleSummaryEntry(summaryEntries, "Source", manifest.source_kind ?? detail.metadata?.source_kind);
  pushConsoleSummaryEntry(
    summaryEntries,
    "Source artifact",
    manifest.source_artifact_id ?? detail.metadata?.source_artifact_id,
    { mono: true, short: true }
  );
  pushConsoleSummaryEntry(summaryEntries, "Files", manifest.file_count ?? detail.metadata?.file_count);
  pushConsoleSummaryEntry(
    summaryEntries,
    "Bundle entries",
    manifest.bundle_entry_count ?? detail.metadata?.bundle_entry_count
  );

  const directoryPreview = directoryEntries.length
    ? `
        <div class="agent-chip-row">
          ${directoryEntries
            .slice(0, 12)
            .map((entry) => `<span class="agent-chip mono">${escapeHtml(entry)}</span>`)
            .join("")}
        </div>
        <p class="microcopy">
          ${escapeHtml(
            directoryEntries.length > 12
              ? `${directoryEntries.length - 12} more workspace entries are available in the artifact detail.`
              : "Directory entries come from the persisted prepared task workspace artifact."
          )}
        </p>
      `
    : '<p class="microcopy">No directory entries were exposed for this workspace artifact.</p>';
  const warningsMarkup = warnings.length
    ? `
        <div class="agent-warning-row">
          ${warnings
            .map((warning) => `<span class="badge badge-warning">${escapeHtml(warning)}</span>`)
            .join("")}
        </div>
      `
    : "";

  return renderAgentInspectorCard(
    "Prepared workspace",
    detail.artifact?.artifact_type ?? "task_workspace_input",
    `
      <div class="data-card-grid">
        ${renderDataCardField(
          "Source kind",
          escapeHtml(manifest.source_kind ?? detail.metadata?.source_kind ?? "unknown"),
          escapeHtml(manifest.source_artifact_type ?? detail.metadata?.source_artifact_type ?? "no source artifact")
        )}
        ${renderDataCardField(
          "Workspace root",
          escapeHtml(manifest.workspace_root ?? detail.resolved_path ?? "not recorded"),
          "",
          "mono"
        )}
        ${renderDataCardField(
          "Bundle",
          escapeHtml(`${manifest.bundle_entry_count ?? detail.metadata?.bundle_entry_count ?? 0} entries`),
          escapeHtml(`${manifest.bundle_byte_count ?? detail.metadata?.bundle_byte_count ?? 0} bytes`)
        )}
        ${renderDataCardField(
          "Files",
          escapeHtml(String(manifest.file_count ?? detail.metadata?.file_count ?? 0)),
          escapeHtml(detail.location_exists ? "artifact path resolved" : "artifact path missing")
        )}
      </div>
      ${warningsMarkup}
      ${directoryPreview}
      ${renderConsoleStructuredPayload(manifest, summaryEntries, false)}
    `
  );
}

function renderSelectedAgentManifestCard(report) {
  const manifest = report.detail?.manifest;
  if (!manifest) {
    return renderAgentInspectorCard(
      "Report manifest",
      "Structured report manifest is still loading",
      '<div class="empty-state compact">Select another report or wait for the structured report manifest to load.</div>'
    );
  }

  const summaryEntries = [];
  pushConsoleSummaryEntry(summaryEntries, "Task", manifest.task_id, { mono: true, short: true });
  pushConsoleSummaryEntry(summaryEntries, "Backlog item", manifest.backlog_item_id);
  pushConsoleSummaryEntry(summaryEntries, "Agent", manifest.assigned_agent);
  pushConsoleSummaryEntry(summaryEntries, "Reported", manifest.reported_status);
  pushConsoleSummaryEntry(summaryEntries, "Task status", manifest.task_status);
  pushConsoleSummaryEntry(summaryEntries, "Workspace", manifest.workspace_root, {
    mono: true,
  });

  return renderAgentInspectorCard(
    "Report manifest",
    "Structured completion metadata",
    renderConsoleStructuredPayload(manifest, summaryEntries, false)
  );
}

function renderAgentInspectorCard(kicker, title, content) {
  return `
    <article class="agent-context-card">
      <div class="detail-section-head">
        <div>
          <p class="panel-kicker">${escapeHtml(kicker)}</p>
          <h3>${escapeHtml(title)}</h3>
        </div>
      </div>
      ${content}
    </article>
  `;
}

function ensureMissionGrafanaShell() {
  if (elements.missionGrafanaPanel.dataset.mode === "detail") {
    return;
  }

  setRenderedHtml(
    elements.missionGrafanaPanel,
    `
      <div class="mission-surface-layout">
        <div id="missionGrafanaLinkGrid" class="surface-link-grid"></div>
        <section class="surface-frame-shell">
          <div class="detail-section-head">
            <div>
              <p class="panel-kicker">Embedded Grafana</p>
              <h3>Provisioned overview board</h3>
            </div>
            <span id="missionGrafanaFrameBadge" class="badge badge-warning">Load on demand</span>
          </div>
          <p id="missionGrafanaFrameCopy" class="microcopy"></p>
          <div id="missionGrafanaFrameWrap" class="surface-frame-wrap"></div>
        </section>
      </div>
    `,
    { markUpdated: false }
  );
  elements.missionGrafanaPanel.dataset.mode = "detail";
}

function ensureMissionLitellmShell() {
  if (elements.missionLitellmPanel.dataset.mode === "detail") {
    return;
  }

  setRenderedHtml(
    elements.missionLitellmPanel,
    `
      <div class="mission-surface-layout">
        <div id="missionLitellmMiniGrid" class="mission-mini-grid"></div>
        <div id="missionLitellmLinkGrid" class="surface-link-grid"></div>
        <section class="surface-frame-shell">
          <div class="detail-section-head">
            <div>
              <p class="panel-kicker">Embedded LiteLLM</p>
              <h3>Native gateway surface</h3>
            </div>
            <span id="missionLitellmFrameBadge" class="badge badge-warning">Check gateway</span>
          </div>
          <p id="missionLitellmFrameCopy" class="microcopy"></p>
          <div id="missionLitellmFrameWrap" class="surface-frame-wrap"></div>
        </section>
      </div>
    `,
    { markUpdated: false }
  );
  elements.missionLitellmPanel.dataset.mode = "detail";
}

function renderMissionSurfaceEmbedPrompt(surface, title, detail, buttonLabel) {
  return `
    <div class="empty-state compact">
      <h3>${escapeHtml(title)}</h3>
      <p>${escapeHtml(detail)}</p>
      <div class="empty-state-actions">
        <button
          class="button button-primary"
          type="button"
          data-mission-surface-embed="${escapeHtml(surface)}"
        >
          ${escapeHtml(buttonLabel)}
        </button>
      </div>
    </div>
  `;
}

function missionSurfaceStatus(surfaceId) {
  return state.dashboardSnapshot.surfaces?.data?.[surfaceId] ?? {};
}

function missionSurfaceReady(surface) {
  return surface.ready === true;
}

function missionSurfaceTone(surface) {
  if (missionSurfaceReady(surface)) {
    return surface.status === "protected" ? "warning" : "success";
  }

  if (surface.http_status || surface.error) {
    return "warning";
  }

  return "neutral";
}

function missionSurfaceBadge(surface, readyLabel = "Ready") {
  if (missionSurfaceReady(surface)) {
    return surface.status === "protected" ? "Protected" : readyLabel;
  }

  if (surface.http_status) {
    return `HTTP ${surface.http_status}`;
  }

  if (surface.status === "unreachable") {
    return "Unavailable";
  }

  return surface.status ? String(surface.status) : "Unknown";
}

function missionSurfaceFailureDetail(surface, fallbackMessage) {
  if (surface.error) {
    return surface.error;
  }

  if (surface.http_status) {
    return `Probe returned HTTP ${surface.http_status}.`;
  }

  return fallbackMessage;
}

function renderMissionGrafanaPanel() {
  const grafanaBaseUrl = localServiceBaseUrl(DEFAULT_GRAFANA_PORT);
  const overviewUrl = safeExternalUrl(`${grafanaBaseUrl}${GRAFANA_OVERVIEW_DASHBOARD_PATH}`);
  const grafanaHomeUrl = safeExternalUrl(grafanaBaseUrl);
  const prometheusUrl = safeExternalUrl(localServiceBaseUrl(DEFAULT_PROMETHEUS_PORT));
  const lokiUrl = safeExternalUrl(`${localServiceBaseUrl(DEFAULT_LOKI_PORT)}/ready`);
  const tempoUrl = safeExternalUrl(`${localServiceBaseUrl(DEFAULT_TEMPO_PORT)}/ready`);
  const grafanaSurface = missionSurfaceStatus("grafana");
  const prometheusSurface = missionSurfaceStatus("prometheus");
  const lokiSurface = missionSurfaceStatus("loki");
  const tempoSurface = missionSurfaceStatus("tempo");
  ensureMissionGrafanaShell();

  setRenderedHtml(
    document.getElementById("missionGrafanaLinkGrid"),
    `
      ${renderSurfaceLinkCard(
        "Grafana dashboard",
        "Catalyst Continuum Overview",
        missionSurfaceReady(grafanaSurface)
          ? "Use the provisioned dashboard for stack health, orchestration throughput, and gateway signals."
          : `Grafana is not ready yet. ${missionSurfaceFailureDetail(
              grafanaSurface,
              "Start the local observability stack to expose the overview dashboard."
            )}`,
        [
          { href: overviewUrl, label: "Open dashboard", variant: "primary" },
          { href: grafanaHomeUrl, label: "Open Grafana", variant: "ghost" },
        ],
        missionSurfaceTone(grafanaSurface)
      )}
      ${renderSurfaceLinkCard(
        "Metrics",
        "Prometheus",
        missionSurfaceReady(prometheusSurface)
          ? "Jump into raw metric queries when the dashboard summary is not enough."
          : `Prometheus is not ready yet. ${missionSurfaceFailureDetail(
              prometheusSurface,
              "Start the local observability stack to expose metric queries."
            )}`,
        [{ href: prometheusUrl, label: "Open Prometheus", variant: "ghost" }],
        missionSurfaceTone(prometheusSurface)
      )}
      ${renderSurfaceLinkCard(
        "Logs",
        "Loki",
        missionSurfaceReady(lokiSurface)
          ? "The compose stack sends orchestrator and LiteLLM logs into Loki for deeper inspection."
          : `Loki is not ready yet. ${missionSurfaceFailureDetail(
              lokiSurface,
              "Start the local observability stack to inspect collected logs."
            )}`,
        [{ href: lokiUrl, label: "Open Loki readiness", variant: "ghost" }],
        missionSurfaceTone(lokiSurface)
      )}
      ${renderSurfaceLinkCard(
        "Traces",
        "Tempo",
        missionSurfaceReady(tempoSurface)
          ? "Tempo keeps the OTLP traces used by the provisioned overview dashboard and future deeper debugging flows."
          : `Tempo is not ready yet. ${missionSurfaceFailureDetail(
              tempoSurface,
              "Start the local observability stack to inspect OTLP traces."
            )}`,
        [{ href: tempoUrl, label: "Open Tempo readiness", variant: "ghost" }],
        missionSurfaceTone(tempoSurface)
      )}
    `,
    { markUpdated: false }
  );

  const grafanaBadge = document.getElementById("missionGrafanaFrameBadge");
  const grafanaCopy = document.getElementById("missionGrafanaFrameCopy");
  const grafanaFrameWrap = document.getElementById("missionGrafanaFrameWrap");
  const embedLoaded = state.missionSurfaceEmbeds.grafana === true;

  if (!overviewUrl) {
    setBadge(grafanaBadge, "warning", "URL unavailable");
    setTextContent(
      grafanaCopy,
      "Use the quick links above to open Grafana once the local compose stack is running.",
      { markUpdated: false }
    );
    setRenderedHtml(
      grafanaFrameWrap,
      renderSectionEmptyState(
        "Embedded Grafana",
        "Grafana URL is unavailable",
        "Use the quick links above once the local compose stack is reachable."
      ),
      { markUpdated: false }
    );
    return;
  }

  if (embedLoaded) {
    setBadge(
      grafanaBadge,
      missionSurfaceReady(grafanaSurface) ? "success" : "warning",
      "Embed loaded"
    );
    setTextContent(
      grafanaCopy,
      missionSurfaceReady(grafanaSurface)
        ? "The embedded dashboard stays mounted while the rest of the operator UI keeps refreshing around it."
        : `The embedded dashboard stays mounted, but the latest Grafana probe is degraded. ${missionSurfaceFailureDetail(
            grafanaSurface,
            "Check the local observability stack."
          )}`,
      { markUpdated: false }
    );
    setRenderedHtml(
      grafanaFrameWrap,
      `<iframe class="surface-frame" title="Embedded Grafana dashboard" src="${escapeHtml(
        overviewUrl
      )}" loading="lazy"></iframe>`,
      { markUpdated: false }
    );
    return;
  }

  if (!missionSurfaceReady(grafanaSurface)) {
    setBadge(grafanaBadge, missionSurfaceTone(grafanaSurface), missionSurfaceBadge(grafanaSurface));
    setTextContent(
      grafanaCopy,
      "Grafana has to be reachable before the in-page dashboard is worth mounting.",
      { markUpdated: false }
    );
    setRenderedHtml(
      grafanaFrameWrap,
      renderSectionEmptyState(
        "Embedded Grafana",
        "Grafana is not ready yet",
        missionSurfaceFailureDetail(
          grafanaSurface,
          "Start the local observability stack, then reload this tab."
        )
      ),
      { markUpdated: false }
    );
    return;
  }

  setBadge(grafanaBadge, "success", "Ready to load");
  setTextContent(
    grafanaCopy,
    "Load the embedded dashboard only when you want it in-page. This keeps live operator refreshes from remounting the frame.",
    { markUpdated: false }
  );
  setRenderedHtml(
    grafanaFrameWrap,
    renderMissionSurfaceEmbedPrompt(
      "grafana",
      "Load embedded Grafana when you need it",
      "The quick links stay available all the time, while the iframe mounts only on demand to avoid unnecessary frame resets.",
      "Load embedded Grafana"
    ),
    { markUpdated: false }
  );
}

function renderMissionLitellmPanel() {
  const aiGateway = state.dashboardSnapshot.aiGateway?.data ?? {};
  const gatewayConfig = state.dashboardSnapshot.config?.data?.ai_gateway ?? {};
  const litellmBaseUrl = serviceBaseUrlFromConfig(
    aiGateway.host_base_url ?? gatewayConfig.host_base_url,
    DEFAULT_LITELLM_PORT
  );
  const litellmHomeUrl = safeExternalUrl(litellmBaseUrl);
  const litellmUiUrl = safeExternalUrl(`${litellmBaseUrl}/ui`);
  const litellmDocsUrl = safeExternalUrl(`${litellmBaseUrl}/docs`);
  const litellmModelsUrl = safeExternalUrl(`${litellmBaseUrl}/v1/models`);
  const litellmUiSurface = missionSurfaceStatus("litellm_ui");
  const capabilities = Array.isArray(gatewayConfig.capabilities)
    ? gatewayConfig.capabilities.filter((capability) => capability.enabled)
    : [];
  ensureMissionLitellmShell();

  setRenderedHtml(
    document.getElementById("missionLitellmMiniGrid"),
    `
      ${renderMissionMiniCard(
        "Gateway status",
        aiGateway.ready ? "Ready" : aiGateway.status ?? "Unavailable",
        aiGateway.error ?? `${aiGateway.available_model_count ?? 0} model(s) visible`,
        aiGateway.ready ? "success" : statusTone(aiGateway.status)
      )}
      ${renderMissionMiniCard(
        "Default alias",
        aiGateway.current_host_default_model_alias ?? gatewayConfig.default_model_aliases?.other_platforms ?? "unknown",
        gatewayConfig.provider ?? "litellm",
        "neutral"
      )}
      ${renderMissionMiniCard(
        "Capabilities",
        `${capabilities.length} enabled`,
        summarizeValues(
          capabilities.map((capability) => capability.capability),
          "No enabled capabilities declared"
        ),
        "warning"
      )}
      ${renderMissionMiniCard(
        "Host base URL",
        litellmBaseUrl,
        gatewayConfig.container_base_url ?? "No container base URL recorded",
        "neutral"
      )}
    `,
    { markUpdated: false }
  );
  setRenderedHtml(
    document.getElementById("missionLitellmLinkGrid"),
    `
      ${renderSurfaceLinkCard(
        "Native LiteLLM UI",
        "Gateway dashboard",
        missionSurfaceReady(litellmUiSurface)
          ? "The upstream LiteLLM image advertises an admin dashboard UI for monitoring and management."
          : `The native LiteLLM UI is not ready yet. ${missionSurfaceFailureDetail(
              litellmUiSurface,
              "Use docs or the root surface until the UI route is reachable."
            )}`,
        [
          { href: litellmUiUrl, label: "Open native UI", variant: "primary" },
          { href: litellmHomeUrl, label: "Open root", variant: "ghost" },
        ],
        missionSurfaceTone(litellmUiSurface)
      )}
      ${renderSurfaceLinkCard(
        "API docs",
        "Swagger / docs surface",
        "Use the native docs when you need the exact proxy endpoints rather than the condensed operator summary.",
        [{ href: litellmDocsUrl, label: "Open docs", variant: "ghost" }],
        "neutral"
      )}
      ${renderSurfaceLinkCard(
        "Model catalog",
        "Visible aliases",
        "This is the same gateway model list the orchestrator probes for live status and default alias validation.",
        [{ href: litellmModelsUrl, label: "Open /v1/models", variant: "ghost" }],
        "neutral"
      )}
    `,
    { markUpdated: false }
  );

  const litellmBadge = document.getElementById("missionLitellmFrameBadge");
  const litellmCopy = document.getElementById("missionLitellmFrameCopy");
  const litellmFrameWrap = document.getElementById("missionLitellmFrameWrap");
  const embedLoaded = state.missionSurfaceEmbeds.litellm === true;

  if (!litellmUiUrl) {
    setBadge(litellmBadge, "warning", "URL unavailable");
    setTextContent(
      litellmCopy,
      "Use the quick links above once the local gateway is configured with a reachable base URL.",
      { markUpdated: false }
    );
    setRenderedHtml(
      litellmFrameWrap,
      renderSectionEmptyState(
        "Embedded LiteLLM",
        "LiteLLM URL is unavailable",
        "Use the quick links above once the local gateway is running."
      ),
      { markUpdated: false }
    );
    return;
  }

  if (embedLoaded) {
    setBadge(
      litellmBadge,
      missionSurfaceReady(litellmUiSurface) ? "success" : "warning",
      "Embed loaded"
    );
    setTextContent(
      litellmCopy,
      missionSurfaceReady(litellmUiSurface)
        ? "The embedded gateway surface stays mounted while the rest of the operator UI keeps refreshing around it."
        : `The embedded gateway surface stays mounted, but the latest UI probe is degraded. ${missionSurfaceFailureDetail(
            litellmUiSurface,
            "Fall back to the docs or root surface if the native UI stops responding."
          )}`,
      { markUpdated: false }
    );
    setRenderedHtml(
      litellmFrameWrap,
      `<iframe class="surface-frame" title="Embedded LiteLLM UI" src="${escapeHtml(
        litellmUiUrl
      )}" loading="lazy"></iframe>`,
      { markUpdated: false }
    );
    return;
  }

  if (missionSurfaceReady(litellmUiSurface)) {
    setBadge(litellmBadge, "success", "Ready to load");
    setTextContent(
      litellmCopy,
      "Load the embedded LiteLLM surface only when you need it in-page. This keeps live operator refreshes from remounting the frame.",
      { markUpdated: false }
    );
    setRenderedHtml(
      litellmFrameWrap,
      renderMissionSurfaceEmbedPrompt(
        "litellm",
        "Load embedded LiteLLM when you need it",
        "Keep the native gateway UI in one place without mounting the iframe during every control-plane refresh.",
        "Load embedded LiteLLM"
      ),
      { markUpdated: false }
    );
    return;
  }

  setBadge(
    litellmBadge,
    missionSurfaceTone(litellmUiSurface),
    aiGateway.ready ? missionSurfaceBadge(litellmUiSurface, "UI unavailable") : "Check gateway"
  );
  setTextContent(
    litellmCopy,
    aiGateway.ready
      ? "The AI gateway is alive, but the native LiteLLM UI route is not healthy yet."
      : "If the native UI route is unavailable in the selected LiteLLM build, use the quick links above to fall back to the docs or root surface.",
    { markUpdated: false }
  );
  setRenderedHtml(
    litellmFrameWrap,
    renderSectionEmptyState(
      "Embedded LiteLLM",
      aiGateway.ready ? "LiteLLM UI is not ready yet" : "Gateway readiness is not green yet",
      aiGateway.ready
        ? missionSurfaceFailureDetail(
            litellmUiSurface,
            "Use the docs or root surface until the native UI route becomes reachable."
          )
        : aiGateway.error ??
          "Wait for the AI gateway probe to recover before mounting the embedded LiteLLM surface."
    ),
    { markUpdated: false }
  );
}

function renderSurfaceLinkCard(kicker, title, detail, links, tone) {
  return `
    <article class="surface-link-card">
      <div class="surface-link-head">
        <div>
          <p class="panel-kicker">${escapeHtml(kicker)}</p>
          <h3>${escapeHtml(title)}</h3>
        </div>
        <span class="badge badge-${escapeHtml(normalizePulseTone(tone))}">${escapeHtml(
          toneLabel(normalizePulseTone(tone))
        )}</span>
      </div>
      <p>${escapeHtml(detail)}</p>
      <div class="surface-link-actions">
        ${links
          .map((link) => renderSurfaceLink(link.href, link.label, link.variant))
          .join("")}
      </div>
    </article>
  `;
}

function renderSurfaceLink(href, label, variant) {
  const safeHref = safeExternalUrl(href);
  if (!safeHref) {
    return "";
  }

  return `
    <a
      class="button ${escapeHtml(variant === "ghost" ? "button-ghost" : "button-primary")} button-link"
      href="${escapeHtml(safeHref)}"
      target="_blank"
      rel="noreferrer noopener"
    >
      ${escapeHtml(label)}
    </a>
  `;
}

function localServiceBaseUrl(port) {
  const protocol = window.location.protocol === "https:" ? "https:" : "http:";
  const host = window.location.hostname || "127.0.0.1";
  return `${protocol}//${host}:${port}`;
}

function serviceBaseUrlFromConfig(candidate, fallbackPort) {
  const safeCandidate = safeExternalUrl(candidate);
  if (!safeCandidate) {
    return localServiceBaseUrl(fallbackPort);
  }

  try {
    const parsed = new URL(safeCandidate);
    return localServiceBaseUrl(parsed.port || fallbackPort);
  } catch (_error) {
    return localServiceBaseUrl(fallbackPort);
  }
}

function truncateText(value, maxLength) {
  const rendered = String(value ?? "").trim();
  if (rendered.length <= maxLength) {
    return rendered;
  }

  return `${rendered.slice(0, maxLength - 1)}…`;
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

function repositoryTargetCapabilityCard(repositoryTargets) {
  const targets = Array.isArray(repositoryTargets.targets)
    ? repositoryTargets.targets
    : [];
  const enabledTargets = targets.filter((target) => target.enabled);
  const enforcementEnabled = repositoryTargets.enforcement_enabled === true;

  if (!enforcementEnabled) {
    return {
      title: "Real repository guard",
      statusClass: "warning",
      badge: "unrestricted",
      primary: "Real-repo allowlist is not active",
      secondary:
        "Smoke and local dev can still use explicit remotes, but real GitHub publication should configure repository targets first.",
      detail:
        "Run make repository-targets-bootstrap REPOSITORY=<OWNER>/<REPO> REPOSITORY_TARGET_ID=<TARGET_ID>, then export CATALYST_REPOSITORY_TARGETS_FILE before real publication.",
    };
  }

  return {
    title: "Real repository guard",
    statusClass: enabledTargets.length ? "success" : "error",
    badge: `${enabledTargets.length} target(s)`,
    primary: enabledTargets.length
      ? "Draft PRs are scoped to configured repositories"
      : "Repository publication is denied",
    secondary: summarizeRepositoryTargets(repositoryTargets),
    detail: `Config ${friendlySourcePath(repositoryTargets.source_path, "not configured")}`,
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

function summarizeRepositoryTargets(repositoryTargets) {
  const targets = Array.isArray(repositoryTargets.targets)
    ? repositoryTargets.targets
    : [];
  const enabledTargets = targets.filter((target) => target.enabled);

  if (!repositoryTargets.enforcement_enabled) {
    return "No repository-target config is active; use this only for smoke/dev or explicitly trusted one-off runs.";
  }

  return summarizeValues(
    enabledTargets.map(
      (target) =>
        `${target.target_id} -> ${target.owner}/${target.name}:${target.default_branch}`
    ),
    "No enabled repository targets",
    ", "
  );
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
    setRenderedHtml(elements.packChips, '<span class="chip">No pack catalog</span>');
    return;
  }

  setRenderedHtml(
    elements.packChips,
    packs.items
      .slice(0, 4)
      .map(
        (item) =>
          `<span class="chip">${escapeHtml(item.pack_id)} · ${escapeHtml(
            item.agent_profile?.default_agent ?? "no-default-agent"
          )}</span>`
      )
      .join("")
  );
}

function renderBriefExamples() {
  if (!state.briefExamples.length) {
    setRenderedHtml(elements.briefExamples, renderSectionEmptyState(
      "Starter briefs",
      "No curated starters configured",
      "Paste YAML manually or use files from examples/briefs when this instance does not expose starter scenarios."
    ));
    renderBriefExamplesError("No curated starters are configured for this instance.");
    return;
  }

  setRenderedHtml(
    elements.briefExamples,
    state.briefExamples
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
      .join("")
  );
  renderBriefExampleHint();
}

function renderBriefExamplesError(message) {
  setRenderedHtml(elements.briefExamples, renderSectionEmptyState(
    "Starter briefs",
    "Starter scenarios unavailable",
    "Paste YAML manually or use the repository examples until this instance can serve curated starter briefs."
  ));
  setTextContent(
    elements.briefExampleHint,
    `${message} Paste YAML manually or use the repo examples/briefs files directly.`
  );
}

function renderBriefExampleHint(activeExample) {
  if (activeExample) {
    setTextContent(
      elements.briefExampleHint,
      `Loaded ${activeExample.label} from ${activeExample.source_path}. Review repository owner/name and requested_by before validation or submission.`
    );
    return;
  }

  if (!state.briefExamples.length) {
    setTextContent(elements.briefExampleHint, "Loading curated starters...");
    return;
  }

  setTextContent(
    elements.briefExampleHint,
    "Choose a starter scenario, load it into the editor, then adjust repository metadata before validation and submission."
  );
}

function renderBriefReadiness() {
  const items = buildBriefReadinessItems(elements.briefEditor.value);
  const readyCount = items.filter((item) => item.ready).length;
  const tone = readyCount === items.length ? "success" : readyCount > 0 ? "warning" : "neutral";

  setBadge(elements.briefReadinessBadge, tone, `${readyCount}/${items.length} ready`);
  setRenderedHtml(
    elements.briefReadinessList,
    items.map(renderBriefReadinessItem).join("")
  );
}

function buildBriefReadinessItems(value) {
  const content = value.trim();
  const repositoryBlock = yamlTopLevelBlock(content, "repository");
  const executionBlock = yamlTopLevelBlock(content, "execution_preferences");

  return [
    briefReadinessItem({
      detail: "Needed so the run is auditable and traceable back to a requester.",
      fields: ["schema_version", "brief_id", "title", "requested_by"],
      label: "Brief identity",
      ready: hasYamlFields(content, ["schema_version", "brief_id", "title", "requested_by"]),
    }),
    briefReadinessItem({
      detail: "Goals, requirements, and deliverables tell the planner what to materialize.",
      fields: ["goals", "functional_requirements", "deliverables"],
      label: "Delivery scope",
      ready: hasYamlFields(content, ["goals", "functional_requirements", "deliverables"]),
    }),
    briefReadinessItem({
      detail: "Repository metadata decides where PR handoff can be exported or published.",
      fields: ["repository.host", "repository.owner", "repository.name", "repository.default_branch"],
      label: "Repository target",
      ready:
        hasYamlFields(repositoryBlock, ["host", "owner", "name", "default_branch"]) &&
        hasYamlField(content, "repository"),
    }),
    briefReadinessItem({
      detail: "Pack, runtime, sandbox, and default agent keep execution policy explicit.",
      fields: [
        "execution_preferences.repo_pack",
        "execution_preferences.default_runtime_provider",
        "execution_preferences.sandbox_profile",
        "execution_preferences.default_agent",
      ],
      label: "Execution policy",
      ready:
        hasYamlFields(executionBlock, [
          "repo_pack",
          "default_runtime_provider",
          "sandbox_profile",
          "default_agent",
        ]) && hasYamlField(content, "execution_preferences"),
    }),
  ];
}

function briefReadinessItem({ detail, fields, label, ready }) {
  return {
    detail,
    fields,
    label,
    ready,
    status: ready ? "Ready" : "Needs input",
    tone: ready ? "success" : "neutral",
  };
}

function renderBriefReadinessItem(item) {
  const tone = normalizePulseTone(item.tone);

  return `
    <article class="brief-readiness-card brief-readiness-card-${escapeHtml(tone)}" data-brief-readiness-item="true">
      <div class="mission-feed-head">
        <p class="panel-kicker">${escapeHtml(item.label)}</p>
        <span class="badge badge-${escapeHtml(tone)}">${escapeHtml(item.status)}</span>
      </div>
      <p>${escapeHtml(item.detail)}</p>
      <p class="microcopy">${escapeHtml(item.fields.join(", "))}</p>
    </article>
  `;
}

function hasYamlFields(content, fields) {
  return fields.every((field) => hasYamlField(content, field));
}

function hasYamlField(content, field) {
  if (!content.trim()) {
    return false;
  }

  return new RegExp(`(^|\\n)\\s*${field}:`, "m").test(content);
}

function yamlTopLevelBlock(content, field) {
  if (!content.trim()) {
    return "";
  }

  const lines = content.split(/\r?\n/);
  const startIndex = lines.findIndex((line) =>
    new RegExp(`^${field}:\\s*(?:#.*)?$`).test(line)
  );
  if (startIndex === -1) {
    return "";
  }

  const block = [];
  for (let index = startIndex + 1; index < lines.length; index += 1) {
    const line = lines[index];
    if (/^\S/.test(line) && line.trim()) {
      break;
    }
    block.push(line);
  }

  return block.join("\n");
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
  renderBriefReadiness();
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
    setRenderedHtml(
      elements.runsList,
      state.selectedRunStatus
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
          )
    );
    return;
  }

  if (visibleRuns.length === 0) {
    setRenderedHtml(
      elements.runsList,
      loadingOrEmptyState(
        state.runSearchQuery
          ? "No loaded runs match the current search. Clear search or refresh if you expect a newer run."
          : "No runs match the current filter."
      )
    );
    return;
  }

  setRenderedHtml(
    elements.runsList,
    visibleRuns
      .map((run) => {
        const repositoryName = run.repository?.owner && run.repository?.name
          ? `${run.repository.owner}/${run.repository.name}`
          : "repository not declared";
        const isSelected = run.run_id === state.selectedRunId;
        const guide = runCardGuide(run);
        return `
          <button
            class="run-card${isSelected ? " is-selected" : ""}"
            type="button"
            data-run-id="${escapeHtml(run.run_id)}"
            data-ui-stable-key="run-card:${escapeHtml(run.run_id)}"
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
            <div class="run-card-guide">
              <div class="run-card-guide-head">
                <p class="panel-kicker">Current stage</p>
                <span class="badge badge-${escapeHtml(guide.tone)}">${escapeHtml(guide.stage)}</span>
              </div>
              <p>${escapeHtml(guide.detail)}</p>
              <div class="run-card-next-step" data-run-card-next-step="true">
                <span>Next step</span>
                <strong>${escapeHtml(guide.nextStep)}</strong>
              </div>
            </div>
            ${renderRunCardPhaseRail(run)}
            ${renderRunCardTaskMeter(run.task_counts)}
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
      .join("")
  );
  syncRunCardTaskMeters();
  renderOperatorPulse();
}

function renderRunCardPhaseRail(run) {
  const counts = normalizedTaskCounts(run.task_counts);
  const artifactCount = Number(run.artifact_count ?? 0);
  const hasFailures = run.status === "failed" || counts.failed > 0;
  const hasActiveTasks = counts.running > 0 || counts.queued > 0;
  const tasksComplete = counts.total > 0 && !hasActiveTasks && counts.failed === 0;

  let taskPhase = {
    state: "pending",
    status: "Pending",
    detail: "No tasks yet",
  };
  if (hasFailures) {
    taskPhase = {
      state: "blocked",
      status: "Blocked",
      detail: `${counts.failed} failed`,
    };
  } else if (counts.running > 0) {
    taskPhase = {
      state: "active",
      status: "Running",
      detail: `${counts.running} active`,
    };
  } else if (counts.queued > 0) {
    taskPhase = {
      state: "active",
      status: "Ready",
      detail: `${counts.queued} queued`,
    };
  } else if (tasksComplete) {
    taskPhase = {
      state: "done",
      status: "Done",
      detail: `${counts.succeeded}/${counts.total} done`,
    };
  }

  let evidencePhase = {
    state: "pending",
    status: "Pending",
    detail: "Awaiting artifacts",
  };
  if (artifactCount > 0) {
    evidencePhase = {
      state: "done",
      status: "Captured",
      detail: `${artifactCount} artifact(s)`,
    };
  } else if (hasFailures) {
    evidencePhase = {
      state: "blocked",
      status: "Blocked",
      detail: "Fix tasks first",
    };
  } else if (tasksComplete) {
    evidencePhase = {
      state: "active",
      status: "Needed",
      detail: "Evaluate quality",
    };
  }

  let handoffPhase = {
    state: "pending",
    status: "Pending",
    detail: "Not ready yet",
  };
  if (hasFailures) {
    handoffPhase = {
      state: "blocked",
      status: "Blocked",
      detail: "Needs recovery",
    };
  } else if (run.status === "succeeded" && artifactCount > 0) {
    handoffPhase = {
      state: "active",
      status: "Ready",
      detail: "Verify and publish",
    };
  } else if (tasksComplete && artifactCount > 0) {
    handoffPhase = {
      state: "active",
      status: "Review",
      detail: "Check quality",
    };
  }

  const phases = [
    {
      label: "Brief",
      state: "done",
      status: "Done",
      detail: "Run created",
    },
    {
      label: "Tasks",
      ...taskPhase,
    },
    {
      label: "Evidence",
      ...evidencePhase,
    },
    {
      label: "PR handoff",
      ...handoffPhase,
    },
  ];
  const summary = phases
    .map((phase) => `${phase.label}: ${phase.status}`)
    .join(", ");

  return `
    <div
      class="run-card-phase-rail"
      data-run-card-phase-rail="true"
      aria-label="${escapeHtml(`Run phase rail: ${summary}`)}"
    >
      ${phases.map(renderRunCardPhaseStep).join("")}
    </div>
  `;
}

function renderRunCardPhaseStep(phase) {
  return `
    <span
      class="run-card-phase-step run-card-phase-${escapeHtml(phase.state)}"
      data-run-card-phase-step="true"
      data-run-card-phase-state="${escapeHtml(phase.state)}"
    >
      <span class="run-card-phase-label">${escapeHtml(phase.label)}</span>
      <strong>${escapeHtml(phase.status)}</strong>
      <span>${escapeHtml(phase.detail)}</span>
    </span>
  `;
}

function renderRunCardTaskMeter(taskCounts) {
  const counts = normalizedTaskCounts(taskCounts);
  const total = Math.max(counts.total, 0);
  const denominator = total > 0 ? total : 1;
  const segments = [
    { key: "succeeded", label: "ok", count: counts.succeeded },
    { key: "running", label: "running", count: counts.running },
    { key: "failed", label: "failed", count: counts.failed },
    { key: "queued", label: "queued", count: counts.queued },
  ].filter((segment) => segment.count > 0);
  const summary = total > 0
    ? `${counts.succeeded}/${total} done · ${counts.running} running · ${counts.failed} failed`
    : "No tasks materialized yet";

  return `
    <div
      class="run-card-task-meter"
      data-run-card-task-meter="true"
      aria-label="${escapeHtml(`Task progress: ${summary}`)}"
    >
      <div class="run-card-task-meter-head">
        <span>Task progress</span>
        <strong>${escapeHtml(summary)}</strong>
      </div>
      <div class="run-card-task-meter-track">
        ${
          segments.length
            ? segments
                .map((segment) => renderRunCardTaskMeterSegment(segment, denominator))
                .join("")
            : '<span class="run-card-task-meter-empty"></span>'
        }
      </div>
    </div>
  `;
}

function renderRunCardTaskMeterSegment(segment, denominator) {
  const width = Math.max(4, Math.round((segment.count / denominator) * 100));
  return `
    <span
      class="run-card-task-meter-segment run-card-task-meter-${escapeHtml(segment.key)}"
      title="${escapeHtml(`${segment.count} ${segment.label}`)}"
      data-run-card-task-meter-segment="${escapeHtml(segment.key)}"
      data-run-card-task-meter-width="${escapeHtml(width)}"
    ></span>
  `;
}

function syncRunCardTaskMeters() {
  document.querySelectorAll("[data-run-card-task-meter-width]").forEach((segment) => {
    const width = Number(segment.dataset.runCardTaskMeterWidth ?? 0);
    const safeWidth = Number.isFinite(width) ? Math.min(100, Math.max(0, width)) : 0;
    segment.style.width = `${safeWidth}%`;
  });
}

function runCardGuide(run) {
  const taskCounts = normalizedTaskCounts(run.task_counts);

  if (run.status === "failed") {
    return {
      tone: "error",
      stage: "Blocked",
      detail: "Open this run and inspect failed tasks plus run events before continuing promotion or retries.",
      nextStep: "Inspect failure evidence",
    };
  }

  if (run.status === "executing" || taskCounts.running > 0) {
    return {
      tone: "warning",
      stage: "Executing",
      detail:
        taskCounts.running > 0
          ? `${taskCounts.running} task(s) are running right now. Open the run to watch progress and next actions.`
          : "Execution is in progress. Open the run to inspect the active stage and remaining backlog.",
      nextStep: "Watch agent progress",
    };
  }

  if (run.status === "queued" || taskCounts.queued > 0) {
    return {
      tone: "warning",
      stage: "Ready",
      detail:
        taskCounts.queued > 0
          ? `${taskCounts.queued} queued task(s) are waiting. Open the run and use Run next task or Worker once.`
          : "The run is ready to start execution. Open it and trigger the first controlled step.",
      nextStep: "Start controlled execution",
    };
  }

  if (run.status === "succeeded") {
    return {
      tone: "success",
      stage: "Promotable",
      detail:
        run.artifact_count > 0
          ? "Execution finished. Open the run to evaluate quality or continue PR export and draft-PR handoff."
          : "Execution finished. Open the run to inspect promotion readiness and persisted outputs.",
      nextStep: "Verify quality and handoff",
    };
  }

  return {
    tone: "neutral",
    stage: "Materialized",
    detail: "Open this run to inspect the current orchestration stage, backlog, and next operator action.",
    nextStep: "Open selected-run guide",
  };
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
    setTextContent(
      elements.runLedgerHint,
      "Search the loaded run ledger by title, repository, pack, trigger, or run id."
    );
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

  setTextContent(elements.runLedgerHint, fragments.join(" "));
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
  renderOperatorPulse();
}

function renderAutomationRailCollections() {
  setTextContent(elements.webhookActionCount, String(state.latestWebhookActions.length));
  setTextContent(elements.signalCount, String(state.latestRepositorySignals.length));
  setTextContent(elements.deliveryCount, String(state.latestWebhookDeliveries.length));

  setRenderedHtml(elements.webhookActionsList, renderRailItems(
    state.latestWebhookActions,
    "webhook_action",
    (item) => item.request_id,
    (item) => item.action,
    (item) => `${item.status} · ${item.repository_full_name ?? item.delivery_id}`,
    (item) => item.updated_at ?? item.created_at,
    "No pending webhook actions. Signed GitHub deliveries will queue control-plane work here before a run exists."
  ));
  setRenderedHtml(elements.repositorySignalsList, renderRailItems(
    state.latestRepositorySignals,
    "repository_signal",
    (item) => item.signal_id,
    (item) => item.signal_kind,
    (item) => `${item.status} · ${item.repository_full_name}`,
    (item) => item.updated_at ?? item.created_at,
    "No repository signals yet. Once a routed webhook action succeeds, the automation handoff will appear here."
  ));
  setRenderedHtml(elements.webhookDeliveriesList, renderRailItems(
    state.latestWebhookDeliveries,
    "webhook_delivery",
    (item) => item.delivery_id,
    (item) => item.event,
    (item) => `${item.routing_status} · ${item.repository_full_name ?? item.delivery_id}`,
    (item) => item.updated_at ?? item.created_at,
    "No signed webhook deliveries are loaded yet. Incoming GitHub App traffic will appear here for inspection."
  ));
  syncAutomationDisclosures();
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
          data-ui-stable-key="queue:${escapeHtml(queueKind)}:${escapeHtml(itemId)}"
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
  openAutomationDisclosure(elements.queueInspectorDisclosure);
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
  setTextContent(
    elements.queueInspectorHeadline,
    `Loading ${queueInspectorKindLabel(queueKind)} ${shortId(queueId)}...`
  );
  setRenderedHtml(
    elements.queueInspectorSummary,
    '<div class="empty-state compact">Loading queue detail...</div>'
  );
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

  setTextContent(
    elements.queueInspectorHeadline,
    `${queueInspectorKindTitle(queueKind)} · ${shortId(queueId)}`
  );
  setRenderedHtml(elements.queueInspectorSummary, renderQueueInspectorSummary(
    queueKind,
    queueId,
    primaryEnvelope
  ));
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
  const publicationGuard = repositoryTargetGuardSummary(runDetail);
  state.selectedRunDetail = runDetail;
  state.selectedRunEvents = events;
  syncSelectedAgentActivity(runDetail);
  elements.detailEmptyState.classList.add("hidden");
  elements.runDetailShell.classList.remove("hidden");
  setTextContent(elements.selectedRunLabel, `${runDetail.title} · ${shortId(runDetail.run_id)}`);

  setRenderedHtml(elements.runSummaryCards, [
    summaryCard("Status", displayRunStatus(runDetail.status), `${runDetail.trigger} trigger`),
    summaryCard("Pack", runDetail.target_pack ?? "unassigned", runDetail.requested_by ?? "requested_by unknown"),
    summaryCard(
      "Repository",
      repositoryLabel(runDetail)
        ? repositoryLabel(runDetail)
        : "No repository target",
      runDetail.repository?.default_branch ?? "default branch unknown"
    ),
    summaryCard("Publication guard", publicationGuard.title, publicationGuard.detail),
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
  ].join(""));

  const guide = renderRunGuide(runDetail, events);
  renderRunOutcomeBanner(runDetail, guide, events);
  renderRunSectionNav(runDetail, guide, events);
  renderRunControlReadinessBoard(runDetail, guide);
  renderTasks(runDetail.tasks || []);
  renderArtifacts(runDetail);
  renderEvents(events);
  renderRunActionHighlights(runDetail);
  syncRunActionControlsWithState();
  renderOperatorPulse();
  renderMissionControl();
  ensureAgentReportDetails(runDetail);
}

function renderRunActionHighlights(runDetail) {
  if (!runDetail?.run_id) {
    setTextContent(
      elements.actionSummaryHeadline,
      "Run a control to surface branch, PR, quality, and task execution details here."
    );
    setRenderedHtml(
      elements.actionHighlights,
      '<div class="empty-state compact">No run action summary captured for this run yet.</div>'
    );
    return;
  }

  const actionResult = state.runActionResults[runDetail.run_id];
  if (!actionResult) {
    setTextContent(
      elements.actionSummaryHeadline,
      "Run a control to surface branch, PR, quality, and task execution details here."
    );
    setRenderedHtml(
      elements.actionHighlights,
      '<div class="empty-state compact">No run action summary captured for this run yet.</div>'
    );
    return;
  }

  const presentation = envelopeBadgePresentation(actionResult.envelope);
  setTextContent(
    elements.actionSummaryHeadline,
    `Last action: ${displayRunActionLabel(actionResult.actionId)} · ${presentation.label}.`
  );

  const items = actionHighlightItems(actionResult.actionId, actionResult.envelope);
  setRenderedHtml(
    elements.actionHighlights,
    items.length
      ? items.map(renderActionSummaryCard).join("")
      : '<div class="empty-state compact">The latest action returned no structured summary fields.</div>'
  );
}

function renderRunGuide(runDetail, events) {
  const guide = buildRunGuide(runDetail, events);

  setBadge(elements.runGuideBadge, guide.badgeTone, guide.badgeLabel);
  setTextContent(elements.runGuideHeadline, guide.headline);
  setTextContent(elements.runGuideNextAction, guide.nextActionTitle);
  setTextContent(elements.runGuideRecommendation, guide.nextActionDetail);
  setTextContent(elements.runGuideCurrentStage, guide.currentStageTitle);
  setTextContent(elements.runGuideCurrentStageDetail, guide.currentStageDetail);
  setTextContent(elements.runGuideProgressSummary, guide.progressSummary);
  syncRunGuideProgressMeter(guide);
  setTextContent(elements.runGuideBlockers, guide.blockerDetail);
  renderRunGuideAction(runDetail, guide);
  setRenderedHtml(
    elements.runGuideStages,
    guide.stages.length
      ? guide.stages.map(renderGuideStage).join("")
      : '<div class="empty-state compact">No run-stage guidance is available for this run yet.</div>'
  );
  return guide;
}

function renderRunOutcomeBanner(runDetail, guide, events) {
  if (!runDetail) {
    setRenderedHtml(
      elements.runOutcomeBanner,
      '<div class="empty-state compact">Select a run to see the delivery outcome, proof, and safest next action.</div>',
      { markUpdated: false }
    );
    return;
  }

  const taskCounts = normalizedTaskCounts(runDetail.task_counts);
  const artifactCount = Number(runDetail.artifact_count ?? runDetail.artifacts?.length ?? 0);
  const eventCount = Array.isArray(events) ? events.length : 0;
  const publicationGuard = repositoryTargetGuardSummary(runDetail);
  const tone = normalizePulseTone(guide.badgeTone);
  const actionMarkup = guide.nextActionControlId
    ? `
      <button
        class="button button-primary"
        type="button"
        data-run-action="${escapeHtml(guide.nextActionControlId)}"
      >
        ${escapeHtml(displayRunActionLabel(guide.nextActionControlId))}
      </button>
    `
    : '<a class="button button-ghost button-link" href="#run-events">Review evidence</a>';

  const proofItems = [
    {
      detail: guide.currentStageTitle,
      label: "Delivery flow",
      value: compactGuideProgress(guide),
    },
    {
      detail: `${taskCounts.queued} queued · ${taskCounts.running} running · ${taskCounts.failed} failed`,
      label: "Task execution",
      value: `${taskCounts.succeeded}/${taskCounts.total} succeeded`,
    },
    {
      detail: `${eventCount} event${eventCount === 1 ? "" : "s"} recorded`,
      label: "Evidence trail",
      value: `${artifactCount} artifact${artifactCount === 1 ? "" : "s"}`,
    },
    {
      detail: publicationGuard.detail,
      label: "GitHub handoff",
      value: publicationGuard.title,
    },
  ];

  setRenderedHtml(
    elements.runOutcomeBanner,
    `
      <article
        class="run-outcome-card run-outcome-card-${escapeHtml(tone)}"
        data-run-outcome-banner="true"
      >
        <div class="run-outcome-copy">
          <p class="panel-kicker">Selected run outcome</p>
          <h3>${escapeHtml(guide.nextActionTitle)}</h3>
          <p>${escapeHtml(guide.nextActionDetail)}</p>
        </div>
        <div class="run-outcome-proof-grid">
          ${proofItems.map(renderRunOutcomeProofItem).join("")}
        </div>
        <div class="run-outcome-action">
          <span class="badge badge-${escapeHtml(tone)}">${escapeHtml(guide.badgeLabel)}</span>
          ${actionMarkup}
        </div>
      </article>
    `,
    { markUpdated: false }
  );
}

function renderRunOutcomeProofItem(item) {
  return `
    <div class="run-outcome-proof" data-run-outcome-proof="true">
      <span>${escapeHtml(item.label)}</span>
      <strong>${escapeHtml(item.value)}</strong>
      <small>${escapeHtml(item.detail)}</small>
    </div>
  `;
}

function renderRunSectionNav(runDetail, guide, events) {
  const taskCounts = normalizedTaskCounts(runDetail?.task_counts);
  const artifactCount = Number(runDetail?.artifact_count ?? runDetail?.artifacts?.length ?? 0);
  const eventCount = Array.isArray(events) ? events.length : 0;
  const nextActionLabel = guide?.nextActionControlId
    ? displayRunActionLabel(guide.nextActionControlId)
    : guide?.badgeLabel ?? "Idle";
  const links = [
    {
      key: "guide",
      href: "#run-guide",
      label: "Guide",
      detail: compactGuideProgress(guide),
    },
    {
      key: "controls",
      href: "#run-controls",
      label: "Controls",
      detail: nextActionLabel,
    },
    {
      key: "tasks",
      href: "#run-tasks",
      label: "Tasks",
      detail: `${taskCounts.total} task${taskCounts.total === 1 ? "" : "s"}`,
    },
    {
      key: "artifacts",
      href: "#run-artifacts",
      label: "Artifacts",
      detail: `${artifactCount} artifact${artifactCount === 1 ? "" : "s"}`,
    },
    {
      key: "events",
      href: "#run-events",
      label: "Events",
      detail: `${eventCount} event${eventCount === 1 ? "" : "s"}`,
    },
  ];

  setRenderedHtml(elements.runSectionNav, links.map(renderRunSectionLink).join(""), {
    markUpdated: false,
  });
}

function compactGuideProgress(guide) {
  const completed = Number(guide?.completedStageCount ?? 0);
  const total = Number(guide?.totalStageCount ?? 5);
  const safeCompleted = Number.isFinite(completed) ? completed : 0;
  const safeTotal = Number.isFinite(total) && total > 0 ? total : 5;
  return `${safeCompleted}/${safeTotal} done`;
}

function renderRunSectionLink(link) {
  return `
    <a
      class="run-section-link"
      href="${escapeHtml(link.href)}"
      data-run-section-link="${escapeHtml(link.key)}"
      data-ui-stable-key="run-section:${escapeHtml(link.key)}"
    >
      <span class="run-section-label">${escapeHtml(link.label)}</span>
      <span class="run-section-count" data-run-section-count="${escapeHtml(link.key)}">
        ${escapeHtml(link.detail)}
      </span>
    </a>
  `;
}

function renderRunControlReadinessBoard(runDetail, guide) {
  const items = buildMissionControlReadinessItems(runDetail, guide);
  const availableCount = items.filter((item) => item.enabled).length;

  setRenderedHtml(
    elements.runControlReadinessBoard,
    `
      <div class="run-control-readiness-head">
        <div>
          <p class="panel-kicker">Control readiness</p>
          <h4>Why each run action is safe or locked</h4>
        </div>
        <span class="badge badge-${escapeHtml(availableCount ? "warning" : "neutral")}">
          ${escapeHtml(`${availableCount}/${items.length} available`)}
        </span>
      </div>
      <div class="run-control-readiness-grid">
        ${items.map(renderRunControlReadinessCard).join("")}
      </div>
    `,
    { markUpdated: false }
  );
}

function renderRunControlReadinessCard(item) {
  const tone = item.recommended ? "warning" : item.enabled ? "success" : "neutral";
  const status = item.recommended ? "Recommended" : item.enabled ? "Available" : "Locked";
  const detail = item.enabled
    ? "Guard passed. This control can run from the selected run context."
    : item.reason;

  return `
    <article
      class="run-control-readiness-card run-control-readiness-card-${escapeHtml(tone)}"
      data-run-control-readiness-card="true"
      data-run-control-action="${escapeHtml(item.actionId)}"
    >
      <div class="run-control-readiness-card-head">
        <div>
          <p class="panel-kicker">${escapeHtml(status)}</p>
          <h4>${escapeHtml(item.label)}</h4>
        </div>
        <span class="badge badge-${escapeHtml(tone)}">${escapeHtml(status)}</span>
      </div>
      <p>${escapeHtml(item.description)}</p>
      <p class="microcopy">${escapeHtml(detail)}</p>
    </article>
  `;
}

function syncRunGuideProgressMeter(guide) {
  const total = Number(guide.totalStageCount ?? 5);
  const completed = Number(guide.completedStageCount ?? 0);
  const safeTotal = Number.isFinite(total) && total > 0 ? total : 5;
  const safeCompleted = Math.min(
    safeTotal,
    Math.max(0, Number.isFinite(completed) ? completed : 0)
  );
  const progressPercent = `${Math.round((safeCompleted / safeTotal) * 100)}%`;
  const progressFill = elements.runGuideProgressMeter?.querySelector("span");

  if (!elements.runGuideProgressMeter || !progressFill) {
    return;
  }

  elements.runGuideProgressMeter.setAttribute("aria-valuemax", String(safeTotal));
  elements.runGuideProgressMeter.setAttribute("aria-valuenow", String(safeCompleted));
  elements.runGuideProgressMeter.title = `${safeCompleted} of ${safeTotal} stages complete`;
  progressFill.style.width = progressPercent;
}

function renderRunGuideAction(runDetail, guide) {
  const actionId = guide.nextActionControlId;
  if (!runDetail || !actionId) {
    elements.runGuideActionButton.classList.add("hidden");
    elements.runGuideActionButton.removeAttribute("data-run-action");
    delete elements.runGuideActionButton.dataset.idleLabel;
    setTextContent(elements.runGuideActionHint, guide.nextActionDetail);
    return;
  }

  elements.runGuideActionButton.classList.remove("hidden");
  elements.runGuideActionButton.dataset.runAction = actionId;
  elements.runGuideActionButton.dataset.idleLabel = displayRunActionLabel(actionId);
  elements.runGuideActionButton.textContent = displayRunActionLabel(actionId);
  setTextContent(
    elements.runGuideActionHint,
    `Recommended control: ${displayRunActionLabel(actionId)}. The same action remains available in Run controls below.`
  );
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
      nextActionControlId: null,
      currentStageTitle: "Waiting for a run",
      currentStageDetail:
        "The selected-run guide only activates once a concrete run, task set, and artifact history exist.",
      progressSummary: "0 of 5 stages complete",
      completedStageCount: 0,
      totalStageCount: 5,
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
  const availability = runActionAvailability(runDetail);
  const remotePublicationBlocked =
    !availability["publish-pr"].enabled && !availability["draft-pr"].enabled;

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
            ? availability["draft-pr"].enabled
              ? "Branch publication is complete. Open or reuse the draft PR when you want review to start."
              : "Branch publication is complete, but repository-target policy blocks the draft PR handoff until this run matches an allowlisted publication target."
            : prExportReady
              ? remotePublicationBlocked
                ? "The exported PR bundle is ready locally, but repository-target policy blocks remote publication and draft PR until this run matches an allowlisted publication target."
                : "The exported PR bundle is ready. Publish it or create the draft PR for GitHub review."
              : qualityReady && prCandidateReady
                ? remotePublicationBlocked
                  ? "The run is promotable. Export can still create the local PR bundle, but repository-target policy blocks remote publication and draft PR until this run matches an allowlisted publication target."
                  : "The run is promotable. Export or draft the PR when you are ready for remote handoff."
                : "PR promotion stays locked until execution succeeds and quality is evaluated.",
    },
  ];

  const completedStageCount = stages.filter((stage) => stage.state === "complete").length;
  const currentStage =
    stages.find((stage) => stage.state === "blocked" || stage.state === "active") ??
    stages.find((stage) => stage.state === "pending") ??
    stages[stages.length - 1];

  const nextAction = recommendedRunAction({
    availability,
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
      availability,
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
    nextActionControlId: nextAction.controlActionId ?? null,
    currentStageTitle: currentStage.title,
    currentStageDetail: currentStage.detail,
    progressSummary: `${completedStageCount} of ${stages.length} stages complete`,
    completedStageCount,
    totalStageCount: stages.length,
    blockerDetail: guideBlockerDetail({
      availability,
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
      controlActionId: null,
    };
  }

  if (context.executionBlocked) {
    return {
      badgeTone: "error",
      badgeLabel: "Blocked",
      title: "Inspect the failure",
      detail:
        "Review the failing task cards and recent run events before retrying, changing the brief, or promoting anything further.",
      controlActionId: null,
    };
  }

  if (context.runningTaskCount > 0 || context.runDetail.status === "executing") {
    return {
      badgeTone: "warning",
      badgeLabel: "Running",
      title: "Monitor active work",
      detail:
        "At least one task is already running. Let it finish, then refresh or use Worker once if you are manually draining the queue.",
      controlActionId: null,
    };
  }

  if (context.queuedTaskCount > 0 || context.runDetail.status === "queued") {
    return {
      badgeTone: "warning",
      badgeLabel: "Queued",
      title: "Run next task",
      detail:
        "Backlog, policy, and routing are ready. Use Run next task for one controlled step or Worker once to advance one queued item end-to-end.",
      controlActionId: "tasks-next",
    };
  }

  if (context.executionComplete && !context.qualityReady) {
    return {
      badgeTone: "warning",
      badgeLabel: "Quality",
      title: "Evaluate quality",
      detail:
        "Execution finished. Evaluate quality before you export, publish, or open a PR so promotion stays tied to fresh artifacts.",
      controlActionId: "evaluate-quality",
    };
  }

  if (context.publicationReady) {
    if (!context.availability["draft-pr"].enabled) {
      return {
        badgeTone: "error",
        badgeLabel: "Policy",
        title: "Review repository guard",
        detail:
          "Branch publication is already complete, but repository-target policy still blocks the draft PR handoff. Fix the allowlist before GitHub review can start.",
        controlActionId: null,
      };
    }

    return {
      badgeTone: "warning",
      badgeLabel: "Draft PR",
      title: "Create draft PR",
      detail:
        "Branch publication is complete. Create the draft PR now to move the run into GitHub review without bypassing human approval.",
      controlActionId: "draft-pr",
    };
  }

  if (context.prExportReady) {
    if (!context.availability["publish-pr"].enabled && !context.availability["draft-pr"].enabled) {
      return {
        badgeTone: "error",
        badgeLabel: "Policy",
        title: "Review repository guard",
        detail:
          "The local PR export already exists, but repository-target policy blocks remote publication and draft PR. Fix the allowlist before GitHub handoff can continue.",
        controlActionId: null,
      };
    }

    return {
      badgeTone: "warning",
      badgeLabel: "Publish",
      title: "Publish PR export",
      detail:
        "The PR export bundle is ready. Publish it when you want the remote branch pushed before the GitHub PR handoff.",
      controlActionId: "publish-pr",
    };
  }

  if (context.qualityReady && context.prCandidateReady) {
    if (!context.availability["publish-pr"].enabled && !context.availability["draft-pr"].enabled) {
      return {
        badgeTone: "warning",
        badgeLabel: "Promote",
        title: "Export PR candidate",
        detail:
          "The run is promotable. Export the PR candidate next to persist the local promotion artifact, but remote publication and draft PR will stay blocked until the repository-target allowlist matches this run.",
        controlActionId: "export-pr",
      };
    }

    return {
      badgeTone: "warning",
      badgeLabel: "Promote",
      title: "Export PR candidate",
      detail:
        "The run is promotable. Export the PR candidate for an explicit promotion artifact, or use Create draft PR for the full handoff.",
      controlActionId: "export-pr",
    };
  }

  if (context.planningReady) {
    return {
      badgeTone: "warning",
      badgeLabel: "Ready",
      title: "Start execution",
      detail:
        "The orchestrator already has the plan and routing contract. The next meaningful change is task execution.",
      controlActionId: "tasks-next",
    };
  }

  return {
    badgeTone: "neutral",
    badgeLabel: "Waiting",
    title: "Submit a brief",
    detail:
      "No durable run state is available yet. Start from brief intake so the orchestrator can materialize a backlog and policy contract.",
    controlActionId: null,
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
    if (!context.availability["draft-pr"].enabled) {
      return "Branch publication is complete, but repository-target policy is still blocking the draft PR handoff.";
    }
    return "The branch is already published. The remaining orchestrator handoff is the GitHub draft PR.";
  }
  if (context.prExportReady) {
    if (!context.availability["publish-pr"].enabled && !context.availability["draft-pr"].enabled) {
      return "This run already has a local PR export bundle, but repository-target policy is blocking the remote GitHub handoff.";
    }
    return "This run already has an export bundle and is waiting for remote publication or direct draft PR creation.";
  }
  if (context.qualityReady && !context.availability["publish-pr"].enabled && !context.availability["draft-pr"].enabled) {
    return "The run is promotable locally, but repository-target policy is still blocking the remote GitHub handoff.";
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
    if (!context.availability["publish-pr"].enabled && !context.availability["draft-pr"].enabled) {
      return "Remote GitHub handoff is blocked by repository-target policy; export remains the safe next step until the allowlist matches this run.";
    }
    return "Remote branch publication stays blocked until the PR candidate is exported.";
  }
  if (!context.publicationReady) {
    if (!context.availability["publish-pr"].enabled && !context.availability["draft-pr"].enabled) {
      return "GitHub review cannot start until repository-target policy allows publication or draft PR creation for this run.";
    }
    return "GitHub review does not start until the export is published or the draft-PR flow runs.";
  }

  if (!context.availability["draft-pr"].enabled) {
    return "GitHub review cannot start until repository-target policy allows the draft PR handoff for this run.";
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
  setTextContent(elements.taskHeadline, `${tasks.length} task(s) materialized`);
  if (!tasks.length) {
    setRenderedHtml(
      elements.taskTableWrap,
      '<div class="empty-state compact">No tasks persisted for this run yet.</div>'
    );
    return;
  }

  setRenderedHtml(elements.taskTableWrap, `
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
  `);
}

function renderArtifacts(runDetail) {
  const highlights = Array.isArray(runDetail.artifact_highlights)
    ? runDetail.artifact_highlights
    : [];
  const artifacts = Array.isArray(runDetail.artifacts) ? runDetail.artifacts : [];
  setTextContent(
    elements.artifactHeadline,
    `${artifacts.length} artifact(s), ${highlights.length} highlight(s)`
  );
  setRenderedHtml(
    elements.artifactEvidenceStrip,
    renderArtifactEvidenceStrip(runDetail, artifacts)
  );

  setRenderedHtml(
    elements.artifactHighlights,
    highlights.length
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
      : '<div class="empty-state compact">No highlight artifacts selected for this run.</div>'
  );

  if (!artifacts.length) {
    setRenderedHtml(
      elements.artifactTableWrap,
      '<div class="empty-state compact">No artifacts persisted for this run yet.</div>'
    );
    return;
  }

  setRenderedHtml(elements.artifactTableWrap, `
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
  `);
}

function renderArtifactEvidenceStrip(runDetail, artifacts) {
  return artifactEvidenceSummaryItems(runDetail, artifacts)
    .map(renderArtifactEvidenceSummaryItem)
    .join("");
}

function artifactEvidenceSummaryItems(runDetail, artifacts) {
  const safeArtifacts = Array.isArray(artifacts) ? artifacts : [];
  const planningTypes = [
    BACKLOG_ARTIFACT_TYPE,
    POLICY_REPORT_ARTIFACT_TYPE,
    DISPATCH_PLAN_ARTIFACT_TYPE,
    "scaffold_bundle",
    "code_bundle",
  ];
  const executionTypes = [
    "workspace_snapshot",
    "task_workspace_input",
    "workspace_patch",
    "agent_task_report",
    "log",
  ];
  const qualityTypes = [QUALITY_REPORT_ARTIFACT_TYPE];
  const handoffTypes = [
    PR_CANDIDATE_ARTIFACT_TYPE,
    PR_EXPORT_ARTIFACT_TYPE,
    PR_PUBLICATION_ARTIFACT_TYPE,
    GITHUB_PULL_REQUEST_ARTIFACT_TYPE,
  ];

  return [
    artifactEvidenceSummaryItem({
      artifacts: safeArtifacts,
      detail: "Brief, backlog, policy, routing, and generated repository inputs.",
      label: "Planning evidence",
      readyTone: "success",
      types: planningTypes,
    }),
    artifactEvidenceSummaryItem({
      artifacts: safeArtifacts,
      detail: "Runtime logs, prepared workspaces, snapshots, patches, and agent reports.",
      label: "Execution evidence",
      readyTone: runDetail.status === "failed" ? "error" : "success",
      types: executionTypes,
    }),
    artifactEvidenceSummaryItem({
      artifacts: safeArtifacts,
      detail: "Quality reports that gate promotion and should follow the latest execution output.",
      emptyTone: runDetail.status === "succeeded" ? "warning" : "neutral",
      label: "Quality evidence",
      readyTone: "success",
      types: qualityTypes,
    }),
    artifactEvidenceSummaryItem({
      artifacts: safeArtifacts,
      detail: "PR candidate, export, branch publication, and GitHub draft-PR handoff records.",
      label: "Handoff evidence",
      readyTone: artifactTypesInclude(safeArtifacts, GITHUB_PULL_REQUEST_ARTIFACT_TYPE)
        ? "success"
        : "warning",
      types: handoffTypes,
    }),
  ];
}

function artifactEvidenceSummaryItem({
  artifacts,
  detail,
  emptyTone = "neutral",
  label,
  readyTone,
  types,
}) {
  const matchingArtifacts = artifacts.filter((artifact) => types.includes(artifact.artifact_type));
  const latest = latestArtifactTimestamp(types, artifacts);

  return {
    count: matchingArtifacts.length,
    detail,
    label,
    latest,
    tone: matchingArtifacts.length ? readyTone : emptyTone,
    types,
  };
}

function renderArtifactEvidenceSummaryItem(item) {
  const tone = normalizePulseTone(item.tone);
  const latestLabel = item.latest ? formatTimestamp(item.latest) : "not recorded yet";

  return `
    <article
      class="artifact-evidence-card artifact-evidence-card-${escapeHtml(tone)}"
      data-artifact-evidence-card="true"
    >
      <div class="mission-feed-head">
        <div>
          <p class="panel-kicker">${escapeHtml(item.label)}</p>
          <h4>${escapeHtml(`${item.count} artifact(s)`)}</h4>
        </div>
        <span class="badge badge-${escapeHtml(tone)}">${escapeHtml(toneLabel(tone))}</span>
      </div>
      <p>${escapeHtml(item.detail)}</p>
      <div class="mission-feed-meta">
        <span>${escapeHtml(latestLabel)}</span>
        <span>${escapeHtml(item.types.join(", "))}</span>
      </div>
    </article>
  `;
}

function artifactTypesInclude(artifacts, artifactType) {
  return artifacts.some((artifact) => artifact.artifact_type === artifactType);
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
  const safeEvents = Array.isArray(events) ? events : [];
  const visibleEvents = filteredRunEvents(safeEvents, state.selectedRunEventFilter);
  const filterLabel = runEventFilterLabel(state.selectedRunEventFilter);
  setTextContent(
    elements.eventHeadline,
    state.selectedRunEventFilter === "all"
      ? `${safeEvents.length} recent event(s)`
      : `${visibleEvents.length} of ${safeEvents.length} event(s) · ${filterLabel}`
  );
  setRenderedHtml(elements.eventFilterBar, renderRunEventFilterBar(safeEvents), {
    markUpdated: false,
  });

  if (!safeEvents.length) {
    setRenderedHtml(
      elements.eventTimeline,
      '<div class="empty-state compact">No run events persisted for this run yet.</div>'
    );
    return;
  }

  if (!visibleEvents.length) {
    setRenderedHtml(
      elements.eventTimeline,
      `<div class="empty-state compact">No run events match the ${escapeHtml(filterLabel)} filter.</div>`
    );
    return;
  }

  setRenderedHtml(
    elements.eventTimeline,
    visibleEvents
      .map(renderRunEventTimelineItem)
      .join("")
  );
}

function renderRunEventFilterBar(events) {
  return RUN_EVENT_FILTERS.map((filter) => {
    const count = runEventFilterCount(events, filter.id);
    const active = filter.id === state.selectedRunEventFilter;
    return `
      <button
        class="event-filter-chip${active ? " is-active" : ""}"
        type="button"
        data-run-event-filter="${escapeHtml(filter.id)}"
        data-ui-stable-key="run-event-filter:${escapeHtml(filter.id)}"
        aria-pressed="${active ? "true" : "false"}"
      >
        <span>${escapeHtml(filter.label)}</span>
        <strong>${escapeHtml(count)}</strong>
      </button>
    `;
  }).join("");
}

function setSelectedRunEventFilter(filterId) {
  const nextFilter = normalizeRunEventFilter(filterId);
  if (state.selectedRunEventFilter === nextFilter) {
    return;
  }

  state.selectedRunEventFilter = nextFilter;
  renderEvents(state.selectedRunEvents);
}

function filteredRunEvents(events, filterId) {
  const normalizedFilterId = normalizeRunEventFilter(filterId);
  return events.filter((event) => matchesRunEventFilter(event, normalizedFilterId));
}

function runEventFilterCount(events, filterId) {
  if (filterId === "all") {
    return events.length;
  }

  return filteredRunEvents(events, filterId).length;
}

function matchesRunEventFilter(event, filterId) {
  const filter = RUN_EVENT_FILTERS.find((item) => item.id === filterId);
  if (!filter || !Array.isArray(filter.types)) {
    return true;
  }

  return filter.types.includes(event.event_type);
}

function normalizeRunEventFilter(filterId) {
  return RUN_EVENT_FILTERS.some((filter) => filter.id === filterId) ? filterId : "all";
}

function runEventFilterLabel(filterId) {
  return RUN_EVENT_FILTERS.find((filter) => filter.id === normalizeRunEventFilter(filterId))?.label ?? "All";
}

function renderRunEventTimelineItem(event) {
  const item = runEventPresentation(event, state.selectedRunDetail);

  return `
    <article
      class="timeline-item timeline-item-${escapeHtml(item.tone)}"
      data-run-event-card="true"
    >
      <div class="timeline-head">
        <div>
          <p class="panel-kicker">${escapeHtml(item.kicker)}</p>
          <h4 data-run-event-human-title="true">${escapeHtml(item.title)}</h4>
        </div>
        <span class="badge badge-${escapeHtml(item.tone)}">
          ${escapeHtml(item.badge)}
        </span>
      </div>
      <p>${escapeHtml(item.summary)}</p>
      <div class="timeline-meta">
        <span>${escapeHtml(item.detail)}</span>
      </div>
    </article>
  `;
}

function clearRunSelection(message, title = "No run selected") {
  state.selectedRunId = null;
  state.selectedRunDetail = null;
  state.selectedRunEvents = [];
  state.selectedAgentActivityId = "all";
  state.selectedAgentReportArtifactId = null;
  state.selectedAgentLogArtifactId = null;
  setTextContent(elements.selectedRunLabel, "No run selected.");
  setRenderedHtml(elements.detailEmptyState, renderDetailEmptyStateMarkup({
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
  }));
  elements.detailEmptyState.classList.remove("hidden");
  elements.runDetailShell.classList.add("hidden");
  const guide = renderRunGuide(null, []);
  renderRunOutcomeBanner(null, guide, []);
  renderRunSectionNav(null, guide, []);
  renderRunControlReadinessBoard(null, guide);
  renderRunActionHighlights(null);
  syncRunActionDraftInputs(null);
  syncRunActionControlsWithState();
  syncUiUrlState();
  renderOperatorPulse();
  renderMissionControl();
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

function setDashboardRefreshState(refreshing) {
  if (elements.pageShell) {
    elements.pageShell.dataset.refreshState = refreshing ? "syncing" : "idle";
  }
  if (elements.refreshButton) {
    elements.refreshButton.dataset.syncing = refreshing ? "true" : "false";
  }
  elements.lastRefresh.classList.toggle("is-syncing", refreshing);
}

function setTextContent(target, text, options = {}) {
  const nextText = String(text ?? "");
  if (target.textContent === nextText) {
    return false;
  }

  target.textContent = nextText;
  if (options.markUpdated !== false) {
    markRefreshTargetUpdated(target);
  }
  return true;
}

function setRenderedHtml(target, html, options = {}) {
  const nextHtml = html ?? "";
  if (target.__lastRenderedHtml === nextHtml) {
    return false;
  }

  const focusRestore = captureStableFocus(target);
  const scrollRestore = captureScrollPosition(target);
  preserveTargetHeight(target, () => {
    target.innerHTML = nextHtml;
  });
  restoreScrollPosition(target, scrollRestore);
  restoreStableFocus(target, focusRestore);
  target.__lastRenderedHtml = nextHtml;
  if (options.markUpdated !== false) {
    markRefreshTargetUpdated(target);
  }
  return true;
}

function preserveTargetHeight(target, update) {
  const currentHeight = target.offsetHeight;
  if (currentHeight > 0) {
    target.style.minHeight = `${currentHeight}px`;
  }
  update();
  window.requestAnimationFrame(() => {
    target.style.removeProperty("min-height");
  });
}

function captureStableFocus(target) {
  const active = document.activeElement;
  if (!active || !target.contains(active) || typeof active.closest !== "function") {
    return null;
  }

  const stableElement = active.closest("[data-ui-stable-key]");
  if (!stableElement || !target.contains(stableElement)) {
    return null;
  }

  const stableKey = stableElement.dataset.uiStableKey;
  if (!stableKey) {
    return null;
  }

  return `[data-ui-stable-key="${cssAttributeValue(stableKey)}"]`;
}

function restoreStableFocus(target, selector) {
  if (!selector) {
    return;
  }

  const nextActiveElement = target.querySelector(selector);
  if (!nextActiveElement || typeof nextActiveElement.focus !== "function") {
    return;
  }

  nextActiveElement.focus({ preventScroll: true });
}

function captureScrollPosition(target) {
  return {
    left: target.scrollLeft,
    top: target.scrollTop,
  };
}

function restoreScrollPosition(target, position) {
  if (!position) {
    return;
  }

  if (target.scrollLeft !== position.left) {
    target.scrollLeft = position.left;
  }
  if (target.scrollTop !== position.top) {
    target.scrollTop = position.top;
  }
}

function cssAttributeValue(value) {
  return String(value).replace(/\\/g, "\\\\").replace(/"/g, '\\"');
}

function markRefreshTargetUpdated(target) {
  if (!state.refreshAnimationsEnabled) {
    return;
  }

  const surface = target.closest("[data-ui-refresh-surface]") ?? target;
  surface.classList.add("surface-updated");

  const existingTimer = surface.__surfaceUpdateTimer;
  if (existingTimer) {
    window.clearTimeout(existingTimer);
  }

  surface.__surfaceUpdateTimer = window.setTimeout(() => {
    surface.classList.remove("surface-updated");
    surface.__surfaceUpdateTimer = null;
  }, 520);
}

function setBadge(target, tone, text) {
  const nextClassName = `badge badge-${tone}`;
  if (target.className !== nextClassName) {
    target.className = nextClassName;
  }
  setTextContent(target, text, { markUpdated: false });
}

function setConsolePayload(target, payload) {
  setRenderedHtml(target, renderConsolePayload(payload));
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
  state.automationDisclosurePreferences = restoreAutomationDisclosurePreferences();
  state.autoRefresh = restoreAutoRefreshPreference();
  elements.autoRefreshToggle.checked = state.autoRefresh;
  syncRefreshModeControls();
  renderDashboardLoadingState();
  renderBriefExampleHint();
  renderBriefReadiness();
  renderQueueInspectorEmpty();
  const guide = renderRunGuide(null, []);
  renderRunOutcomeBanner(null, guide, []);
  renderRunSectionNav(null, guide, []);
  renderRunControlReadinessBoard(null, guide);
  renderRunActionHighlights(null);
  syncRunActionDraftInputs(null);
  syncRunActionControlsWithState();
  renderMissionControl();
}

function renderDashboardLoadingState() {
  renderLastRefreshStatus();
  renderOperatorDock();
  setRenderedHtml(elements.packChips, '<span class="chip chip-loading">Loading pack catalog...</span>', {
    markUpdated: false,
  });
  setRenderedHtml(
    elements.capabilityGrid,
    CAPABILITY_LOADING_CARD_TITLES
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
      .join(""),
    { markUpdated: false }
  );
  setRenderedHtml(
    elements.statusGrid,
    DASHBOARD_LOADING_CARD_TITLES
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
      .join(""),
    { markUpdated: false }
  );
  setRenderedHtml(elements.runsList, loadingOrEmptyState("Loading recent runs..."), {
    markUpdated: false,
  });
  setRenderedHtml(elements.webhookActionsList, loadingOrEmptyState("Loading webhook actions..."), {
    markUpdated: false,
  });
  setRenderedHtml(
    elements.repositorySignalsList,
    loadingOrEmptyState("Loading repository signals..."),
    { markUpdated: false }
  );
  setRenderedHtml(
    elements.webhookDeliveriesList,
    loadingOrEmptyState("Loading webhook deliveries..."),
    { markUpdated: false }
  );
  setTextContent(elements.webhookActionCount, "…", { markUpdated: false });
  setTextContent(elements.signalCount, "…", { markUpdated: false });
  setTextContent(elements.deliveryCount, "…", { markUpdated: false });
  syncAutomationDisclosures();

  if (state.selectedRunId) {
    setTextContent(elements.selectedRunLabel, `Loading run ${shortId(state.selectedRunId)}...`, {
      markUpdated: false,
    });
    setRenderedHtml(elements.detailEmptyState, renderDetailEmptyStateMarkup({
      title: "Loading selected run",
      message: "Loading the selected run snapshot, tasks, artifacts, and events.",
      steps: [],
    }), { markUpdated: false });
  } else {
    setTextContent(elements.selectedRunLabel, "Loading latest run...", { markUpdated: false });
    setRenderedHtml(elements.detailEmptyState, renderDetailEmptyStateMarkup({
      title: "Loading latest run",
      message: "Loading the latest run snapshot, tasks, artifacts, and events.",
      steps: [],
    }), { markUpdated: false });
  }

  renderMissionControl();
}

function restoreAutoRefreshPreference() {
  if (supportsRealtimeUpdates()) {
    return false;
  }

  const saved = window.localStorage.getItem(AUTO_REFRESH_STORAGE_KEY);
  if (saved === "true") {
    return true;
  }
  if (saved === "false") {
    return false;
  }
  return elements.autoRefreshToggle.checked;
}

function persistAutoRefreshPreference() {
  if (supportsRealtimeUpdates()) {
    window.localStorage.removeItem(AUTO_REFRESH_STORAGE_KEY);
    return;
  }

  window.localStorage.setItem(
    AUTO_REFRESH_STORAGE_KEY,
    state.autoRefresh ? "true" : "false"
  );
}

function syncRefreshModeControls() {
  const toggleShell = elements.autoRefreshToggleShell ?? elements.autoRefreshToggle?.closest(".toggle");
  if (!toggleShell) {
    return;
  }

  if (supportsRealtimeUpdates()) {
    state.autoRefresh = false;
    elements.autoRefreshToggle.checked = false;
    elements.autoRefreshToggle.disabled = true;
    toggleShell.hidden = true;
    window.localStorage.removeItem(AUTO_REFRESH_STORAGE_KEY);
    return;
  }

  elements.autoRefreshToggle.disabled = false;
  toggleShell.hidden = false;
}

function renderLastRefreshStatus() {
  const fallbackSummary = supportsRealtimeUpdates()
    ? "manual HTTP fallback"
    : state.autoRefresh
      ? `polling every ${AUTO_REFRESH_INTERVAL_MS / 1000}s`
      : "manual mode";

  if (state.realtimeConnected) {
    if (!state.lastRealtimeSnapshotAt && !state.lastHttpRefreshAt) {
      setTextContent(elements.lastRefresh, "Live updates connected · waiting for first websocket snapshot", {
        markUpdated: false,
      });
      return;
    }

    setTextContent(
      elements.lastRefresh,
      "Live updates connected · WebSocket stream active",
      { markUpdated: false }
    );
    return;
  }

  if (state.realtimeConnecting) {
    setTextContent(
      elements.lastRefresh,
      state.realtimeHasConnected
        ? `Live updates reconnecting · ${fallbackSummary}`
        : "Live updates connecting · waiting for first websocket snapshot",
      { markUpdated: false }
    );
    return;
  }

  if (!state.lastHttpRefreshAt) {
    setTextContent(
      elements.lastRefresh,
      supportsRealtimeUpdates()
        ? `Waiting for live connection · ${fallbackSummary}`
        : state.autoRefresh
          ? `Waiting for first snapshot · polling every ${AUTO_REFRESH_INTERVAL_MS / 1000}s`
          : "Manual refresh mode",
      { markUpdated: false }
    );
    return;
  }

  setTextContent(
    elements.lastRefresh,
    supportsRealtimeUpdates()
      ? `Live updates unavailable · last HTTP refresh ${new Date(state.lastHttpRefreshAt).toLocaleTimeString()} · ${fallbackSummary}`
      : `Last refresh ${new Date(state.lastHttpRefreshAt).toLocaleTimeString()} · ${fallbackSummary}`,
    { markUpdated: false }
  );
  renderOperatorPulse();
}

function renderQueueInspectorEmpty() {
  state.selectedQueueRunId = null;
  setTextContent(
    elements.queueInspectorHeadline,
    "Queue items show what reached the orchestrator before or around run creation. Open one to inspect signed ingress, routed actions, or automation handoff state.",
    { markUpdated: false }
  );
  setRenderedHtml(
    elements.queueInspectorSummary,
    renderSectionEmptyState(
      "Queue inspector",
      "No queue item selected",
      "Pick a delivery, action request, or repository signal to see what happened before a run was created or advanced."
    ),
    { markUpdated: false }
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

function humanizeIdentifier(value) {
  const rendered = String(value ?? "unknown")
    .trim()
    .replace(/[_-]+/g, " ")
    .replace(/\s+/g, " ");
  if (!rendered) {
    return "Unknown";
  }

  return rendered.replace(/\b\w/g, (letter) => letter.toUpperCase());
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
  pushActionHighlightItem(
    items,
    "Repository target",
    payload.repository_target_id,
    "Repository target used for branch defaults."
  );
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
  pushActionHighlightItem(
    items,
    "Repository target",
    payload.repository_target_id,
    "Repository target used for remote resolution."
  );
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
  pushActionHighlightItem(
    items,
    "Repository target",
    payload.repository_target_id,
    "Repository target used for draft PR handoff."
  );
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
  const activeButtons = runActionButtons().filter(
    (button) => button.dataset.runAction === actionId
  );
  renderOperatorDock();

  if (!busy) {
    syncRunActionControlsWithState();
    return;
  }

  for (const button of runActionButtons()) {
    const activeButton = activeButtons.includes(button);
    setButtonBusyState(
      button,
      activeButton ? runActionBusyLabel(actionId) : buttonIdleLabel(button),
      activeButton
    );
    if (!activeButton) {
      button.disabled = true;
    }
    button.title = "";
  }

  elements.branchNameInput.disabled = true;
  elements.remoteUrlInput.disabled = true;
  elements.repositoryTargetSelect.disabled = true;
  elements.publishPushToggle.disabled = true;
  elements.resetRunActionDraftButton.disabled = true;
  elements.branchNameInput.title = "";
  elements.remoteUrlInput.title = "";
  elements.repositoryTargetSelect.title = "";
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

function remotePublicationAvailability(runDetail, actionLabel) {
  if (!runDetail) {
    return disabledRunAction("Select a run to use operator actions.");
  }

  const enforcementEnabled = repositoryTargetsConfig().enforcement_enabled === true;
  if (!enforcementEnabled) {
    return enabledRunAction();
  }

  const repository = repositoryLabel(runDetail);
  if (!repository) {
    return disabledRunAction(
      `${actionLabel} is blocked because this run has no repository metadata. PR export can still create a local promotion artifact.`
    );
  }

  const matchingTargets = matchingRepositoryTargetsForRun(runDetail);
  const draft = runActionDraftForRun(runDetail);
  const selectedTarget = draft.repositoryTargetId
    ? repositoryTargetById(draft.repositoryTargetId)
    : null;
  const selectedTargetMatches = repositoryTargetMatchesRun(selectedTarget, runDetail);

  if (selectedTargetMatches || matchingTargets.length === 1) {
    return enabledRunAction();
  }

  if (matchingTargets.length > 1) {
    return disabledRunAction(
      `${actionLabel} is blocked until you choose one matching repository target for ${repository}. PR export can still create a local promotion artifact.`
    );
  }

  return disabledRunAction(
    `${actionLabel} is blocked by repository-target policy for ${repository}. PR export can still create a local promotion artifact until the allowlist is updated.`
  );
}

function runArtifactTypes(runDetail) {
  return new Set(runArtifacts(runDetail).map((artifact) => artifact.artifact_type));
}

function runArtifacts(runDetail) {
  const artifacts = [];
  const seen = new Set();
  for (const artifact of [
    ...(Array.isArray(runDetail?.artifacts) ? runDetail.artifacts : []),
    ...(Array.isArray(runDetail?.artifact_highlights) ? runDetail.artifact_highlights : []),
  ]) {
    const key =
      artifact.artifact_id ??
      [artifact.artifact_type, artifact.location_value, artifact.created_at].join(":");
    if (seen.has(key)) {
      continue;
    }
    seen.add(key);
    artifacts.push(artifact);
  }
  return artifacts;
}

function latestRunEventTimestamp(eventTypes, events = state.selectedRunEvents) {
  return latestTimestamp(
    events
      .filter((event) => eventTypes.includes(event.event_type))
      .map((event) => event.created_at)
  );
}

function firstRunEventTimestamp(eventTypes, events = state.selectedRunEvents) {
  return firstTimestamp(
    events
      .filter((event) => eventTypes.includes(event.event_type))
      .map((event) => event.created_at)
  );
}

function latestArtifactTimestamp(artifactTypes, artifacts = runArtifacts(state.selectedRunDetail)) {
  return latestTimestamp(
    artifacts
      .filter((artifact) => artifactTypes.includes(artifact.artifact_type))
      .map((artifact) => artifact.created_at)
  );
}

function latestTaskTimestamp(runDetail) {
  return latestTimestamp(
    (Array.isArray(runDetail?.tasks) ? runDetail.tasks : []).flatMap((task) => [
      task.completed_at,
      task.lease_expires_at,
      task.started_at,
      task.created_at,
    ])
  );
}

function latestTimestamp(values) {
  return timestampByOrder(values, "desc");
}

function firstTimestamp(values) {
  return timestampByOrder(values, "asc");
}

function timestampByOrder(values, order) {
  const sortedValues = values
    .filter(Boolean)
    .sort((left, right) =>
      order === "asc"
        ? sortableTimestamp(left) - sortableTimestamp(right)
        : sortableTimestamp(right) - sortableTimestamp(left)
    );
  return sortedValues[0] ?? null;
}

function timestampAtLeast(candidate, reference) {
  if (!reference) {
    return Boolean(candidate);
  }
  if (!candidate) {
    return false;
  }
  return sortableTimestamp(candidate) >= sortableTimestamp(reference);
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
  const remotePublication = remotePublicationAvailability(runDetail, "Remote publication");
  const draftPrHandoff = remotePublicationAvailability(runDetail, "Draft PR handoff");

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
          : !remotePublication.enabled
            ? disabledRunAction(remotePublication.reason)
            : enabledRunAction(),
    "draft-pr": (() => {
      const availability = prCandidateAvailability(runDetail, artifactTypes, "Draft PR creation");
      if (!availability.enabled) {
        return availability;
      }
      if (!draftPrHandoff.enabled) {
        return disabledRunAction(draftPrHandoff.reason);
      }
      return enabledRunAction();
    })(),
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
    if (!availability["draft-pr"].enabled) {
      return availability["draft-pr"].reason || "Draft PR handoff is blocked by repository-target policy.";
    }
    return "Branch publication is done. Create the draft PR next when you want GitHub review to start.";
  }

  if (!artifactTypes.has(PR_EXPORT_ARTIFACT_TYPE)) {
    if (!availability["publish-pr"].enabled && !availability["draft-pr"].enabled) {
      return "Quality is available. Export the PR candidate next if you want a local promotion artifact, but repository-target policy still blocks remote publication and draft PR for this run.";
    }
    return "Quality is available. Export the PR candidate next, or use Create draft PR for the full GitHub handoff.";
  }

  if (!availability["publish-pr"].enabled && !availability["draft-pr"].enabled) {
    return availability["publish-pr"].reason || availability["draft-pr"].reason || "Remote GitHub handoff is blocked by repository-target policy.";
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
    const busyButton =
      state.runActionInFlight && actionId === state.runActionBusyActionId;

    if (!button.dataset.idleLabel) {
      button.dataset.idleLabel = button.textContent;
    }
    button.textContent = busyButton ? runActionBusyLabel(actionId) : buttonIdleLabel(button);
    button.disabled = state.runActionInFlight || !actionState.enabled;
    button.title = busyButton || actionState.enabled ? "" : actionState.reason;
  }

  const branchActionsEnabled =
    availability["export-pr"].enabled || availability["draft-pr"].enabled;
  const remoteActionsEnabled =
    availability["publish-pr"].enabled || availability["draft-pr"].enabled;

  elements.branchNameInput.disabled = state.runActionInFlight || !branchActionsEnabled;
  elements.remoteUrlInput.disabled = state.runActionInFlight || !remoteActionsEnabled;
  elements.repositoryTargetSelect.disabled =
    state.runActionInFlight || (!branchActionsEnabled && !remoteActionsEnabled);
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
  elements.repositoryTargetSelect.title =
    branchActionsEnabled || remoteActionsEnabled
      ? ""
      : availability["draft-pr"].reason || availability["publish-pr"].reason;
  elements.publishPushToggle.title = availability["publish-pr"].enabled
    ? ""
    : availability["publish-pr"].reason;
  elements.resetRunActionDraftButton.title = state.selectedRunDetail
    ? ""
    : "Select a run before resetting promotion defaults.";
  setTextContent(
    elements.runActionHint,
    runActionHintText(state.selectedRunDetail, availability)
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
