/// Захват звука с микрофона через CPAL.
///
/// Vosk получает моно PCM с частотой 16 кГц. Данные сначала собираются
/// в непрерывные блоки по 100 мс на исходной частоте, и только затем
/// ресемплируются. Это не сбрасывает фазу ресемплера на каждом callback CPAL.

use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, HostId, SampleFormat, StreamConfig};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioHost {
    Alsa,
    #[cfg(target_os = "macos")]
    CoreAudio,
    #[cfg(target_os = "windows")]
    Wasapi,
}

impl AudioHost {
    pub fn all() -> &'static [Self] {
        #[cfg(target_os = "linux")]
        { &[Self::Alsa] }
        #[cfg(target_os = "macos")]
        { &[Self::CoreAudio] }
        #[cfg(target_os = "windows")]
        { &[Self::Wasapi] }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Alsa => "ALSA",
            #[cfg(target_os = "macos")]
            Self::CoreAudio => "Core Audio",
            #[cfg(target_os = "windows")]
            Self::Wasapi => "WASAPI",
        }
    }

    pub fn host_id(&self) -> Option<HostId> {
        match self {
            Self::Alsa => Some(HostId::Alsa),
            #[cfg(target_os = "macos")]
            Self::CoreAudio => Some(HostId::CoreAudio),
            #[cfg(target_os = "windows")]
            Self::Wasapi => Some(HostId::Wasapi),
        }
    }

    pub fn get_host(&self) -> Option<cpal::Host> {
        self.host_id().and_then(|id| cpal::host_from_id(id).ok())
    }

    pub fn is_available(&self) -> bool {
        self.get_host().is_some()
    }

    pub fn default() -> Self {
        #[cfg(target_os = "linux")]
        { Self::Alsa }
        #[cfg(target_os = "macos")]
        { Self::CoreAudio }
        #[cfg(target_os = "windows")]
        { Self::Wasapi }
    }
}

/// Параметры оставлены в публичном API для совместимости с настройками EARS.
/// На этапе базовой отладки распознавания DSP намеренно отключён: сначала
/// передаём Vosk исходный сигнал без шумоподавления и VAD-гейтинга.
#[derive(Debug, Clone)]
pub struct AudioProcessingParams {
    pub sensitivity: f32,
    pub noise_suppression: f32,
    pub high_pass_cutoff: f32,
    pub min_speech_frames: usize,
    pub min_silence_frames: usize,
}

impl Default for AudioProcessingParams {
    fn default() -> Self {
        Self {
            sensitivity: 0.5,
            noise_suppression: 0.0,
            high_pass_cutoff: 0.0,
            min_speech_frames: 3,
            min_silence_frames: 10,
        }
    }
}

pub struct AudioCapture {
    running: Arc<AtomicBool>,
    #[allow(dead_code)]
    stream: Option<cpal::Stream>,
}

impl AudioCapture {
    pub fn start(
        callback: Arc<dyn Fn(&[f32]) + Send + Sync>,
        host: AudioHost,
        device_name: Option<&str>,
        _params: AudioProcessingParams,
    ) -> Result<Self> {
        let cpal_host = host.get_host()
            .ok_or_else(|| anyhow!("Аудиосервер '{}' недоступен", host.name()))?;

        let device = match device_name {
            Some(name) => cpal_host.input_devices()
                .context("Не удалось получить список устройств ввода")?
                .find(|device| device.name().map(|n| n == name).unwrap_or(false))
                .ok_or_else(|| anyhow!("Устройство ввода не найдено: {name}"))?,
            None => cpal_host.default_input_device()
                .context("Нет доступного устройства ввода (микрофона)")?,
        };

        let device_label = device.name().unwrap_or_else(|_| "<unknown>".into());
        let supported = device.default_input_config()
            .with_context(|| format!("Не удалось получить конфигурацию микрофона '{device_label}'"))?;
        let sample_format = supported.sample_format();
        let config: StreamConfig = supported.into();

        if config.channels == 0 {
            return Err(anyhow!("Микрофон сообщает недопустимое число каналов"));
        }

        eprintln!(
            "[AudioCapture] device='{}', host={}, channels={}, rate={} Hz, format={:?}",
            device_label, host.name(), config.channels, config.sample_rate.0, sample_format
        );

        let running = Arc::new(AtomicBool::new(true));
        let stream_running = Arc::clone(&running);
        let error_callback = |error| eprintln!("[Audio] Input stream error: {error}");

        let stream = match sample_format {
            SampleFormat::F32 => Self::build_stream::<f32>(
                &device, &config, callback, stream_running, error_callback
            )?,
            SampleFormat::I16 => Self::build_stream::<i16>(
                &device, &config, callback, stream_running, error_callback
            )?,
            SampleFormat::U16 => Self::build_stream::<u16>(
                &device, &config, callback, stream_running, error_callback
            )?,
            other => return Err(anyhow!("Неподдерживаемый формат аудио: {other:?}")),
        };

        stream.play().context("Не удалось запустить микрофон")?;
        eprintln!("[AudioCapture] input stream is running");
        Ok(Self { running, stream: Some(stream) })
    }

    fn build_stream<T>(
        device: &cpal::Device,
        config: &StreamConfig,
        callback: Arc<dyn Fn(&[f32]) + Send + Sync>,
        running: Arc<AtomicBool>,
        mut error_callback: impl FnMut(cpal::StreamError) + Send + 'static,
    ) -> Result<cpal::Stream>
    where
        T: cpal::SizedSample + Send + 'static,
        f32: FromSample<T>,
    {
        const TARGET_RATE: u32 = 16_000;
        const BLOCK_MS: u32 = 100;

        let channels = config.channels as usize;
        let source_rate = config.sample_rate.0;
        let block_samples = ((source_rate as u64 * BLOCK_MS as u64) / 1000) as usize;
        let running_callback = Arc::clone(&running);

        let stream = device.build_input_stream(
            config,
            move |data: &[T], _info: &cpal::InputCallbackInfo| {
                if !running_callback.load(Ordering::Relaxed) || data.is_empty() {
                    return;
                }

                // Сводим все входные каналы к моно, сохраняя непрерывный буфер.
                let mut mono = Vec::with_capacity(data.len() / channels);
                for frame in data.chunks_exact(channels) {
                    let sum: f32 = frame.iter().map(|sample| f32::from_sample(*sample)).sum();
                    mono.push(sum / channels as f32);
                }

                // Не ресемплируем отдельные маленькие callback-блоки:
                // накапливаем ровно 100 мс, чтобы не терять временную сетку.
                let mut pending = PENDING_AUDIO.with(|_| Vec::<f32>::new());
                pending.extend(mono);
                while pending.len() >= block_samples {
                    let source_block: Vec<f32> = pending.drain(..block_samples).collect();
                    let output = if source_rate == TARGET_RATE {
                        source_block
                    } else {
                        Self::resample_linear(&source_block, source_rate, TARGET_RATE)
                    };
                    if !output.is_empty() {
                        callback(&output);
                    }
                }
            },
            move |error| {
                error_callback(error);
            },
            None,
        )?;

        Ok(stream)
    }

    fn resample_linear(input: &[f32], source_rate: u32, target_rate: u32) -> Vec<f32> {
        if input.is_empty() || source_rate == 0 || target_rate == 0 {
            return Vec::new();
        }
        let output_len = (input.len() as u64 * target_rate as u64 / source_rate as u64) as usize;
        let ratio = source_rate as f64 / target_rate as f64;
        let mut output = Vec::with_capacity(output_len);
        for index in 0..output_len {
            let position = index as f64 * ratio;
            let left = position.floor() as usize;
            let fraction = (position - left as f64) as f32;
            let a = input[left.min(input.len() - 1)];
            let b = input[(left + 1).min(input.len() - 1)];
            output.push(a + (b - a) * fraction);
        }
        output
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        self.stream.take();
    }
}

impl Drop for AudioCapture {
    fn drop(&mut self) {
        self.stop();
    }
}
