const stateNode = document.querySelector("#state");
const rangeNode = document.querySelector("#range");
const chaser = document.querySelector("#chaser");
const target = document.querySelector("#target");
const chaserConfig = document.querySelector("#chaser-config");
const targetConfig = document.querySelector("#target-config");
const eventsNode = document.querySelector("#events");
const stages = [...document.querySelectorAll("#stages li")];
const rerunButton = document.querySelector("#rerun");
const scenarioSelect = document.querySelector("#scenario");
const policyIdNode = document.querySelector("#policy-id");
const policyChecksNode = document.querySelector("#policy-checks");
const authEventsNode = document.querySelector("#auth-events");
const entitlementsNode = document.querySelector("#entitlements");
const protocolMessagesNode = document.querySelector("#protocol-messages");
const chaserMessageNode = document.querySelector("#chaser-message");
const stationMessageNode = document.querySelector("#station-message");
const replayControls = document.querySelector("#replay-controls");
const replayLabel = document.querySelector("#replay-label");
const replayTimeline = document.querySelector("#replay-timeline");
const replayBack = document.querySelector("#replay-back");
const replayPlay = document.querySelector("#replay-play");
const replayForward = document.querySelector("#replay-forward");
const replayLive = document.querySelector("#replay-live");
let replayHistory = [];
let replaySignature = "";
let replayMode = false;
let replayTimer = null;
let latestLiveSnapshot = null;
let displayedMessageKey = "";
let messageTimer = null;
let awaitingReset = false;

function shortId(value) {
  if (!value || value === "pending") return value;
  return value.length > 24 ? `${value.slice(0, 12)}…${value.slice(-8)}` : value;
}

function eventMetadata(event) {
  const values = [];
  if (event.session_id) values.push(`session ${shortId(event.session_id)}`);
  if (event.policy_id) values.push(`${event.policy_id} v${event.policy_version}`);
  if (event.rule_id) values.push(`rule ${event.rule_id}`);
  if (event.grant_id) values.push(`grant ${shortId(event.grant_id)}`);
  if (event.entitlement_ttl_s) values.push(`TTL ${event.entitlement_ttl_s}s`);
  return values;
}

function policyRows(auth) {
  const assessment = auth.policy_assessments?.at(-1);
  if (assessment) return assessment.rows;
  const policy = auth.policy;
  if (!policy) return [];
  const trustBundle = policy.trust_bundle;
  const credentialProfiles = policy.credential_profiles ?? [];
  const stagePolicies = policy.stage_policies ?? [];
  return [
    {
      control: "Policy validity",
      requirement: `${policy.valid_from} through ${policy.valid_until}`,
      observed: "loaded by authority",
      passed: true,
    },
    {
      control: "Trust bundle",
      requirement: trustBundle
        ? `${trustBundle.bundle_id ?? "unspecified"} v${trustBundle.minimum_version ?? "-"}+`
        : "not specified",
      observed: auth.trust_bundle,
      passed: Boolean(trustBundle),
    },
    {
      control: "Credential profiles",
      requirement: credentialProfiles.map((profile) => profile.profile_id).join(", ") || "not specified",
      observed: auth.phase === "SESSION_AUTHORIZED" ? "verified" : "awaiting session",
      passed: auth.phase === "SESSION_AUTHORIZED",
    },
    {
      control: "Stage rules",
      requirement: `${stagePolicies.length} transition rules; default deny`,
      observed: "awaiting transition evidence",
      passed: true,
    },
  ];
}

function setText(selector, value) {
  document.querySelector(selector).textContent = value;
}

function setConfig(openCraft, openPanel) {
  [[chaser, chaserConfig], [target, targetConfig]].forEach(([craft, panel]) => {
    const isOpen = craft === openCraft && panel === openPanel;
    craft.setAttribute("aria-expanded", String(isOpen));
    panel.setAttribute("aria-hidden", String(!isOpen));
    panel.classList.toggle("open", isOpen);
  });
}

function bindCraftConfig(craft, panel) {
  craft.addEventListener("click", () => setConfig(craft, panel));
  craft.addEventListener("keydown", (event) => {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      setConfig(craft, panel);
    }
  });
}

function showViewportMessage(auth) {
  const messages = auth.events.filter((event) => event.kind === "message");
  const message = messages.at(-1);
  if (!message) {
    chaserMessageNode.classList.remove("active");
    chaserMessageNode.classList.remove("denied");
    stationMessageNode.classList.remove("active");
    stationMessageNode.classList.remove("denied");
    displayedMessageKey = "";
    return;
  }
  const key = `${messages.length}:${message.message_type}:${message.detail}`;
  if (key === displayedMessageKey && !replayMode) return;
  displayedMessageKey = key;
  const node = message.from === "ODYSSEY-7" ? chaserMessageNode : stationMessageNode;
  const otherNode = node === chaserMessageNode ? stationMessageNode : chaserMessageNode;
  otherNode.classList.remove("active");
  otherNode.classList.remove("denied");
  node.querySelector("span").textContent = `${message.from} → ${message.to}`;
  node.querySelector("strong").textContent = message.message_type;
  const metadata = eventMetadata(message);
  node.querySelector("p").textContent = metadata.length
    ? `${message.summary || message.detail} · ${metadata.join(" · ")}`
    : message.summary || message.detail;
  node.classList.toggle("denied", message.code?.startsWith("DENY_") ?? false);
  node.classList.add("active");
  clearTimeout(messageTimer);
  if (!replayMode) messageTimer = setTimeout(() => node.classList.remove("active"), 6000);
}

function renderAuthorization(auth, simulation) {
  setText("#auth-scenario", auth.scenario);
  setText("#auth-mode", auth.mode);
  const latestAssessment = auth.policy_assessments?.at(-1);
  policyIdNode.textContent = latestAssessment
    ? `${auth.policy_id} v${auth.policy_version} · ${latestAssessment.rule_id} · ${latestAssessment.reason}`
    : `${auth.policy_id} v${auth.policy_version ?? "-"} · ${auth.trust_bundle}`;
  const assessmentRows = policyRows(auth).map(({ control, requirement, observed, passed }) => {
    const row = document.createElement("div");
    row.className = `policy-check${passed ? "" : " failed"}`;
    [control, requirement, observed].forEach((text) => {
      const cell = document.createElement("span");
      cell.textContent = text;
      row.append(cell);
    });
    return row;
  });
  policyChecksNode.replaceChildren(...(assessmentRows.length
    ? assessmentRows
    : [Object.assign(document.createElement("p"), { className: "empty", textContent: "Waiting for authority policy" })]));

  const completed = new Set(auth.completed_steps);
  const issuedActions = new Set(auth.entitlements.map((entitlement) => entitlement.action));
  const milestones = {
    encounter: completed.has("INITIAL_HOLD_CONFIRMED"),
    approach: issuedActions.has("enter_approach"),
    final: issuedActions.has("enter_final_approach"),
    soft: issuedActions.has("engage_soft_capture"),
    hard: issuedActions.has("engage_hard_dock"),
  };
  const criticalGates = [
    ["encounter", "Clear encounter at hold", "Verify trust, identity, session and readiness before motion at 3.320 m."],
    ["approach", "Authorize enter approach", "At HOLD · 3.320 m, issue the enter_approach entitlement."],
    ["final", "Authorize final approach", "At APPROACH · 1.120 m, verify corridor evidence and authorize entry."],
    ["soft", "Authorize soft capture", "At FINAL APPROACH · 0.320 m, verify alignment and capture readiness."],
    ["hard", "Authorize hard dock", "At SOFT CAPTURE · 0.040 m, verify latches and relative-motion stability."],
  ];
  let criticalIndex = criticalGates.findIndex(([key]) => !milestones[key]);
  if (criticalIndex < 0) criticalIndex = criticalGates.length - 1;
  const allComplete = Object.values(milestones).every(Boolean);
  const [, criticalTitle, criticalDetail] = criticalGates[criticalIndex];
  setText("#critical-number", `${String(criticalIndex + 1).padStart(2, "0")} / 05`);
  setText("#critical-status", allComplete ? "COMPLETE" : "IN PROGRESS");
  setText("#critical-title", allComplete ? "All docking gates authorized" : criticalTitle);
  setText("#critical-detail", allComplete ? `Four stage entitlements consumed; ${simulation.state} at ${Number(simulation.range_m).toFixed(3)} m.` : criticalDetail);

  const localEvents = auth.events.filter((event) => event.kind !== "message");
  const deniedEvent = auth.events.findLast((event) => event.code?.startsWith("DENY_"));
  if (deniedEvent) {
    setText("#critical-status", "DENIED");
    setText("#critical-title", "Authorization denied");
    setText("#critical-detail", deniedEvent.summary || deniedEvent.detail);
  }
  document.querySelector(".critical-gate").classList.toggle("denied", Boolean(deniedEvent));
  const authRows = localEvents.length
    ? localEvents.slice().reverse().map((event) => {
        const row = document.createElement("p");
        row.className = "auth-event";
        const code = document.createElement("code");
        code.textContent = event.code;
        const detail = document.createElement("span");
        detail.textContent = event.detail;
        row.append(code, detail);
        return row;
      })
    : [Object.assign(document.createElement("p"), { className: "empty", textContent: "Waiting for local checks" })];
  authEventsNode.replaceChildren(...authRows);

  const messages = auth.events.filter((event) => event.kind === "message");
  const messageRows = messages.length
    ? messages.map((event) => {
        const row = document.createElement("article");
        row.className = `protocol-message ${event.from === "ODYSSEY-7" ? "from-chaser" : "from-station"}`;
        row.classList.toggle("denied", event.code?.startsWith("DENY_") ?? false);
        const route = document.createElement("div");
        route.className = "message-route";
        route.textContent = `${event.from}  →  ${event.to}`;
        const type = document.createElement("strong");
        type.textContent = event.message_type;
        const detail = document.createElement("p");
        detail.textContent = event.detail;
        const metadata = document.createElement("div");
        metadata.className = "message-metadata";
        eventMetadata(event).forEach((value) => {
          const chip = document.createElement("span");
          chip.textContent = value;
          metadata.append(chip);
        });
        row.append(route, type, detail, metadata);
        return row;
      })
    : [Object.assign(document.createElement("p"), { className: "empty", textContent: "Waiting for encounter traffic" })];
  protocolMessagesNode.replaceChildren(...messageRows);
  protocolMessagesNode.scrollTop = protocolMessagesNode.scrollHeight;
  showViewportMessage(auth);

  const entitlementRows = auth.entitlements.length
    ? auth.entitlements.slice().reverse().map((entitlement) => {
        const row = document.createElement("div");
        row.className = "entitlement-row";
        [entitlement.action, entitlement.rule_id, entitlement.stage, `${entitlement.ttl_s}s`, entitlement.status].forEach((value, index) => {
          const cell = document.createElement("span");
          cell.textContent = value;
          if (index === 4) cell.className = entitlement.status;
          if (index === 0) cell.title = entitlement.id;
          row.append(cell);
        });
        return row;
      })
    : [Object.assign(document.createElement("p"), { className: "empty", textContent: "No entitlements issued" })];
  entitlementsNode.replaceChildren(...entitlementRows);
}

function render(data) {
  stateNode.textContent = data.state;
  rangeNode.textContent = Number(data.range_m).toFixed(3);
  const progress = Math.max(0, Math.min(1, (3.32 - data.range_m) / 3.32));
  const isMobile = window.matchMedia("(max-width: 760px)").matches;
  let chaserPosition = `${7 + progress * 57}%`;
  if (isMobile) {
    const viewportRect = chaser.parentElement.getBoundingClientRect();
    const targetRect = target.getBoundingClientRect();
    const startPosition = viewportRect.width * 0.07;
    const dockPosition = targetRect.left - viewportRect.left
      - chaser.getBoundingClientRect().width - viewportRect.width * 0.013;
    chaserPosition = `${startPosition + progress * (dockPosition - startPosition)}px`;
  }
  chaser.style.left = chaserPosition;
  chaserMessageNode.style.left = chaserPosition;
  chaserConfig.style.left = isMobile ? "9%" : chaserPosition;
  chaserMessageNode.style.setProperty("--message-anchor", isMobile ? `${5 + progress * 55}%` : "18%");
  chaserConfig.style.setProperty("--config-anchor", `${isMobile ? 18 + progress * 64 : 10 + progress * 44}%`);
  stages.forEach((stage) => {
    const stageId = Number(stage.dataset.state);
    stage.classList.toggle("done", stageId < data.state_id);
    stage.classList.toggle("active", stageId === data.state_id);
  });
  const eventRows = data.events.length
    ? data.events.slice().reverse().map((event) => {
        const row = document.createElement("p");
        row.className = `event ${event.kind}`;
        row.textContent = event.text;
        return row;
      })
    : [Object.assign(document.createElement("p"), {
        className: "empty",
        textContent: "Waiting for transition activity",
      })];
  eventsNode.replaceChildren(...eventRows);
  renderAuthorization(data.authorization, data);
}

function replayFrame(index) {
  if (!replayHistory.length) return;
  replayMode = true;
  const boundedIndex = Math.max(0, Math.min(replayHistory.length - 1, index));
  replayTimeline.value = boundedIndex;
  render(replayHistory[boundedIndex]);
  replayLabel.textContent = `Frame ${boundedIndex + 1} / ${replayHistory.length}`;
}

function stopReplay() {
  clearInterval(replayTimer);
  replayTimer = null;
  replayPlay.textContent = "▶";
  replayPlay.title = "Play replay";
  replayPlay.setAttribute("aria-label", "Play replay");
}

function enableReplay(outcome) {
  replayControls.classList.add("available");
  [replayTimeline, replayBack, replayPlay, replayForward, replayLive].forEach((control) => { control.disabled = false; });
  replayTimeline.max = Math.max(0, replayHistory.length - 1);
  if (!replayMode) {
    replayTimeline.value = replayHistory.length - 1;
    replayLabel.textContent = `${replayHistory.length} frames · ${outcome}`;
  }
}

function captureSnapshot(data) {
  latestLiveSnapshot = data;
  const signature = `${data.state_id}:${data.range_m}:${data.authorization.events.length}:${data.authorization.entitlements.length}`;
  if (signature !== replaySignature) {
    replayHistory.push(JSON.parse(JSON.stringify(data)));
    replaySignature = signature;
  }
  const denied = data.authorization.events.some((event) => event.code?.startsWith("DENY_"));
  if (denied) enableReplay("authorization denied");
  else if (data.state === "HARD DOCK" && Number(data.range_m) <= 0.005) enableReplay("hard dock complete");
}

async function refresh() {
  try {
    const response = await fetch("/api/state", { cache: "no-store" });
    const data = await response.json();
    if (awaitingReset) {
      const denialCleared = !data.authorization.events.some((event) => event.code?.startsWith("DENY_"));
      const resetConfirmed = data.state === "HOLD"
        && Number(data.range_m) >= 3.319
        && data.authorization.entitlements.length === 0
        && denialCleared
        && data.authorization.scenario_id === scenarioSelect.value;
      if (!resetConfirmed) {
        render(data);
        return;
      }
      awaitingReset = false;
      replayHistory = [];
      replaySignature = "";
    }
    captureSnapshot(data);
    if (!replayMode) render(data);
  } catch (_) {
    stateNode.textContent = "DISCONNECTED";
  }
}

async function rerun() {
  stopReplay();
  awaitingReset = true;
  replayHistory = [];
  replaySignature = "";
  replayMode = false;
  replayControls.classList.remove("available");
  [replayTimeline, replayBack, replayPlay, replayForward, replayLive].forEach((control) => { control.disabled = true; });
  replayLabel.textContent = "Replay available after terminal outcome";
  rerunButton.disabled = true;
  rerunButton.textContent = "Resetting";
  try {
    const response = await fetch("/api/rerun", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ scenario: scenarioSelect.value }),
    });
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    await refresh();
  } catch (_) {
    awaitingReset = false;
    stateNode.textContent = "RESET FAILED";
  } finally {
    rerunButton.disabled = false;
    rerunButton.textContent = "Run validation";
  }
}

setInterval(refresh, 150);
window.addEventListener("pagehide", () => {
  navigator.sendBeacon("/gateway/release");
});
rerunButton.addEventListener("click", rerun);
bindCraftConfig(chaser, chaserConfig);
bindCraftConfig(target, targetConfig);
document.querySelectorAll("[data-close-config]").forEach((button) => {
  button.addEventListener("click", () => setConfig(null, null));
});
document.addEventListener("keydown", (event) => {
  if (event.key === "Escape") setConfig(null, null);
});
replayTimeline.addEventListener("input", () => { stopReplay(); replayFrame(Number(replayTimeline.value)); });
replayBack.addEventListener("click", () => { stopReplay(); replayFrame(Number(replayTimeline.value) - 1); });
replayForward.addEventListener("click", () => { stopReplay(); replayFrame(Number(replayTimeline.value) + 1); });
replayPlay.addEventListener("click", () => {
  if (replayTimer) { stopReplay(); return; }
  if (Number(replayTimeline.value) >= replayHistory.length - 1) replayFrame(0);
  replayPlay.textContent = "Ⅱ";
  replayPlay.title = "Pause replay";
  replayPlay.setAttribute("aria-label", "Pause replay");
  replayTimer = setInterval(() => {
    const nextIndex = Number(replayTimeline.value) + 1;
    if (nextIndex >= replayHistory.length) { stopReplay(); return; }
    replayFrame(nextIndex);
  }, 90);
});
replayLive.addEventListener("click", () => {
  stopReplay();
  replayMode = false;
  displayedMessageKey = "";
  if (latestLiveSnapshot) render(latestLiveSnapshot);
  replayTimeline.value = replayHistory.length - 1;
  replayLabel.textContent = `${replayHistory.length} recorded frames`;
});
refresh();