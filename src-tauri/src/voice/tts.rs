/// TTS (Text-to-Speech) через AnyTTS (чистый Rust, без Python).
///
/// Поддерживаемые модели: Qwen3-TTS.
/// Загрузка выполняется только из локальной директории: запуск не требует сети.

use anyhow::{Context, Result};
use any_tts::models::qwen3_tts::Qwen3TtsModel;
use any_tts::traits::TtsModel;
use any_tts::{ModelType, SynthesisRequest, TtsConfig};
use rodio::Sink;
use std::path::Path;
use std::process::Command;

pub struct TtsEngine {
    model: Option<Qwen3TtsModel>,
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

        // A lightweight OS speech engine is a useful offline fallback when the
        // optional neural voice model has not been installed yet.
        // Do not block microphone/STT startup if no speech backend is installed.
        // speak() will return a platform-specific setup hint when it is invoked.
        Ok(Self { model: None })
    }

    /// Загрузка модели из локальной директории
    fn load_from_path(path: &str) -> Result<Self> {
        eprintln!("[TTS] Loading Qwen3-TTS model from: {}", path);

        let config = TtsConfig::new(ModelType::Qwen3Tts)
            .with_model_path(path);

        let model = Qwen3TtsModel::load(config)
            .context("Не удалось загрузить модель Qwen3-TTS. Убедись, что модель скачана и путь верный.")?;

        eprintln!("[TTS] Model loaded OK from: {}", path);
        Ok(Self { model: Some(model) })
    }

    /// Генерация аудио из текста
    pub fn synthesize(&self, text: &str) -> Result<(Vec<f32>, u32)> {
        let model = self.model.as_ref()
            .context("Нейросетевая модель Qwen3-TTS не загружена; доступен только системный синтез речи.")?;
        let request = SynthesisRequest::new(text)
            .with_language("ru");

        let samples = model.synthesize(&request)
            .context("Ошибка синтеза речи")?;

        Ok((samples.samples.clone(), samples.sample_rate))
    }

    /// Воспроизведение текста через TTS
    pub fn speak(&self, text: &str) -> Result<()> {
        if self.model.is_none() {
            return Self::speak_with_system_voice(text);
        }

        let (pcm, sample_rate) = self.synthesize(text)?;
        let stream = rodio::OutputStreamBuilder::from_default_device()
            .context("Не удалось открыть устройство вывода")?
            .open_stream()?;
        let mixer = stream.mixer();
        let sink = Sink::connect_new(&mixer);
        sink.append(rodio::buffer::SamplesBuffer::new(1, sample_rate, pcm));
        sink.play();
        // Keep the output stream and sink alive until playback has completed.
        sink.sleep_until_end();
        Ok(())
    }

    fn speak_with_system_voice(text: &str) -> Result<()> {
        #[cfg(target_os = "linux")]
        {
            let executable = if Command::new("espeak-ng").arg("--version").output().is_ok() {
                "espeak-ng"
            } else {
                "espeak"
            };
            let status = Command::new(executable)
                .arg("-v").arg("ru")
                .arg(text)
                .status()
                .context("Не удалось запустить espeak-ng/espeak. Установи espeak-ng для русской речи.")?;
            if !status.success() {
                anyhow::bail!("Системный синтезатор речи завершился с ошибкой: {status}");
            }
            return Ok(());
        }
        #[cfg(target_os = "macos")]
        {
            let status = Command::new("say")
                .arg(text)
                .status()
                .context("Не удалось запустить системный синтезатор say")?;
            if !status.success() {
                anyhow::bail!("Системный синтезатор say завершился с ошибкой: {status}");
            }
            return Ok(());
        }
        #[cfg(target_os = "windows")]
        {
            let status = Command::new("powershell")
                .arg("-NoProfile")
                .arg("-Command")
                .arg("$s = New-Object System.Speech.Synthesis.SpeechSynthesizer; $s.Speak($env:LEXA_TTS_TEXT)")
                .env("LEXA_TTS_TEXT", text)
                .status()
                .context("Не удалось запустить Windows Speech")?;
            if !status.success() {
                anyhow::bail!("Windows Speech завершился с ошибкой: {status}");
            }
            return Ok(());
        }
        #[allow(unreachable_code)]
        anyhow::bail!("Системный синтезатор речи не поддерживается на этой платформе")
    }

    /// Проверка доступности модели
    #[allow(dead_code)]
    pub fn is_available(model_path: Option<&str>) -> bool {
        let path = model_path.unwrap_or("models/Qwen3-TTS");
        Path::new(path).exists()
    }
}
