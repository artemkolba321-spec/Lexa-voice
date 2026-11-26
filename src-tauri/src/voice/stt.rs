/// STT (Speech-to-Text) на базе Vosk.
///
/// Режимы:
/// 1. Wake word detection — обнаружение обращения «Лекса».
/// 2. Continuous — распознавание полной речи.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use vosk::{Model, Recognizer};

const WAKE_WORD: &str = "лекса";
const SAMPLE_RATE: f32 = 16000.0;

pub struct SttEngine {
    recognizer: Recognizer,
    _model: Model,
}

impl SttEngine {
    /// Создание STT-движка.
    pub fn new(model_path: PathBuf) -> Result<Self> {
        let model = Model::new(model_path.to_string_lossy().into_owned())
            .context(
                "Не удалось загрузить модель Vosk. Проверь путь к модели.",
            )?;

        let mut recognizer = Recognizer::new(&model, SAMPLE_RATE)
            .context("Не удалось создать Recognizer Vosk")?;

        recognizer.set_max_alternatives(0);

        Ok(Self {
            recognizer,
            _model: model,
        })
    }

    /// Создание STT-движка для непрерывного распознавания речи.
    pub fn new_continuous(model_path: &Path) -> Result<Self> {
        let mut engine = Self::new(model_path.to_path_buf())?;
        engine.recognizer.set_words(true);
        Ok(engine)
    }

    /// Обработка аудиофрейма.
    pub fn process_audio(&mut self, audio: &[f32]) -> Option<String> {
        if audio.is_empty() {
            return None;
        }

        let pcm: Vec<i16> = audio
            .iter()
            .map(|&sample| {
                let sample = sample.clamp(-1.0, 1.0);
                (sample * 32767.0) as i16
            })
            .collect();

        if pcm.is_empty() {
            return None;
        }

        // Подаём аудио в Vosk
        let _ = self.recognizer.accept_waveform(&pcm);

        // Проверяем partial_result (мгновенный ответ)
        let partial = self.recognizer.partial_result();
        let partial_text = partial.partial.trim();
        if !partial_text.is_empty() {
            return Some(partial_text.to_string());
        }

        // Потом проверяем result (завершённая фраза)
        let result = self.recognizer.result();
        if let Some(single) = result.single() {
            let text = single.text.trim().to_string();
            if !text.is_empty() {
                return Some(text);
            }
        }

        None
    }

    /// Проверка наличия wake word «Лекса».
    pub fn is_wake_word(text: &str) -> bool {
        text.split_whitespace().any(|word| {
            word.trim_matches(|c: char| !c.is_alphanumeric())
                .eq_ignore_ascii_case(WAKE_WORD)
        })
    }

    /// Извлечение команды, произнесённой после wake word.
    pub fn extract_command_after_wake_word(text: &str) -> Option<String> {
        let lower = text.to_lowercase();
        let mut found_wake = false;

        for word in lower.split_whitespace() {
            let normalized = word.trim_matches(|c: char| !c.is_alphanumeric());
            if !found_wake {
                if normalized == WAKE_WORD {
                    found_wake = true;
                    continue;
                }
            } else {
                let command = word.trim_start_matches(|c: char| !c.is_alphanumeric())
                    .trim();
                if !command.is_empty() {
                    return Some(command.to_string());
                }
            }
        }

        None
    }

    /// Получение стандартного пути к модели.
    #[allow(dead_code)]
    pub fn model_path(&self) -> PathBuf {
        PathBuf::from("models/vosk-model-small-ru-0.22")
    }
}

/// Поиск модели Vosk в стандартных местах.
pub fn find_vosk_model() -> Result<PathBuf> {
    let candidates = [
        PathBuf::from("models/vosk-model-small-ru-0.22"),
        PathBuf::from("../models/vosk-model-small-ru-0.22"),
        PathBuf::from("src-tauri/models/vosk-model-small-ru-0.22"),
        PathBuf::from("../src-tauri/models/vosk-model-small-ru-0.22"),
    ];

    for candidate in &candidates {
        if candidate.is_dir() {
            return Ok(candidate.clone());
        }
    }

    Err(anyhow::anyhow!(
        "Модель Vosk не найдена. Положи модель в \
         src-tauri/models/vosk-model-small-ru-0.22/"
    ))
}
