/// Инструменты (tools) умного дома и их выполнение.

use crate::devices::{add_activity, AppState, DeviceKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub permission: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolRequest {
    pub tool_name: String,
    pub arguments: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolResult {
    pub ok: bool,
    pub message: String,
    pub speak: bool,
}

pub fn tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "home.list_devices".into(),
            description: "Список доступных устройств умного дома".into(),
            permission: "read".into(),
        },
        ToolDefinition {
            name: "home.get_state".into(),
            description: "Прочитать состояние устройства по ID".into(),
            permission: "read".into(),
        },
        ToolDefinition {
            name: "home.set_power".into(),
            description: "Включить или выключить виртуальную лампу или розетку".into(),
            permission: "device-control".into(),
        },
        ToolDefinition {
            name: "home.set_temperature".into(),
            description: "Изменить целевую температуру виртуального термостата".into(),
            permission: "device-control".into(),
        },
        ToolDefinition {
            name: "system.get_status".into(),
            description: "Получить сводку о состоянии ядра Лексы".into(),
            permission: "read".into(),
        },
    ]
}

fn arg_string<'a>(request: &'a ToolRequest, key: &str) -> Option<&'a str> {
    request.arguments.get(key).and_then(Value::as_str)
}

/// Выполнение инструмента. Возвращает (успех, сообщение).
pub fn dispatch_tool(state: &mut AppState, request: ToolRequest) -> (bool, String) {
    match request.tool_name.as_str() {
        "home.list_devices" => {
            let summary = state
                .devices
                .iter()
                .map(|device| format!("{} ({})", device.name, device.id))
                .collect::<Vec<_>>()
                .join(", ");

            (
                true,
                format!(
                    "В песочнице {} устройств: {}.",
                    state.devices.len(),
                    summary
                ),
            )
        }

        "home.get_state" => {
            let Some(device_id) = arg_string(&request, "deviceId") else {
                return (false, "Не указан deviceId.".into());
            };

            let Some(device) = state.devices.iter().find(|d| d.id == device_id) else {
                return (false, format!("Устройство «{}» не найдено.", device_id));
            };

            let details = match device.kind {
                DeviceKind::Light => format!(
                    "{}: {}, яркость {}%.",
                    device.name,
                    if device.is_on { "включён" } else { "выключен" },
                    device.brightness.unwrap_or(0)
                ),
                DeviceKind::Thermostat => format!(
                    "{}: целевая температура {} °C.",
                    device.name,
                    device.temperature.unwrap_or(0.0)
                ),
                DeviceKind::Plug => format!(
                    "{}: {}.",
                    device.name,
                    if device.is_on { "включена" } else { "выключена" }
                ),
                DeviceKind::Sensor => format!(
                    "{}: {}.",
                    device.name,
                    device.reading.as_deref().unwrap_or("нет данных")
                ),
            };

            (true, details)
        }

        "home.set_power" => {
            let Some(device_id) = arg_string(&request, "deviceId").map(str::to_owned) else {
                return (false, "Не указан deviceId.".into());
            };

            let Some(on) = request.arguments.get("on").and_then(Value::as_bool) else {
                return (false, "Аргумент on должен быть true или false.".into());
            };

            let brightness = request
                .arguments
                .get("brightness")
                .and_then(Value::as_u64)
                .unwrap_or(70)
                .min(100) as u8;

            let Some(device) = state.devices.iter_mut().find(|d| d.id == device_id) else {
                return (false, format!("Устройство «{}» не найдено.", device_id));
            };

            match device.kind {
                DeviceKind::Light => {
                    device.is_on = on;
                    device.brightness = Some(if on { brightness.max(1) } else { 0 });
                }
                DeviceKind::Plug => {
                    device.is_on = on;
                }
                _ => {
                    return (
                        false,
                        "Этим типом устройства нельзя управлять через home.set_power.".into(),
                    );
                }
            }

            let device_name = device.name.clone();
            let message = format!(
                "{}: {}.",
                device_name,
                if on { "включено" } else { "выключено" }
            );

            add_activity(
                state,
                &message,
                &format!("Инструмент home.set_power · {}", device_id),
                "success",
            );

            (true, message)
        }

        "home.set_temperature" => {
            let Some(device_id) = arg_string(&request, "deviceId").map(str::to_owned) else {
                return (false, "Не указан deviceId.".into());
            };

            let Some(temperature) = request
                .arguments
                .get("temperature")
                .and_then(Value::as_f64)
            else {
                return (false, "Аргумент temperature должен быть числом.".into());
            };

            if !(5.0..=35.0).contains(&temperature) {
                return (
                    false,
                    "Температура должна быть в диапазоне от 5 до 35 °C.".into(),
                );
            }

            let Some(device) = state.devices.iter_mut().find(|d| d.id == device_id) else {
                return (false, format!("Устройство «{}» не найдено.", device_id));
            };

            if !matches!(device.kind, DeviceKind::Thermostat) {
                return (false, "Указанное устройство не является термостатом.".into());
            }

            device.temperature = Some((temperature * 10.0).round() as f32 / 10.0);

            let device_name = device.name.clone();
            let value = device.temperature.unwrap_or(temperature as f32);

            let message = format!(
                "Для «{}» установлено {:.1} °C.",
                device_name,
                value
            );

            add_activity(
                state,
                &message,
                &format!("Инструмент home.set_temperature · {}", device_id),
                "success",
            );

            (true, message)
        }

        "system.get_status" => (
            true,
            "Ядро отвечает. Голосовые рантаймы и системные интеграции не подключены; доступна только песочница.".into(),
        ),

        unknown => (false, format!("Инструмент «{}» не зарегистрирован.", unknown)),
    }
}
