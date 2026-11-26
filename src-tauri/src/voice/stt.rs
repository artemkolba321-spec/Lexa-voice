/// STT (Speech-to-Text) на базе Vosk.
///
/// Режимы:
/// 1. Wake word detection — обнаружение обращения «Лекса».
/// 2. Continuous — распознавание полной речи.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use vosk::{DecodingState, Model, Recognizer};

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

        // Only emit finalized utterances. Partial hypotheses can change on the
        // next audio frame and must not trigger wake words or duplicate commands.
        match self.recognizer.accept_waveform(&pcm) {
            Ok(DecodingState::Finalized) => {
                let result = self.recognizer.result();
                result.single().and_then(|single| {
                    let text = single.text.trim();
                    (!text.is_empty()).then(|| text.to_string())
                })
            }
            Ok(DecodingState::Running) => None,
            Ok(DecodingState::Failed) | Err(_) => None,
        }
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
        let words: Vec<&str> = lower.split_whitespace().collect();

        for (index, word) in words.iter().enumerate() {
            let normalized = word.trim_matches(|c: char| !c.is_alphanumeric());
            if normalized == WAKE_WORD {
                let command = words[index + 1..]
                    .join(" ")
                    .trim_matches(|c: char| !c.is_whitespace() && !c.is_alphanumeric())
                    .to_string();
                return if command.is_empty() { None } else { Some(command) };
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


#[cfg(test)]
mod tests {
    use super::SttEngine;

    #[test]
    fn extracts_entire_command_after_wake_word() {
        assert_eq!(
            SttEngine::extract_command_after_wake_word("Лекса включи свет в гостиной"),
            Some("включи свет в гостиной".to_string())
        );
    }

    #[test]
    fn returns_none_when_wake_word_has_no_command() {
        assert_eq!(SttEngine::extract_command_after_wake_word("Лекса!"), None);
    }

    #[test]
    fn ignores_words_before_wake_word() {
        assert_eq!(
            SttEngine::extract_command_after_wake_word("эй, Лекса, который час?"),
            Some("который час?".to_string())
        );
    }
}
