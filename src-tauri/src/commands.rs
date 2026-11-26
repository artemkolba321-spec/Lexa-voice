/// Tauri-команды: обработчики для фронтенда.

use crate::devices::{add_activity, AppState};
use crate::snapshot::snapshot;
use crate::tools::{dispatch_tool, ToolRequest, ToolResult};
use crate::utils;
use cpal::traits::{DeviceTrait, HostTrait};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use super::voice::audio::AudioHost;
use super::voice::{find_vosk_model, VoiceEngine, VoiceEvent};
use super::VoiceStateWrapper;

// ===== DTO для команд =====

#[derive(Debug, Clone, Serialize)]
pub struct CommandResult {
    pub message: String,
    pub speak: bool,
}

// ===== Демо-парсер (текстовый ввод) =====

fn parse_demo_command(state: &mut AppState, text: &str) -> String {
    let query = text.trim().to_lowercase();

    if query.is_empty() {
        return "Напиши команду для тестирования.".into();
    }

    if query.contains("покажи устройства")
        || query.contains("список устройств")
        || query.contains("что с домом")
    {
        let request = ToolRequest {
            tool_name: "home.list_devices".into(),
            arguments: BTreeMap::new(),
        };

        return dispatch_tool(state, request).1;
    }

    let wants_on = ["включи", "включить", "зажги", "зажечь"]
        .iter()
        .any(|word| query.contains(word));

    let wants_off = ["выключи", "выключить", "погаси", "погасить"]
        .iter()
        .any(|word| query.contains(word));

    if wants_on || wants_off {
        let room = if query.contains("гостин") {
            Some("Гостиная")
        } else if query.contains("спальн") {
            Some("Спальня")
        } else if query.contains("рабочего стола") || query.contains("розетк") {
            Some("Кабинет")
        } else {
            None
        };

        if let Some(room_name) = room {
            let device_id = state
                .devices
                .iter()
                .find(|device| {
                    device.room == room_name
                        && matches!(device.kind, crate::devices::DeviceKind::Light | crate::devices::DeviceKind::Plug)
                })
                .map(|device| device.id.clone());

            if let Some(device_id) = device_id {
                let mut arguments = BTreeMap::new();
                arguments.insert("deviceId".into(), json!(device_id));
                arguments.insert("on".into(), json!(wants_on));

                let request = ToolRequest {
                    tool_name: "home.set_power".into(),
                    arguments,
                };

                return dispatch_tool(state, request).1;
            }
        }
    }

    if query.contains("температур") && query.contains("спальн") {
        if query.contains("какая")
            || query.contains("сколько")
            || query.contains("текущ")
        {
            let value = state
                .devices
                .iter()
                .find(|device| matches!(device.kind, crate::devices::DeviceKind::Thermostat))
                .and_then(|device| device.temperature);

            return match value {
                Some(value) => format!(
                    "Сейчас у виртуального термостата установлено {:.1} °C.",
                    value
                ),
                None => "Виртуальный термостат не найден.".into(),
            };
        }

        if let Some(number) = utils::extract_number(&query) {
            let mut arguments = BTreeMap::new();
            arguments.insert("deviceId".into(), json!("climate.bedroom"));
            arguments.insert("temperature".into(), json!(number));

            let request = ToolRequest {
                tool_name: "home.set_temperature".into(),
                arguments,
            };

            return dispatch_tool(state, request).1;
        }
    }

    add_activity(
        state,
        "Команда не распознана демо-парсером",
        text,
        "warning",
    );

    "Демо-парсер пока не знает эту команду. Это не LLM: позже здесь будет планировщик Лексы на локальной Qwen.".into()
}

// ===== Tauri Command Handlers =====

#[tauri::command]
pub async fn get_snapshot(state: tauri::State<'_, Mutex<AppState>>) -> Result<crate::snapshot::AppSnapshot, String> {
    let state = state
        .lock()
        .map_err(|_| "Состояние Лексы заблокировано.".to_string())?;

    Ok(snapshot(&state))
}

#[tauri::command]
pub async fn execute_tool(
    request: ToolRequest,
    state: tauri::State<'_, Mutex<AppState>>,
) -> Result<ToolResult, String> {
    let mut state = state
        .lock()
        .map_err(|_| "Состояние Лексы заблокировано.".to_string())?;

    let (ok, message) = dispatch_tool(&mut state, request);

    Ok(ToolResult {
        ok,
        message,
        speak: true,
    })
}

#[tauri::command]
pub async fn run_demo_command(
    text: String,
    state: tauri::State<'_, Mutex<AppState>>,
) -> Result<CommandResult, String> {
    let mut state = state
        .lock()
        .map_err(|_| "Состояние Лексы заблокировано.".to_string())?;

    let message = parse_demo_command(&mut state, &text);

    Ok(CommandResult {
        message,
        speak: true,
    })
}

#[tauri::command]
pub async fn set_profile(
    mode: String,
    state: tauri::State<'_, Mutex<AppState>>,
) -> Result<crate::snapshot::AppSnapshot, String> {
    if mode != "normal" && mode != "gaming" {
        return Err("Неизвестный профиль. Поддерживаются normal и gaming.".into());
    }

    let mut state = state
        .lock()
        .map_err(|_| "Состояние Лексы заблокировано.".to_string())?;

    state.mode = mode.clone();

    let (title, detail) = if mode == "gaming" {
        (
            "Игровой профиль включён (демо)",
            "Никакие процессы, клавиатура или контроллер ОС не изменялись",
        )
    } else {
        (
            "Обычный профиль включён (демо)",
            "Никакие системные настройки не изменялись",
        )
    };

    add_activity(&mut state, title, detail, "info");

    Ok(snapshot(&state))
}

/// Парсинг имени аудиосервера из строки.
fn parse_audio_host(host_name: Option<&str>) -> AudioHost {
    match host_name {
        Some("alsa") | Some("ALSA") => AudioHost::Alsa,
        _ => AudioHost::default(),
    }
}

// ==================== Голосовые команды Tauri ====================

#[tauri::command]
pub async fn start_listen(
    stt_model_path: Option<String>,
    audio_host_name: Option<String>,
    device_name: Option<String>,
    state: tauri::State<'_, VoiceStateWrapper>,
) -> Result<Value, String> {
    let engine_handle = Arc::clone(&state.0);
    let audio_host = parse_audio_host(audio_host_name.as_deref());

    let (tx, rx) = std::sync::mpsc::channel();

    {
        let mut guard = engine_handle
            .lock()
            .map_err(|_| "Voice engine locked".to_string())?;

        if guard.as_ref().is_some_and(|engine| engine.is_active()) {
            return Ok(json!({
                "ok": false,
                "error": "Already listening"
            }));
        }

        if guard.is_none() {
            let model_path = stt_model_path
                .map(PathBuf::from)
                .or_else(|| find_vosk_model().ok())
                .unwrap_or_else(|| {
                    PathBuf::from("models/vosk-model-small-ru-0.22")
                });

            let engine = VoiceEngine::new(Some(model_path), audio_host)
                .map_err(|e| format!("Failed to create voice engine: {e}"))?;

            *guard = Some(engine);
        }

        let engine = guard
            .as_mut()
            .ok_or_else(|| "Voice engine initialization failed".to_string())?;

        engine
            .start_listening(tx, audio_host, device_name)
            .map_err(|e| format!("Failed to start listening: {e}"))?;
    }

    // Этот поток только принимает и журналирует события.
    // Команды уже обрабатываются внутри VoiceEngine.
    let event_engine_handle = Arc::clone(&engine_handle);

    std::thread::spawn(move || {
        loop {
            match rx.recv_timeout(std::time::Duration::from_millis(300)) {
                Ok(event) => {
                    match event {
                        VoiceEvent::StateChanged(state) => {
                            eprintln!("Voice state changed: {:?}", state);
                        }
                        VoiceEvent::WakeWordDetected(text) => {
                            eprintln!("Wake word detected: {}", text);
                        }
                        VoiceEvent::CommandRecognized(command) => {
                            eprintln!("Command recognized: {}", command);
                        }
                        VoiceEvent::ResponseSpoken(text) => {
                            eprintln!("Response spoken: {}", text);
                        }
                        VoiceEvent::Error(error) => {
                            eprintln!("Voice error: {}", error);
                        }
                    }
                }

                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    let active = event_engine_handle
                        .lock()
                        .map(|guard| {
                            guard
                                .as_ref()
                                .is_some_and(|engine| engine.is_active())
                        })
                        .unwrap_or(false);

                    if !active {
                        break;
                    }
                }

                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    break;
                }
            }
        }
    });

    Ok(json!({ "ok": true }))
}

#[tauri::command]
pub async fn stop_listen(
    state: tauri::State<'_, VoiceStateWrapper>,
) -> Result<Value, String> {
    let mut guard = state
        .0
        .lock()
        .map_err(|_| "Voice engine locked".to_string())?;

    if let Some(engine) = guard.as_mut() {
        engine.stop_listening();
    }

    Ok(json!({ "ok": true }))
}

#[tauri::command]
pub async fn text_command(
    text: String,
    state: tauri::State<'_, VoiceStateWrapper>,
) -> Result<Value, String> {
    let response = {
        let guard = state
            .0
            .lock()
            .map_err(|_| "Voice engine locked".to_string())?;

        let engine = guard
            .as_ref()
            .ok_or_else(|| "Voice engine not initialized".to_string())?;

        engine
            .process_command(&text)
            .map_err(|e| format!("Command processing error: {e}"))?
    };

    // Не удерживаем блокировку Tauri State во время синтеза и воспроизведения.
    let engine_handle = Arc::clone(&state.0);
    let speech_text = response.clone();

    std::thread::spawn(move || {
        if let Ok(guard) = engine_handle.lock() {
            if let Some(engine) = guard.as_ref() {
                if let Err(error) = engine.speak(&speech_text) {
                    eprintln!("TTS error: {}", error);
                }
            }
        }
    });

    Ok(json!({
        "ok": true,
        "response": response,
        "state": "speaking"
    }))
}

#[tauri::command]
pub async fn get_voice_status(
    state: tauri::State<'_, VoiceStateWrapper>,
) -> Result<Value, String> {
    let guard = state
        .0
        .lock()
        .map_err(|_| "Voice engine locked".to_string())?;

    let status = if let Some(engine) = guard.as_ref() {
        json!({
            "state": format!("{:?}", engine.get_state()),
            "active": engine.is_active(),
            "initialized": true
        })
    } else {
        json!({
            "state": "Idle",
            "active": false,
            "initialized": false
        })
    };

    Ok(status)
}

// ===== Аудио утилиты =====

#[derive(Debug, Clone, Serialize)]
pub struct AudioDeviceInfo {
    pub name: String,
    pub is_default: bool,
}

#[tauri::command]
pub async fn list_audio_devices(
    audio_host_name: Option<String>,
) -> Result<Vec<AudioDeviceInfo>, String> {
    let audio_host = parse_audio_host(audio_host_name.as_deref());
    let host = audio_host
        .get_host()
        .ok_or_else(|| format!("Аудиосервер '{}' недоступен", audio_host.name()))?;

    let mut devices: Vec<AudioDeviceInfo> = Vec::new();

    // Получаем имя устройства по умолчанию
    let default_name = host
        .default_input_device()
        .map(|d| d.name().ok())
        .unwrap_or(None);

    // Перебираем все устройства ввода
    if let Ok(input_devices) = host.input_devices() {
        for device in input_devices {
            if let Ok(name) = device.name() {
                let is_default = default_name.as_deref() == Some(name.as_str());
                devices.push(AudioDeviceInfo {
                    name,
                    is_default,
                });
            }
        }
    }

    // Если ничего не нашли — возвращаем "default"
    if devices.is_empty() {
        devices.push(AudioDeviceInfo {
            name: "default".to_string(),
            is_default: true,
        });
    }

    Ok(devices)
}

#[tauri::command]
pub async fn list_audio_hosts() -> Result<Vec<(String, bool)>, String> {
    Ok(AudioHost::all()
        .iter()
        .map(|h| (h.name().to_string(), h.is_available()))
        .collect())
}

// ===== Тесты =====

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn power_tool_changes_only_allowed_device() {
        let mut state = AppState::default();

        let mut args = BTreeMap::new();
        args.insert("deviceId".into(), json!("light.bedroom"));
        args.insert("on".into(), json!(true));

        let (ok, message) = dispatch_tool(
            &mut state,
            ToolRequest {
                tool_name: "home.set_power".into(),
                arguments: args,
            },
        );

        assert!(ok);
        assert!(message.contains("включено"));

        assert!(
            state
                .devices
                .iter()
                .find(|device| device.id == "light.bedroom")
                .unwrap()
                .is_on
        );
    }

    #[test]
    fn temperature_tool_rejects_out_of_range_value() {
        let mut state = AppState::default();

        let mut args = BTreeMap::new();
        args.insert("deviceId".into(), json!("climate.bedroom"));
        args.insert("temperature".into(), json!(99.0));

        let (ok, message) = dispatch_tool(
            &mut state,
            ToolRequest {
                tool_name: "home.set_temperature".into(),
                arguments: args,
            },
        );

        assert!(!ok);
        assert!(message.contains("от 5 до 35"));
    }
}
