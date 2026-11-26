/// EARS — подсистема обработки звука перед распознаванием.
///
/// Компоненты:
/// - VAD (Voice Activity Detection) — обнаружение речи/тишины
/// - Noise suppression — подавление фонового шума
/// - Gain control — регулировка чувствительности микрофона

// ===== VAD (Voice Activity Detection) =====

/// Состояние VAD.
#[derive(Debug, Clone, PartialEq)]
pub enum VadState {
    Idle,
    Speech,
}

/// Простой VAD на основе RMS (Root Mean Square) и zero-crossing rate.
/// Работает локально, без ML-моделей.
pub struct VoiceDetector {
    /// Порог RMS для обнаружения речи (0.0..1.0)
    speech_threshold: f32,
    /// Минимальное количество кадров речи для активации
    min_speech_frames: usize,
    /// Минимальное количество кадров тишины для завершения
    min_silence_frames: usize,
    /// Текущее состояние
    state: VadState,
    /// Флаг для предотвращения дублирования событий
    last_state: VadState,
    /// Счётчик кадров речи
    speech_frames: usize,
    /// Счётчик кадров тишины
    silence_frames: usize,
    /// Буфер последних кадров для VAD
    frame_history: Vec<bool>,
}

impl VoiceDetector {
    /// Создать VAD с заданными параметрами.
    pub fn new(speech_threshold: f32, min_speech_frames: usize, min_silence_frames: usize) -> Self {
        Self {
            speech_threshold,
            min_speech_frames,
            min_silence_frames,
            state: VadState::Idle,
            last_state: VadState::Idle,
            speech_frames: 0,
            silence_frames: 0,
            frame_history: Vec::with_capacity(min_speech_frames + min_silence_frames),
        }
    }

    /// Настроить чувствительность (0.0 = очень чувствительный, 1.0 = мало чувствительный).
    pub fn set_sensitivity(&mut self, sensitivity: f32) {
        // sensitivity 0.0 → threshold 0.01 (очень чувствительный)
        // sensitivity 1.0 → threshold 0.3 (мало чувствительный)
        self.speech_threshold = 0.01 + (sensitivity * 0.29);
    }

    /// Проверить аудиофрейм на наличие речи.
    /// Возвращает (is_speech, state_changed).
    pub fn process_frame(&mut self, samples: &[f32]) -> (bool, bool) {
        if samples.is_empty() {
            return (false, false);
        }

        let rms = Self::calculate_rms(samples);
        let is_speech = rms > self.speech_threshold;

        // Обновляем счётчики
        match self.state {
            VadState::Idle => {
                if is_speech {
                    self.speech_frames += 1;
                    self.frame_history.push(true);
                    if self.speech_frames >= self.min_speech_frames {
                        self.state = VadState::Speech;
                        self.speech_frames = 0;
                        return (true, true); // state changed: Idle → Speech
                    }
                } else {
                    self.silence_frames += 1;
                    self.frame_history.push(false);
                }
            }
            VadState::Speech => {
                if is_speech {
                    self.speech_frames += 1;
                    self.frame_history.push(true);
                } else {
                    self.silence_frames += 1;
                    self.frame_history.push(false);
                    if self.silence_frames >= self.min_silence_frames {
                        self.state = VadState::Idle;
                        self.silence_frames = 0;
                        return (true, true); // state changed: Speech → Idle
                    }
                }
            }
        }

        (is_speech, false)
    }

    /// Получить текущее состояние.
    pub fn state(&self) -> &VadState {
        &self.state
    }

    /// Сбросить состояние (например, при прерывании).
    pub fn reset(&mut self) {
        self.state = VadState::Idle;
        self.last_state = VadState::Idle;
        self.speech_frames = 0;
        self.silence_frames = 0;
        self.frame_history.clear();
    }

    /// Рассчитать RMS (Root Mean Square) фрейма.
    fn calculate_rms(samples: &[f32]) -> f32 {
        if samples.is_empty() {
            return 0.0;
        }
        let sum_sq: f32 = samples.iter().map(|s| s * s).sum();
        (sum_sq / samples.len() as f32).sqrt()
    }
}

// ===== Шумоподавление =====

/// Простое шумоподавление на основе спектрального вычитания.
/// Работает с временным сигналом, без FFT (для простоты).
pub struct NoiseSuppressor {
    /// Оценка уровня шума (скользящее среднее RMS в тишине)
    noise_estimate: f32,
    /// Коэффициент подавления шума (0.0 = не подавлять, 1.0 = полностью)
    suppression_factor: f32,
    /// Минимальное значение для предотвращения полной тишины
    noise_floor: f32,
}

impl NoiseSuppressor {
    /// Создать шумоподавитель.
    pub fn new(suppression_factor: f32, noise_floor: f32) -> Self {
        Self {
            noise_estimate: 0.0,
            suppression_factor,
            noise_floor,
        }
    }

    /// Настроить уровень подавления (0.0 = выкл, 1.0 = макс).
    pub fn set_suppression(&mut self, factor: f32) {
        self.suppression_factor = factor.clamp(0.0, 1.0);
    }

    /// Обработать фрейм, подавив шум.
    pub fn process(&mut self, samples: &[f32]) -> Vec<f32> {
        if samples.is_empty() {
            return Vec::new();
        }

        let rms = Self::calculate_rms(samples);

        // Обновляем оценку шума (когда тишина)
        if rms < 0.02 {
            self.noise_estimate = self.noise_estimate * 0.95 + rms * 0.05;
        }

        // Подавляем шум
        let mut output = Vec::with_capacity(samples.len());
        for &sample in samples {
            let signal = (sample - self.noise_estimate).abs();
            let gain = if signal > self.noise_estimate * self.suppression_factor {
                1.0
            } else {
                0.1 * (1.0 - self.suppression_factor)
            };
            let denoised = sample * gain;
            // Предотвращаем полную тишину
            let final_sample = if denoised.abs() < self.noise_floor {
                denoised.signum() * self.noise_floor
            } else {
                denoised
            };
            output.push(final_sample);
        }

        output
    }

    /// Рассчитать RMS.
    fn calculate_rms(samples: &[f32]) -> f32 {
        if samples.is_empty() {
            return 0.0;
        }
        let sum_sq: f32 = samples.iter().map(|s| s * s).sum();
        (sum_sq / samples.len() as f32).sqrt()
    }
}

// ===== High-pass filter для удаления низкочастотного шума =====

/// Простой high-pass фильтр первого порядка.
/// Удаляет низкочастотный шум (гул, фон).
pub struct HighPassFilter {
    cutoff_freq: f32,
    sample_rate: f32,
    previous_output: f32,
    alpha: f32,
}

impl HighPassFilter {
    /// Создать high-pass фильтр.
    pub fn new(cutoff_freq: f32, sample_rate: f32) -> Self {
        let alpha = 2.0 * std::f32::consts::PI * cutoff_freq / sample_rate;
        Self {
            cutoff_freq,
            sample_rate,
            previous_output: 0.0,
            alpha: alpha.min(1.0), // clamp для стабильности
        }
    }

    /// Обработать один образец.
    pub fn process_one(&mut self, sample: f32) -> f32 {
        let output = sample - self.previous_output + self.alpha * self.previous_output;
        self.previous_output = sample;
        output
    }

    /// Обработать весь фрейм.
    pub fn process(&mut self, samples: &[f32]) -> Vec<f32> {
        samples.iter().map(|&s| self.process_one(s)).collect()
    }
}

// ===== Тесты =====

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vad_detects_speech() {
        let mut vad = VoiceDetector::new(0.05, 3, 5);

        // Генерируем тишину
        let silence: Vec<f32> = vec![0.0; 160];
        for _ in 0..5 {
            let (is_speech, changed) = vad.process_frame(&silence);
            assert!(!is_speech);
            assert!(!changed);
        }

        // Генерируем речь (громкий сигнал)
        // changed=true только на 3-м кадре когда состояние меняется Idle→Speech
        let speech: Vec<f32> = vec![0.5; 160];
        for i in 0..3 {
            let (is_speech, changed) = vad.process_frame(&speech);
            assert!(is_speech);
            if i == 2 {
                assert!(changed); // state changed: Idle → Speech
            } else {
                assert!(!changed); // still accumulating
            }
        }

        assert_eq!(vad.state(), &VadState::Speech);
    }

    #[test]
    fn test_vad_detects_silence() {
        let mut vad = VoiceDetector::new(0.05, 3, 5);

        // Сначала речь
        let speech: Vec<f32> = vec![0.5; 160];
        for _ in 0..3 {
            vad.process_frame(&speech);
        }
        assert_eq!(vad.state(), &VadState::Speech);

        // Потом тишина
        let silence: Vec<f32> = vec![0.0; 160];
        for _ in 0..5 {
            vad.process_frame(&silence);
        }

        assert_eq!(vad.state(), &VadState::Idle);
    }

    #[test]
    fn test_noise_suppressor() {
        let mut suppressor = NoiseSuppressor::new(0.5, 0.01);

        // Смешанный сигнал (речь + шум)
        let mixed: Vec<f32> = (0..160).map(|i| {
            0.5 * (i as f32 * 0.01).sin() + 0.05 * (i as f32 * 0.1).sin()
        }).collect();

        let denoised = suppressor.process(&mixed);
        assert_eq!(denoised.len(), mixed.len());
    }

    #[test]
    fn test_high_pass_filter() {
        let mut filter = HighPassFilter::new(200.0, 16000.0);

        // Низкочастотный сигнал должен ослабнуться
        let low_freq: Vec<f32> = (0..160).map(|i| {
            0.5 * (i as f32 * 0.001).sin()
        }).collect();

        let output = filter.process(&low_freq);
        assert_eq!(output.len(), low_freq.len());

        // Выходной сигнал должен быть меньше входного для низких частот
        let input_rms = VoiceDetector::calculate_rms(&low_freq);
        let output_rms = VoiceDetector::calculate_rms(&output);
        assert!(output_rms < input_rms);
    }

    #[test]
    fn test_sensitivity_adjustment() {
        let mut vad = VoiceDetector::new(0.1, 3, 5);

        // Высокая чувствительность → низкий порог
        vad.set_sensitivity(0.0);
        assert!((vad.speech_threshold - 0.01).abs() < 0.001);

        // Низкая чувствительность → высокий порог
        vad.set_sensitivity(1.0);
        assert!((vad.speech_threshold - 0.3).abs() < 0.001);
    }
}
