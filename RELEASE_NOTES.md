# Claude Usage Monitor — v0.1.0

First release. A tiny macOS **menu-bar** app that shows your Claude usage at a
glance — no Dock icon, no window in the way.

## ✨ Highlights

- **Always-on glance.** Your current-session **%** sits right in the menu bar next
  to a gauge icon.
- **Smart, pace-aware colors.** The color tells you whether you're on track to get
  blocked *before* the session resets — not just how much you've used.
- **Hover for details.** A clean popover with both usage bars and reset countdowns.

## 🧭 What it shows

### In the menu bar
- **Title** = current session usage (the 5-hour rolling window), e.g. `49%`.
- **Icon** = a gauge that **depletes as the session nears its reset** (time left).
- **Color = pace, not raw usage:** 🟢 **Green** on pace · 🟡 **Yellow** ahead of
  pace · 🔴 **Red** burning fast (you'll likely hit the limit before reset).

  > *90% used with 10% time left* → 🟢 (resets soon, relax).
  > *78% used but the session barely started* → 🔴 (rethink usage or you'll block).

### In the hover panel
- Two progress bars — **Current session (5h)** and **Weekly limits** — each with a
  "Resets in …" countdown.
- Fades in/out; the bars animate filling on every open.

## ⚙️ Under the hood

- Reads your OAuth token from the **macOS Keychain** (piggybacks on Claude Code's
  own refresh — no separate login).
- A background poller keeps the menu bar fresh **even while the panel is hidden**,
  and is **rate-limit aware** (slow polling + backoff).
- **Launch at login** toggle (right-click the icon). Accessory app — no Dock icon.

## 💻 Requirements

macOS (Apple Silicon) with **Claude Code installed and logged in** — the token
comes from its Keychain entry.

## 📦 Install

1. Open `Claude Usage Monitor.app` (from the `.dmg`, drag it onto Applications).
2. It's **ad-hoc signed (not notarized)** — if Gatekeeper blocks it, right-click →
   **Open** the first time.
3. On first launch, approve the Keychain prompt (**Always Allow**).

## ⚠️ Known limitations

- The **Weekly** bar colors by raw usage (its reset-window length isn't exposed by
  the API).
- Not code-signed / notarized.
- No per-model (Opus / Sonnet) breakdown yet.

## 🔒 Heads-up

This relies on an **undocumented** endpoint and the Claude Code OAuth token. It's
for **personal, local, read-only** monitoring with your own account, and is
**against Anthropic's ToS** for third-party tools — use at your own discretion.

<sub>Dev tip: set `WIDGET_FORCE_LEVEL=green|yellow|red|demo` to preview the color
states (`demo` cycles them).</sub>
