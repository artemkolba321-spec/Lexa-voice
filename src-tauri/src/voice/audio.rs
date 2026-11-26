/// Захват звука с микрофона через CPAL.
/// Интегрирует EARS-обработку: VAD, шумоподавление, чувствительность.

use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, HostId, Sample, SampleFormat, StreamConfig};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use crate::voice::ears::{HighPassFilter, NoiseSuppressor, VoiceDetector};

/// Доступные аудиосерверы (CPAL hosts).
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
        {
            &[Self::Alsa]
        }
        #[cfg(target_os = "macos")]
        {
            &[Self::CoreAudio]
        }
        #[cfg(target_os = "windows")]
        {
            &[Self::Wasapi]
        }
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

    /// Получить CPAL HostId для этого сервера.
    pub fn host_id(&self) -> Option<HostId> {
        match self {
            Self::Alsa => Some(HostId::Alsa),
            #[cfg(target_os = "macos")]
            Self::CoreAudio => Some(HostId::CoreAudio),
            #[cfg(target_os = "windows")]
            Self::Wasapi => Some(HostId::Wasapi),
        }
    }

    /// Получить CPAL host по Id. Возвращает None, если сервер недоступен.
    pub fn get_host(&self) -> Option<cpal::Host> {
        self.host_id()
            .and_then(|id| cpal::host_from_id(id).ok())
    }

    /// Проверить, доступен ли этот сервер.
    pub fn is_available(&self) -> bool {
        self.get_host().is_some()
    }

    /// Получить сервер, который используется по умолчанию.
    pub fn default() -> Self {
        #[cfg(target_os = "linux")]
        {
            Self::Alsa
        }
        #[cfg(target_os = "macos")]
        {
            Self::CoreAudio
        }
        #[cfg(target_os = "windows")]
        {
            Self::Wasapi
        }
    }
}

/// Параметры обработки звука (EARS).
#[derive(Debug, Clone)]
pub struct AudioProcessingParams {
    /// Чувствительность микрофона (0.0 = очень чувствительный, 1.0 = мало чувствительный)
    pub sensitivity: f32,
    /// Уровень шумоподавления (0.0 = выкл, 1.0 = макс)
    pub noise_suppression: f32,
    /// Частота cutoff high-pass фильтра (Гц)
    pub high_pass_cutoff: f32,
    /// Минимальная длительность речи для активации VAD (кадры)
    pub min_speech_frames: usize,
    /// Минимальная длительность тишины для деактивации VAD (кадры)
    pub min_silence_frames: usize,
}

impl Default for AudioProcessingParams {
    fn default() -> Self {
        Self {
            sensitivity: 0.5,       // средняя чувствительность
            noise_suppression: 0.5, // среднее шумоподавление
            high_pass_cutoff: 150.0, // удаляем гул ниже 150 Гц
            min_speech_frames: 3,   // ~100ms при 30fps
            min_silence_frames: 10, // ~330ms при 30fps
        }
    }
}

pub struct AudioCapture {
    running: Arc<AtomicBool>,
    #[allow(dead_code)]
    stream: Option<cpal::Stream>,
    /// Параметры обработки (для обновления на лету)
    params: Arc<AudioProcessingParams>,
}

impl AudioCapture {
    /// Запуск захвата микрофона.
    /// `host` — аудиосервер (ALSA, PulseAudio, JACK и т.д.).
    /// `device_name` — если None, используется устройство по умолчанию.
    /// `params` — параметры обработки звука (VAD, шумоподавление, чувствительность).
    pub fn start(
        callback: Arc<dyn Fn(&[f32]) + Send + Sync>,
        host: AudioHost,
        device_name: Option<&str>,
        params: AudioProcessingParams,
    ) -> Result<Self> {
        eprintln!("[Audio] AudioCapture::start() CALLED");
        eprintln!("[AudioCapture] Host: {}", host.name());
        eprintln!("[AudioCapture] Processing params: sensitivity={}, noise_suppression={}",
            params.sensitivity, params.noise_suppression);

        let cpal_host = host
            .get_host()
            .ok_or_else(|| anyhow!("Аудиосервер '{}' недоступен. Установите библиотеку development.", host.name()))?;

        let device = match device_name {
            Some(name) => {
                eprintln!("[AudioCapture] Looking for device: {:?}", name);
                cpal_host
                    .input_devices()
                    .context("Не удалось получить список устройств ввода")?
                    .find(|d| {
                        if let Ok(n) = d.name() {
                            n == *name
                        } else {
                            false
                        }
                    })
                    .ok_or_else(|| anyhow!("Устройство не найдено: {name}"))?
            }
            None => cpal_host
                .default_input_device()
                .context("Нет доступного устройства ввода (микрофона)")?,
        };

        eprintln!("[AudioCapture] Input device: {:?}", device.name());

        let supported_config = device
            .default_input_config()
            .context("Не удалось получить конфигурацию микрофона")?;

        eprintln!("[AudioCapture] Config: channels={}, format={:?}",
            supported_config.channels(),
            supported_config.sample_format()
        );

        let sample_format = supported_config.sample_format();
        let config: StreamConfig = supported_config.into();

        eprintln!("[AudioCapture] Stream config: channels={}, rate={}", config.channels, config.sample_rate.0);

        if config.channels == 0 {
            return Err(anyhow!("Микрофон сообщает недопустимое число каналов"));
        }

        let running = Arc::new(AtomicBool::new(true));
        let running_callback = Arc::clone(&running);
        let params_shared = Arc::new(params);

        let error_callback = |err| {
            eprintln!("Ошибка аудиопотока: {err}");
        };

        let stream = match sample_format {
            SampleFormat::F32 => {
                Self::build_stream::<f32>(
                    &device,
                    &config,
                    callback,
                    running_callback,
                    error_callback,
                    Arc::clone(&params_shared),
                )?
            }
            SampleFormat::I16 => {
                Self::build_stream::<i16>(
                    &device,
                    &config,
                    callback,
                    running_callback,
                    error_callback,
                    Arc::clone(&params_shared),
                )?
            }
            SampleFormat::U16 => {
                Self::build_stream::<u16>(
                    &device,
                    &config,
                    callback,
                    running_callback,
                    error_callback,
                    Arc::clone(&params_shared),
                )?
            }
            other => {
                return Err(anyhow!(
                    "Неподдерживаемый формат аудио: {other:?}"
                ));
            }
        };

        Ok(AudioCapture {
            running,
            stream: Some(stream),
            params: params_shared,
        })
    }

    fn build_stream<T>(
        device: &cpal::Device,
        config: &StreamConfig,
        callback: Arc<dyn Fn(&[f32]) + Send + Sync>,
        running: Arc<AtomicBool>,
        mut error_callback: impl FnMut(cpal::StreamError) + Send + 'static,
        params: Arc<AudioProcessingParams>,
    ) -> Result<cpal::Stream>
    where
        T: cpal::SizedSample + Send + 'static,
        f32: FromSample<T>,
    {
        let channels = config.channels as usize;
        let sample_rate = config.sample_rate.0 as f64;
        let target_rate = 16000.0; // Vosk требует 16kHz
        let resample_ratio = target_rate / sample_rate;
        let running_for_callback = Arc::clone(&running);

        // Создаём EARS-компоненты
        let mut high_pass = HighPassFilter::new(params.high_pass_cutoff, sample_rate as f32);
        let mut noise_suppressor = NoiseSuppressor::new(params.noise_suppression, 0.01);
        let mut vad = VoiceDetector::new(
            0.01 + (params.sensitivity * 0.29),
            params.min_speech_frames,
            params.min_silence_frames,
        );

        let stream = device.build_input_stream(
            config,
            move |data: &[T], _info: &cpal::InputCallbackInfo| {
                if !running_for_callback.load(Ordering::Relaxed) {
                    return;
                }

                // Конвертируем в f32
                let samples: Vec<f32> = data
                    .iter()
                    .map(|sample| f32::from_sample(*sample))
                    .collect();

                // Моно (если стерео)
                let mono: Vec<f32> = if channels > 1 {
                    samples
                        .chunks(channels)
                        .map(|frame| {
                            frame.iter().sum::<f32>() / frame.len() as f32
                        })
                        .collect()
                } else {
                    samples
                };

                // ===== EARS обработка =====

                // 1. High-pass filter (удаляем низкочастотный гул)
                let filtered = high_pass.process(&mono);

                // 2. Ресемплинг в 16kHz (Vosk требует 16kHz)
                let resampled = if (sample_rate - target_rate).abs() > 10.0 {
                    Self::resample_linear(&filtered, resample_ratio)
                } else {
                    filtered
                };

                // 3. Шумоподавление
                let denoised = noise_suppressor.process(&resampled);

                // 4. VAD (Voice Activity Detection)
                let _ = vad.process_frame(&denoised);

                // Передаём обработанный сигнал в STT
                callback(&denoised);
            },
            move |err| {
                eprintln!("[Audio] Stream ERROR: {}", err);
                error_callback(err)
            },
            None,
        )?;

        eprintln!("[AudioCapture] Stream built successfully, playing...");
        stream.play().context("Не удалось запустить микрофон")?;
        eprintln!("[AudioCapture] Stream playing!");

        Ok(stream)
    }

    /// Линейный ресемплинг аудио (up/down)
    fn resample_linear(input: &[f32], ratio: f64) -> Vec<f32> {
        let output_len = (input.len() as f64 * ratio) as usize;
        if output_len == 0 {
            return Vec::new();
        }

        let mut output = Vec::with_capacity(output_len);
        for i in 0..output_len {
            let src_pos = i as f64 / ratio;
            let idx = src_pos as usize;
            let frac = src_pos - idx as f64;

            if idx + 1 < input.len() {
                let v0 = input[idx] as f64;
                let v1 = input[idx + 1] as f64;
                output.push((v0 + frac * (v1 - v0)) as f32);
            } else if idx < input.len() {
                output.push(input[idx]);
            }
        }
        output
    }

    /// Остановка захвата микрофона.
    pub fn stop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
    }
}

impl Drop for AudioCapture {
    fn drop(&mut self) {
        self.stop();
    }
}
