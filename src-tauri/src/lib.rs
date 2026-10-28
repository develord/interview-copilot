pub mod audio;
pub mod events;
pub mod intent;
pub mod secrets;
mod shortcuts;
pub mod stt;

use std::sync::Arc;

use audio::backend::CpalBackend;
use audio::engine::AudioEngine;
use audio::worker::RecoveryPolicy;
use tauri_plugin_window_state::StateFlags;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(shortcuts::ShortcutRegistry::default())
        .manage(AudioEngine::new(
            Arc::new(CpalBackend),
            RecoveryPolicy::DEFAULT,
        ))
        .plugin(shortcuts::plugin())
        // Restores the overlay's size and position across launches; the plugin falls back to
        // the default placement when the saved monitor is no longer connected.
        .plugin(
            tauri_plugin_window_state::Builder::new()
                .with_state_flags(StateFlags::SIZE | StateFlags::POSITION)
                .build(),
        )
        .setup(|app| {
            shortcuts::register_defaults(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            shortcuts::shortcut_status,
            audio::commands::audio_list_devices,
            audio::commands::audio_start,
            audio::commands::audio_pause,
            audio::commands::audio_resume,
            audio::commands::audio_set_muted,
            audio::commands::audio_stop,
            audio::commands::open_microphone_privacy_settings,
            stt::commands::stt_key_status,
            stt::commands::stt_set_key,
            stt::commands::stt_clear_key,
            intent::commands::llm_key_status,
            intent::commands::llm_set_key,
            intent::commands::llm_clear_key,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Interview Copilot");
}
