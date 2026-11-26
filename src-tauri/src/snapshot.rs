/// Снимок состояния приложения для фронтенда.

use crate::devices::AppState;
use crate::tools::tool_definitions;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeStatus {
    pub stt: String,
    pub llm: String,
    pub tts: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AppSnapshot {
    pub mode: String,
    pub listening: bool,
    pub devices: Vec<crate::devices::SmartDevice>,
    pub tools: Vec<crate::tools::ToolDefinition>,
    pub activity: Vec<crate::devices::ActivityItem>,
    pub runtime: RuntimeStatus,
}

pub fn snapshot(state: &AppState) -> AppSnapshot {
    AppSnapshot {
        mode: state.mode.clone(),
        listening: state.listening,
        devices: state.devices.clone(),
        tools: tool_definitions(),
        activity: state.activity.clone(),
        runtime: RuntimeStatus {
            stt: "not-connected".into(),
            llm: "not-connected".into(),
            tts: "not-connected".into(),
        },
    }
}
