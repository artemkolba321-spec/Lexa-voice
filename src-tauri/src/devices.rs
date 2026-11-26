/// Устройства умного дома и состояние приложения.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DeviceKind {
    Light,
    Thermostat,
    Plug,
    Sensor,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmartDevice {
    pub id: String,
    pub name: String,
    pub room: String,
    pub kind: DeviceKind,
    pub is_on: bool,
    pub brightness: Option<u8>,
    pub temperature: Option<f32>,
    pub reading: Option<String>,
}

// ===== Состояние приложения =====

#[derive(Debug, Clone, Serialize)]
pub struct ActivityItem {
    pub id: String,
    pub time: String,
    pub title: String,
    pub detail: String,
    pub tone: String,
}

pub struct AppState {
    pub mode: String,
    pub listening: bool,
    pub devices: Vec<SmartDevice>,
    pub activity: Vec<ActivityItem>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            mode: "normal".to_string(),
            listening: false,
            devices: vec![
                SmartDevice {
                    id: "light.living_room".to_string(),
                    name: "Свет в гостиной".to_string(),
                    room: "Гостиная".to_string(),
                    kind: DeviceKind::Light,
                    is_on: true,
                    brightness: Some(70),
                    temperature: None,
                    reading: None,
                },
                SmartDevice {
                    id: "light.bedroom".to_string(),
                    name: "Свет в спальне".to_string(),
                    room: "Спальня".to_string(),
                    kind: DeviceKind::Light,
                    is_on: false,
                    brightness: Some(0),
                    temperature: None,
                    reading: None,
                },
                SmartDevice {
                    id: "climate.bedroom".to_string(),
                    name: "Термостат".to_string(),
                    room: "Спальня".to_string(),
                    kind: DeviceKind::Thermostat,
                    is_on: true,
                    brightness: None,
                    temperature: Some(21.0),
                    reading: None,
                },
                SmartDevice {
                    id: "plug.desk".to_string(),
                    name: "Розетка рабочего стола".to_string(),
                    room: "Кабинет".to_string(),
                    kind: DeviceKind::Plug,
                    is_on: false,
                    brightness: None,
                    temperature: None,
                    reading: None,
                },
                SmartDevice {
                    id: "sensor.motion_living".to_string(),
                    name: "Движение".to_string(),
                    room: "Гостиная".to_string(),
                    kind: DeviceKind::Sensor,
                    is_on: true,
                    brightness: None,
                    temperature: None,
                    reading: Some("Движения нет".to_string()),
                },
            ],
            activity: vec![
                activity_item(
                    "Песочница умного дома готова",
                    "5 виртуальных устройств доступны для тестирования",
                    "success",
                ),
                activity_item(
                    "Голосовой стек не подключён",
                    "Голосовые компоненты запускаются отдельно при включении микрофона",
                    "info",
                ),
            ],
        }
    }
}

// ===== Утилиты активности =====

pub fn activity_item(title: &str, detail: &str, tone: &str) -> ActivityItem {
    use chrono::Local;
    ActivityItem {
        id: format!("{}-{}", Local::now().timestamp_millis(), title.len()),
        time: Local::now().format("%H:%M").to_string(),
        title: title.to_string(),
        detail: detail.to_string(),
        tone: tone.to_string(),
    }
}

pub fn add_activity(state: &mut AppState, title: &str, detail: &str, tone: &str) {
    state.activity.insert(0, activity_item(title, detail, tone));
    state.activity.truncate(100);
}
