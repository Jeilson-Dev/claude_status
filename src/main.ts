import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";

type UsageWindow = {
  utilization: number; // 0-100
  resets_at: string | null;
};

type Usage = {
  five_hour: UsageWindow | null;
  seven_day: UsageWindow | null;
  seven_day_opus: UsageWindow | null;
  seven_day_sonnet: UsageWindow | null;
  fetched_at_ms: number;
};

const FADE_MS = 150;

const $ = (id: string) => document.getElementById(id)!;

let shownState = false;
let refreshTimer: number | null = null;
let pendingHide: number | null = null;

/** "Resets in 1 hr 53 min" from an ISO timestamp. */
function formatReset(iso: string | null): string {
  if (!iso) return "";
  const ms = new Date(iso).getTime() - Date.now();
  if (Number.isNaN(ms)) return "";
  if (ms <= 0) return "Resets now";
  const totalMin = Math.floor(ms / 60_000);
  const h = Math.floor(totalMin / 60);
  const m = totalMin % 60;
  if (h >= 24) return `Resets in ${Math.floor(h / 24)}d ${h % 24}h`;
  if (h > 0) return `Resets in ${h} hr ${m} min`;
  return `Resets in ${m} min`;
}

type Level = "" | "warn" | "danger";

function usageLevel(used: number): Level {
  if (used >= 90) return "danger";
  if (used >= 75) return "warn";
  return "";
}

// Pace-based: danger is being AHEAD OF PACE, not raw usage.
// deficit = used − elapsed; floor at 50% used (plenty of headroom below that).
function paceLevel(used: number, remainingFrac: number): Level {
  if (used < 50) return "";
  const deficit = used / 100 - (1 - remainingFrac);
  if (deficit >= 0.3) return "danger";
  if (deficit >= 0.15) return "warn";
  return "";
}

function paintWindow(
  prefix: string,
  w: UsageWindow | null,
  windowMin: number | null,
) {
  const pct = $(`${prefix}-pct`);
  const fill = $(`${prefix}-fill`) as HTMLElement;
  const sub = $(`${prefix}-sub`);
  const setLevel = (lvl: Level) => {
    for (const el of [pct, fill]) {
      el.classList.toggle("warn", lvl === "warn");
      el.classList.toggle("danger", lvl === "danger");
    }
  };
  if (!w) {
    pct.textContent = "—";
    fill.style.width = "0%";
    sub.textContent = "no active window";
    setLevel("");
    return;
  }
  const u = Math.max(0, Math.min(100, Math.round(w.utilization)));
  pct.textContent = `${u}% used`;
  fill.style.width = `${u}%`;
  sub.textContent = formatReset(w.resets_at);
  let lvl: Level;
  if (windowMin != null && w.resets_at) {
    const remMin = (new Date(w.resets_at).getTime() - Date.now()) / 60000;
    const remFrac = Math.max(0, Math.min(1, remMin / windowMin));
    lvl = paceLevel(u, remFrac);
  } else {
    lvl = usageLevel(u);
  }
  setLevel(lvl);
}

function showError(msg: string) {
  const code = msg.split(":")[0];
  const human: Record<string, string> = {
    RATE_LIMITED: "Rate limited — retrying soon",
    LOADING: "Loading…",
    NO_TOKEN: "Open Claude Code to sign in",
    UNAUTHORIZED: "Token expired — reopen Claude Code",
  };
  $("session-pct").textContent = code === "LOADING" ? "" : "!";
  $("weekly-pct").textContent = "";
  $("session-sub").textContent = human[code] ?? "Couldn't read usage";
  $("weekly-sub").textContent = "";
}

async function render() {
  try {
    const u = await invoke<Usage>("get_usage");
    paintWindow("session", u.five_hour, 300); // 5h session window → pace coloring
    paintWindow("weekly", u.seven_day, null); // weekly → simple usage coloring
  } catch (e) {
    showError(typeof e === "string" ? e : String(e));
  }
}

// ---- fade + visibility (hover state machine) ----
// macOS tray Enter/Leave events are unreliable, so we treat every Enter/Move as a
// liveness signal and run a watchdog that hides once the signal goes stale.

function fadeIn() {
  if (pendingHide) {
    clearTimeout(pendingHide);
    pendingHide = null;
  }
  document.body.classList.remove("hiding");
  requestAnimationFrame(() => document.body.classList.add("shown"));
}

function fadeOutAndHide() {
  document.body.classList.remove("shown");
  document.body.classList.add("hiding");
  if (pendingHide) clearTimeout(pendingHide);
  pendingHide = window.setTimeout(() => {
    getCurrentWindow().hide();
    pendingHide = null;
  }, FADE_MS);
}

// Snap both bars back to 0 with no transition, so the next render animates
// them filling from 0 — replaying the first-open animation on every open.
function resetBars() {
  for (const p of ["session", "weekly"]) {
    const fill = $(`${p}-fill`) as HTMLElement;
    fill.style.transition = "none";
    fill.style.width = "0%";
    fill.classList.remove("warn", "danger");
    $(`${p}-pct`).classList.remove("warn", "danger");
  }
  // commit the 0% width before re-enabling the transition
  void document.documentElement.offsetHeight;
  for (const p of ["session", "weekly"]) {
    ($(`${p}-fill`) as HTMLElement).style.transition = "";
  }
}

function ensureShown() {
  if (shownState) return;
  shownState = true;
  resetBars();
  render(); // async: real widths a tick later → bars animate 0→value
  fadeIn();
  if (!refreshTimer) refreshTimer = window.setInterval(render, 30_000);
}

function hideNow() {
  if (!shownState) return;
  shownState = false;
  fadeOutAndHide();
  if (refreshTimer) {
    clearInterval(refreshTimer);
    refreshTimer = null;
  }
}

window.addEventListener("DOMContentLoaded", () => {
  // Visibility is driven from Rust: "enter" when the cursor reaches the icon,
  // "leave" when the hover monitor sees it leave both the icon and the panel.
  listen<string>("popover", (e) => {
    if (e.payload === "leave") hideNow();
    else ensureShown();
  });
});
