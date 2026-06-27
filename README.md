# Claude Status Widget

A tiny, discreet floating macOS widget that shows your Claude usage — **Current
session** (5-hour rolling window) and **Weekly limits** — mirroring what Claude
Code's `/usage` screen displays.

Built with [Tauri 2](https://tauri.app) (Rust backend + vanilla TS frontend).

![two progress bars: Current session and Weekly limits]

## How it works

1. **Token source — macOS Keychain.** The live OAuth access token is read from
   the Keychain entry `Claude Code-credentials` (the one Claude Code keeps fresh).
   We piggyback on Claude Code's own token refresh, so the widget needs no login
   flow of its own.
   > Note: `~/.claude/.credentials.json` exists but its token is often **stale**
   > on macOS — Claude Code stores the live credential in the Keychain. That's why
   > we read the Keychain, not the file.
2. **Usage data — undocumented OAuth endpoint.** The Rust backend calls
   `GET https://api.anthropic.com/api/oauth/usage` with the token and parses
   `five_hour` / `seven_day` (utilization 0–100 + `resets_at`).
3. **UI.** Two progress bars + reset countdown, polled every 60s.

### Important caveats

- **This is unofficial.** The usage endpoint is undocumented and using the Claude
  Code OAuth token from a third-party tool is against Anthropic's ToS. It's used
  here for personal, local, read-only monitoring with your own token.
- **Rate limiting.** The endpoint returns `429` aggressively. Two mitigations:
  - The request **must** send `User-Agent: claude-code/<version>` or it lands in a
    harshly throttled bucket.
  - Poll slowly (60s) and back off to 5 min on `429`. Do **not** poll fast.
- The `version` in the User-Agent should track your installed Claude Code
  (`claude --version`); it lives in `src-tauri/src/lib.rs` as `CLAUDE_CODE_VERSION`.

## Develop

```bash
bun install
bun run tauri dev
```

First run compiles the Rust toolchain deps (a few minutes). On first launch macOS
will prompt for Keychain access — click **Always Allow**.

## Build

```bash
bun run tauri build
```

## Project layout

- `src/` — frontend (HTML/CSS/TS): the two progress bars and polling loop.
- `src-tauri/src/lib.rs` — the `get_usage` command (Keychain read + endpoint call).
- `src-tauri/tauri.conf.json` — the floating window config (frameless, transparent,
  always-on-top, no Dock icon).
