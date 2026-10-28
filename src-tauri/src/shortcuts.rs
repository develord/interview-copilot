//! Global keyboard shortcuts for the overlay.
//!
//! Shortcuts are registered natively rather than from the webview so that show/hide keeps
//! working while the overlay is hidden. Each action has a preferred binding plus fallbacks:
//! when another application already owns a binding, the next candidate is tried, and the
//! outcome (including conflicts) is reported to the UI through `shortcut_status`.

use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime, WebviewWindow};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

pub const OVERLAY_LABEL: &str = "overlay";
/// Event sent to the webview for actions it handles itself. Payload: kebab-case action name.
pub const SHORTCUT_EVENT: &str = "overlay:shortcut";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ShortcutAction {
    ToggleOverlay,
    ToggleCapture,
    SubmitManual,
    EnlargeActive,
}

impl ShortcutAction {
    pub const ALL: [ShortcutAction; 4] = [
        Self::ToggleOverlay,
        Self::ToggleCapture,
        Self::SubmitManual,
        Self::EnlargeActive,
    ];

    /// Preferred binding first, then fallbacks used when the preferred one is taken.
    pub fn candidates(self) -> &'static [&'static str] {
        match self {
            Self::ToggleOverlay => &["Ctrl+Shift+Space", "Ctrl+Alt+Space"],
            Self::ToggleCapture => &["Ctrl+Shift+P", "Ctrl+Alt+P"],
            Self::SubmitManual => &["Ctrl+Shift+Enter", "Ctrl+Alt+Enter"],
            Self::EnlargeActive => &["Ctrl+Shift+Up", "Ctrl+Alt+Up"],
        }
    }

    /// Actions that need the overlay visible and focused (e.g. to type a manual question).
    fn needs_focus(self) -> bool {
        matches!(self, Self::SubmitManual | Self::EnlargeActive)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutBinding {
    pub action: ShortcutAction,
    pub preferred: String,
    /// Accelerator actually registered, or `None` when every candidate failed.
    pub accelerator: Option<String>,
    /// Why the preferred binding (or every binding) could not be registered.
    pub error: Option<String>,
}

#[derive(Default)]
pub struct ShortcutRegistry {
    active: Mutex<Vec<(Shortcut, ShortcutAction)>>,
    status: Mutex<Vec<ShortcutBinding>>,
}

/// Tries each candidate in order until one registers. Pure apart from `try_register`, so the
/// conflict/fallback policy is unit-testable without touching the OS hotkey API.
pub fn bind_first<T>(
    action: ShortcutAction,
    mut try_register: impl FnMut(&str) -> Result<T, String>,
) -> (ShortcutBinding, Option<T>) {
    let candidates = action.candidates();
    let preferred = candidates[0].to_string();
    let mut errors = Vec::new();

    for candidate in candidates {
        match try_register(candidate) {
            Ok(value) => {
                let binding = ShortcutBinding {
                    action,
                    preferred,
                    accelerator: Some((*candidate).to_string()),
                    error: errors.into_iter().next(),
                };
                return (binding, Some(value));
            }
            Err(error) => errors.push(format!("{candidate}: {error}")),
        }
    }

    let binding = ShortcutBinding {
        action,
        preferred,
        accelerator: None,
        error: Some(errors.join("; ")),
    };
    (binding, None)
}

pub fn plugin<R: Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, shortcut, event| {
            if event.state() != ShortcutState::Pressed {
                return;
            }
            let Some(registry) = app.try_state::<ShortcutRegistry>() else {
                return;
            };
            let action = registry.active.lock().ok().and_then(|active| {
                active
                    .iter()
                    .find(|(registered, _)| registered == shortcut)
                    .map(|(_, action)| *action)
            });
            if let Some(action) = action {
                handle(app, action);
            }
        })
        .build()
}

pub fn register_defaults<R: Runtime>(app: &AppHandle<R>) {
    let mut active = Vec::new();
    let mut status = Vec::new();

    for action in ShortcutAction::ALL {
        let (binding, shortcut) = bind_first(action, |accelerator| {
            let shortcut: Shortcut = accelerator.parse().map_err(|e| format!("{e}"))?;
            app.global_shortcut()
                .register(shortcut)
                .map_err(|e| e.to_string())?;
            Ok(shortcut)
        });
        if let Some(shortcut) = shortcut {
            active.push((shortcut, action));
        }
        if let Some(error) = &binding.error {
            eprintln!("[shortcuts] {action:?}: {error}");
        }
        status.push(binding);
    }

    let registry = app.state::<ShortcutRegistry>();
    if let Ok(mut guard) = registry.active.lock() {
        *guard = active;
    }
    if let Ok(mut guard) = registry.status.lock() {
        *guard = status;
    }
}

fn handle<R: Runtime>(app: &AppHandle<R>, action: ShortcutAction) {
    let Some(window) = app.get_webview_window(OVERLAY_LABEL) else {
        return;
    };
    match action {
        ShortcutAction::ToggleOverlay => {
            let shown =
                window.is_visible().unwrap_or(false) && !window.is_minimized().unwrap_or(false);
            if shown {
                let _ = window.hide();
            } else {
                reveal(&window);
            }
        }
        other => {
            if other.needs_focus() {
                reveal(&window);
            }
            let _ = app.emit(SHORTCUT_EVENT, other);
        }
    }
}

fn reveal<R: Runtime>(window: &WebviewWindow<R>) {
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_focus();
}

#[tauri::command]
pub fn shortcut_status(registry: tauri::State<'_, ShortcutRegistry>) -> Vec<ShortcutBinding> {
    registry
        .status
        .lock()
        .map(|status| status.clone())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_candidate_accelerator_parses() {
        for action in ShortcutAction::ALL {
            for candidate in action.candidates() {
                assert!(
                    candidate.parse::<Shortcut>().is_ok(),
                    "{candidate} should be a valid accelerator"
                );
            }
        }
    }

    #[test]
    fn candidates_are_unique_across_actions() {
        let mut all: Vec<&str> = ShortcutAction::ALL
            .iter()
            .flat_map(|a| a.candidates().iter().copied())
            .collect();
        let total = all.len();
        all.sort_unstable();
        all.dedup();
        assert_eq!(all.len(), total);
    }

    #[test]
    fn binds_the_preferred_shortcut_when_free() {
        let (binding, value) = bind_first(ShortcutAction::ToggleOverlay, |accel| Ok(accel.len()));
        assert_eq!(binding.accelerator.as_deref(), Some("Ctrl+Shift+Space"));
        assert_eq!(binding.error, None);
        assert_eq!(value, Some("Ctrl+Shift+Space".len()));
    }

    #[test]
    fn falls_back_and_reports_the_conflict() {
        let (binding, value) = bind_first(ShortcutAction::ToggleCapture, |accel| {
            if accel == "Ctrl+Shift+P" {
                Err("already registered".into())
            } else {
                Ok(())
            }
        });
        assert_eq!(binding.accelerator.as_deref(), Some("Ctrl+Alt+P"));
        assert_eq!(binding.preferred, "Ctrl+Shift+P");
        assert_eq!(
            binding.error.as_deref(),
            Some("Ctrl+Shift+P: already registered")
        );
        assert!(value.is_some());
    }

    #[test]
    fn reports_every_failure_when_no_candidate_registers() {
        let (binding, value) =
            bind_first::<()>(ShortcutAction::SubmitManual, |_| Err("taken".into()));
        assert_eq!(binding.accelerator, None);
        assert_eq!(
            binding.error.as_deref(),
            Some("Ctrl+Shift+Enter: taken; Ctrl+Alt+Enter: taken")
        );
        assert!(value.is_none());
    }

    #[test]
    fn actions_serialize_to_the_frontend_contract() {
        let json = serde_json::to_string(&ShortcutAction::ALL).unwrap();
        assert_eq!(
            json,
            r#"["toggle-overlay","toggle-capture","submit-manual","enlarge-active"]"#
        );
    }
}
