// Claude usage widget — menu-bar app.
// A background thread polls the undocumented usage endpoint (token from the macOS
// Keychain) and drives the tray: title = current-session %, icon = a depleting
// gauge of time remaining in the 5h session. The window is a hover popover.
use serde::Serialize;
use std::process::Command;
use std::sync::Mutex;
use std::time::Duration;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition};
use tauri_plugin_autostart::{ManagerExt, MacosLauncher};

const KEYCHAIN_SERVICE: &str = "Claude Code-credentials";
const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const CLAUDE_CODE_VERSION: &str = "2.1.195";
const SESSION_WINDOW_MIN: f64 = 300.0; // 5h rolling window

/// Depleting-gauge frames (frame 0 = nearly empty .. frame 12 = full), in three
/// color sets: mono (template, normal), yellow (ahead of pace), red (will block).
macro_rules! gauge_set {
    ($dir:literal) => {
        [
            include_bytes!(concat!("../icons/gauge/", $dir, "/f00.png")),
            include_bytes!(concat!("../icons/gauge/", $dir, "/f01.png")),
            include_bytes!(concat!("../icons/gauge/", $dir, "/f02.png")),
            include_bytes!(concat!("../icons/gauge/", $dir, "/f03.png")),
            include_bytes!(concat!("../icons/gauge/", $dir, "/f04.png")),
            include_bytes!(concat!("../icons/gauge/", $dir, "/f05.png")),
            include_bytes!(concat!("../icons/gauge/", $dir, "/f06.png")),
            include_bytes!(concat!("../icons/gauge/", $dir, "/f07.png")),
            include_bytes!(concat!("../icons/gauge/", $dir, "/f08.png")),
            include_bytes!(concat!("../icons/gauge/", $dir, "/f09.png")),
            include_bytes!(concat!("../icons/gauge/", $dir, "/f10.png")),
            include_bytes!(concat!("../icons/gauge/", $dir, "/f11.png")),
            include_bytes!(concat!("../icons/gauge/", $dir, "/f12.png")),
        ]
    };
}
const MONO: [&[u8]; 13] = gauge_set!("mono");
const YELLOW: [&[u8]; 13] = gauge_set!("yellow");
const RED: [&[u8]; 13] = gauge_set!("red");

#[derive(Clone, Copy, PartialEq)]
enum Level {
    Green,
    Yellow,
    Red,
}

/// The core insight: danger is being AHEAD OF PACE, not raw usage. Compare how
/// much of the quota you've used vs how much of the window has elapsed.
/// deficit = used − elapsed. Big positive deficit => you'll exhaust before reset.
/// Floor: below 50% used there's plenty of absolute headroom, so stay green.
fn danger_level(used_pct: f64, remaining_frac: f64) -> Level {
    let used = used_pct / 100.0;
    let elapsed = 1.0 - remaining_frac;
    if used < 0.50 {
        return Level::Green;
    }
    let deficit = used - elapsed;
    if deficit >= 0.30 {
        Level::Red
    } else if deficit >= 0.15 {
        Level::Yellow
    } else {
        Level::Green
    }
}

#[derive(Serialize, Clone)]
struct Window {
    utilization: f64, // 0-100
    resets_at: Option<String>,
}

#[derive(Serialize, Clone)]
struct Usage {
    five_hour: Option<Window>,
    seven_day: Option<Window>,
    seven_day_opus: Option<Window>,
    seven_day_sonnet: Option<Window>,
    fetched_at_ms: u128,
}

struct AppState {
    usage: Mutex<Result<Usage, String>>,
    icon_rect: Mutex<Option<[f64; 4]>>, // x, y, w, h in physical px
}

// ---------- data fetching ----------

fn read_keychain_token() -> Result<String, String> {
    let out = Command::new("security")
        .args(["find-generic-password", "-s", KEYCHAIN_SERVICE, "-w"])
        .output()
        .map_err(|e| format!("KEYCHAIN_SPAWN: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "NO_TOKEN: keychain entry '{KEYCHAIN_SERVICE}' not readable ({})",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let blob = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value =
        serde_json::from_str(blob.trim()).map_err(|e| format!("BAD_KEYCHAIN_JSON: {e}"))?;
    v.get("claudeAiOauth")
        .and_then(|o| o.get("accessToken"))
        .or_else(|| v.get("accessToken"))
        .and_then(|t| t.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "NO_TOKEN: keychain blob has no accessToken".to_string())
}

fn fetch_raw(token: &str) -> Result<(u16, String), String> {
    let out = Command::new("curl")
        .args([
            "-s",
            "--max-time",
            "15",
            "-w",
            "\n%{http_code}",
            "-H",
            &format!("Authorization: Bearer {token}"),
            "-H",
            "anthropic-beta: oauth-2025-04-20",
            "-H",
            &format!("User-Agent: claude-code/{CLAUDE_CODE_VERSION}"),
            "-H",
            "Content-Type: application/json",
            USAGE_URL,
        ])
        .output()
        .map_err(|e| format!("CURL_SPAWN: {e}"))?;
    let combined = String::from_utf8_lossy(&out.stdout).to_string();
    let idx = combined
        .rfind('\n')
        .ok_or_else(|| "CURL_NO_OUTPUT: empty response".to_string())?;
    let status: u16 = combined[idx + 1..].trim().parse().unwrap_or(0);
    Ok((status, combined[..idx].to_string()))
}

fn parse_window(v: &serde_json::Value, key: &str) -> Option<Window> {
    let o = v.get(key)?;
    if o.is_null() {
        return None;
    }
    let u = o.get("utilization")?.as_f64()?;
    let utilization = if u <= 1.0 { u * 100.0 } else { u };
    Some(Window {
        utilization,
        resets_at: o
            .get("resets_at")
            .and_then(|r| r.as_str())
            .map(|s| s.to_string()),
    })
}

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

fn fetch_usage_data() -> Result<Usage, String> {
    let token = read_keychain_token()?;
    let (status, body) = fetch_raw(&token)?;
    match status {
        200 => {
            let v: serde_json::Value =
                serde_json::from_str(&body).map_err(|e| format!("BAD_USAGE_JSON: {e}"))?;
            Ok(Usage {
                five_hour: parse_window(&v, "five_hour"),
                seven_day: parse_window(&v, "seven_day"),
                seven_day_opus: parse_window(&v, "seven_day_opus"),
                seven_day_sonnet: parse_window(&v, "seven_day_sonnet"),
                fetched_at_ms: now_ms(),
            })
        }
        429 => Err("RATE_LIMITED: usage endpoint is throttling".to_string()),
        401 | 403 => Err("UNAUTHORIZED: reopen Claude Code".to_string()),
        s => Err(format!("HTTP_{s}: {}", body.chars().take(160).collect::<String>())),
    }
}

// ---------- tray ----------

/// Parse an RFC3339 UTC timestamp (the endpoint always returns +00:00) to epoch
/// seconds, without pulling a date crate. Uses Howard Hinnant's days-from-civil.
fn rfc3339_to_epoch(s: &str) -> Option<i64> {
    let year: i64 = s.get(0..4)?.parse().ok()?;
    let mon: i64 = s.get(5..7)?.parse().ok()?;
    let day: i64 = s.get(8..10)?.parse().ok()?;
    let hh: i64 = s.get(11..13)?.parse().ok()?;
    let mm: i64 = s.get(14..16)?.parse().ok()?;
    let ss: i64 = s.get(17..19)?.parse().ok()?;
    let y = if mon <= 2 { year - 1 } else { year };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if mon > 2 { mon - 3 } else { mon + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    Some(days * 86400 + hh * 3600 + mm * 60 + ss)
}

/// Fraction (0-1) of the session window still remaining.
fn remaining_fraction(resets_at: &str) -> f64 {
    match rfc3339_to_epoch(resets_at) {
        Some(target) => {
            let now = (now_ms() / 1000) as i64;
            ((target - now) as f64 / (SESSION_WINDOW_MIN * 60.0)).clamp(0.0, 1.0)
        }
        None => 1.0,
    }
}

fn update_tray_ui(app: &AppHandle, result: &Result<Usage, String>, force: Option<Level>) {
    let Some(tray) = app.tray_by_id("main-tray") else {
        return;
    };
    match result {
        Ok(u) => match u.five_hour.as_ref() {
            Some(w) => {
                let frac = w
                    .resets_at
                    .as_ref()
                    .map(|r| remaining_fraction(r))
                    .unwrap_or(1.0);
                let level = force.unwrap_or_else(|| danger_level(w.utilization, frac));
                // icon: color set by danger level, frame by remaining time
                let idx = (frac * 12.0).round().clamp(0.0, 12.0) as usize;
                let (set, is_template): (&[&[u8]; 13], bool) = match level {
                    Level::Green => (&MONO, true),
                    Level::Yellow => (&YELLOW, false),
                    Level::Red => (&RED, false),
                };
                if let Ok(img) = tauri::image::Image::from_bytes(set[idx]) {
                    let _ = tray.set_icon(Some(img));
                    let _ = tray.set_icon_as_template(is_template);
                }
                // title is just the % — the gauge color carries the urgency
                let _ = tray.set_title(Some(format!("{}%", w.utilization.round() as i64)));
            }
            None => {
                let _ = tray.set_title(Some("—".to_string()));
            }
        },
        Err(e) => {
            let code = e.split(':').next().unwrap_or("");
            let t = if code == "RATE_LIMITED" { "…" } else { "!" };
            let _ = tray.set_title(Some(t.to_string()));
        }
    }
}

fn point_in(px: f64, py: f64, x: f64, y: f64, w: f64, h: f64, pad: f64) -> bool {
    px >= x - pad && px <= x + w + pad && py >= y - pad && py <= y + h + pad
}

// Anchor the popover to the tray ICON's screen rect (fixed), not the cursor, so
// it always opens in the same spot. Also stash the icon rect for the hover monitor.
fn show_popover(app: &AppHandle, rect: tauri::Rect) {
    if let Some(w) = app.get_webview_window("main") {
        let scale = w.scale_factor().unwrap_or(2.0);
        let pos = rect.position.to_physical::<f64>(scale);
        let isize = rect.size.to_physical::<f64>(scale);
        if let Some(st) = app.try_state::<AppState>() {
            *st.icon_rect.lock().unwrap() = Some([pos.x, pos.y, isize.width, isize.height]);
        }
        if let Ok(win) = w.outer_size() {
            let icon_cx = pos.x + isize.width / 2.0;
            let icon_bottom = pos.y + isize.height;
            let x = (icon_cx - win.width as f64 / 2.0).max(6.0);
            let y = icon_bottom + 2.0; // just below the menu bar
            let _ = w.set_position(PhysicalPosition::new(x, y));
        }
        let _ = w.show();
        let _ = w.emit("popover", "enter");
    }
}

fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let autostart_on = app.autolaunch().is_enabled().unwrap_or(false);

    let auto_i = CheckMenuItem::with_id(
        app,
        "autostart",
        "Launch at login",
        true,
        autostart_on,
        None::<&str>,
    )?;
    let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&auto_i, &PredefinedMenuItem::separator(app)?, &quit_i])?;

    let icon = tauri::image::Image::from_bytes(MONO[12]).expect("valid frame");
    let auto_for_cb = auto_i.clone();

    TrayIconBuilder::with_id("main-tray")
        .icon(icon)
        .icon_as_template(true)
        .title("…")
        .menu(&menu)
        .show_menu_on_left_click(false) // hover shows popover; right-click opens menu
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "quit" => app.exit(0),
            "autostart" => {
                let mgr = app.autolaunch();
                let now_on = mgr.is_enabled().unwrap_or(false);
                let _ = if now_on { mgr.disable() } else { mgr.enable() };
                let _ = auto_for_cb.set_checked(!now_on);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            let app = tray.app_handle();
            match event {
                // Show on Enter/Move. HIDING is owned by the hover-monitor thread
                // (geometry-based), because macOS Enter/Leave are unreliable and a
                // stationary cursor emits no events. Click does nothing.
                TrayIconEvent::Enter { rect, .. } => show_popover(app, rect),
                TrayIconEvent::Move { rect, .. } => {
                    let visible = app
                        .get_webview_window("main")
                        .and_then(|w| w.is_visible().ok())
                        .unwrap_or(false);
                    if !visible {
                        show_popover(app, rect);
                    }
                }
                _ => {}
            }
        })
        .build(app)?;
    Ok(())
}

// ---------- commands ----------

/// Returns the cached usage (the background thread keeps it fresh).
#[tauri::command]
fn get_usage(state: tauri::State<AppState>) -> Result<Usage, String> {
    state.usage.lock().unwrap().clone()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, None))
        .manage(AppState {
            usage: Mutex::new(Err("LOADING".to_string())),
            icon_rect: Mutex::new(None),
        })
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.handle()
                .set_activation_policy(tauri::ActivationPolicy::Accessory)?;
            setup_tray(app)?;

            // Background poller — independent of window visibility.
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let mut demo_i = 0usize;
                loop {
                    // Debug/preview hook. WIDGET_FORCE_LEVEL=demo cycles the colors
                    // every 2s (no network); =green|yellow|red pins one level.
                    let forced = std::env::var("WIDGET_FORCE_LEVEL").ok();
                    if forced.as_deref() == Some("demo") {
                        let lvl = [Level::Green, Level::Yellow, Level::Red][demo_i % 3];
                        demo_i += 1;
                        let fake = Ok(Usage {
                            five_hour: Some(Window {
                                utilization: 88.0,
                                resets_at: None,
                            }),
                            seven_day: None,
                            seven_day_opus: None,
                            seven_day_sonnet: None,
                            fetched_at_ms: now_ms(),
                        });
                        update_tray_ui(&handle, &fake, Some(lvl));
                        std::thread::sleep(Duration::from_secs(2));
                        continue;
                    }

                    let result = fetch_usage_data();
                    if let Some(state) = handle.try_state::<AppState>() {
                        *state.usage.lock().unwrap() = result.clone();
                    }
                    let force = match forced.as_deref() {
                        Some("green") => Some(Level::Green),
                        Some("yellow") => Some(Level::Yellow),
                        Some("red") => Some(Level::Red),
                        _ => None,
                    };
                    update_tray_ui(&handle, &result, force);
                    let secs = match &result {
                        Ok(_) => 60,
                        Err(e) if e.starts_with("RATE_LIMITED") => 300,
                        Err(_) => 120,
                    };
                    std::thread::sleep(Duration::from_secs(secs));
                }
            });

            // Hover monitor — geometry-based hide. Polls the real cursor position
            // and hides the popover once it's outside BOTH the icon and the panel.
            // Robust to flaky tray events and a stationary cursor (no events).
            let hover = app.handle().clone();
            std::thread::spawn(move || {
                let mut out = 0u32;
                let mut emitted = false;
                loop {
                    std::thread::sleep(Duration::from_millis(90));
                    let Some(w) = hover.get_webview_window("main") else {
                        continue;
                    };
                    if !w.is_visible().unwrap_or(false) {
                        out = 0;
                        emitted = false;
                        continue;
                    }
                    let Ok(cursor) = hover.cursor_position() else {
                        continue;
                    };
                    let icon = hover
                        .try_state::<AppState>()
                        .and_then(|s| *s.icon_rect.lock().unwrap());
                    let in_icon = icon
                        .map(|r| point_in(cursor.x, cursor.y, r[0], r[1], r[2], r[3], 8.0))
                        .unwrap_or(false);
                    let in_win = (|| {
                        let p = w.outer_position().ok()?;
                        let s = w.outer_size().ok()?;
                        Some(point_in(
                            cursor.x, cursor.y, p.x as f64, p.y as f64, s.width as f64,
                            s.height as f64, 6.0,
                        ))
                    })()
                    .unwrap_or(false);
                    if in_icon || in_win {
                        out = 0;
                        emitted = false;
                    } else {
                        out += 1;
                        if out >= 2 && !emitted {
                            emitted = true;
                            let _ = w.emit("popover", "leave");
                        }
                    }
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![get_usage])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
