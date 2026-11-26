/// TTS (Text-to-Speech) через AnyTTS (чистый Rust, без Python).
///
/// Поддерживаемые модели: Qwen3-TTS, Kokoro, Voxtral, VibeVoice, OmniVoice.
/// Модель Qwen3-TTS скачивается автоматически с HuggingFace при первом запуске.

use anyhow::{Context, Result};
use any_tts::models::qwen3_tts::Qwen3TtsModel;
use any_tts::traits::TtsModel;
use any_tts::{ModelType, SynthesisRequest, TtsConfig};
use rodio::Sink;
use std::path::Path;

pub struct TtsEngine {
    model: Qwen3TtsModel,
}

impl TtsEngine {
    /// Создание TTS-движка с моделью Qwen3-TTS.
    /// `model_path` — путь к директории модели Qwen3-TTS.
    /// Если None — пытается загрузить из локального кеша HuggingFace или скачивает модель.
    pub fn new(model_path: Option<String>) -> Result<Self> {
        // Пробуем локальный путь, если указан
        if let Some(path) = &model_path {
            if Path::new(path).exists() {
                return Self::load_from_path(path);
            }
        }

        // Пробуем стандартные пути
        let standard_paths = [
            "models/Qwen3-TTS",
            "../models/Qwen3-TTS",
            "src-tauri/models/Qwen3-TTS",
        ];

        for path in &standard_paths {
            if Path::new(path).exists() {
                return Self::load_from_path(path);
            }
        }

        // Скачиваем с HuggingFace
        eprintln!("[TTS] Model not found locally, downloading from HuggingFace...");
        Self::download_and_load()
    }

    /// Загрузка модели из локальной директории
    fn load_from_path(path: &str) -> Result<Self> {
        eprintln!("[TTS] Loading Qwen3-TTS model from: {}", path);

        let config = TtsConfig::new(ModelType::Qwen3Tts)
            .with_model_path(path);

        let model = Qwen3TtsModel::load(config)
            .context("Не удалось загрузить модель Qwen3-TTS. Убедись, что модель скачана и путь верный.")?;

        eprintln!("[TTS] Model loaded OK from: {}", path);
        Ok(Self { model })
    }

    /// Автоматическая загрузка модели с HuggingFace
    fn download_and_load() -> Result<Self> {
        eprintln!("[TTS] Downloading Qwen3-TTS from HuggingFace...");

        let config = TtsConfig::new(ModelType::Qwen3Tts)
            .with_hf_model_id("Qwen/Qwen2.5-0.5B");

        let model = Qwen3TtsModel::load(config)
            .context("Не удалось скачать и загрузить модель Qwen3-TTS с HuggingFace. Проверь интернет.")?;

        eprintln!("[TTS] Model downloaded and loaded OK");
        Ok(Self { model })
    }

    /// Генерация аудио из текста
    pub fn synthesize(&self, text: &str) -> Result<(Vec<f32>, u32)> {
        let request = SynthesisRequest::new(text)
            .with_language("ru");

        let samples = self.model.synthesize(&request)
            .context("Ошибка синтеза речи")?;

        Ok((samples.samples.clone(), samples.sample_rate))
    }

    /// Воспроизведение текста через TTS
    pub fn speak(&self, text: &str) -> Result<()> {
        let (pcm, sample_rate) = self.synthesize(text)?;

        let stream = rodio::OutputStreamBuilder::from_default_device()
            .context("Не удалось открыть устройство вывода")?
            .open_stream()?;

        let mixer = stream.mixer();
        let sink = Sink::connect_new(&mixer);
        sink.append(rodio::buffer::SamplesBuffer::new(1, sample_rate, pcm));
        sink.play();

        Ok(())
    }

    /// Проверка доступности модели
    #[allow(dead_code)]
    pub fn is_available(model_path: Option<&str>) -> bool {
        let path = model_path.unwrap_or("models/Qwen3-TTS");
        Path::new(path).exists()
    }
}
