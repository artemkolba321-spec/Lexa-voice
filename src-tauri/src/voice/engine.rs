/// Голосовой движок Лексы.
///
/// Архитектура:
/// - VoiceEngine хранит только Send+Sync данные (состояние, флаг прослушивания).
/// - start_listening() запускает worker-поток, который владеет
///   AudioCapture, SttEngine и TtsEngine.
/// - speak() запускает TTS в отдельном потоке.

use anyhow::{Context, Result};
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc,
    Arc, Mutex,
};
use std::thread::{self, JoinHandle};

use crate::voice::audio::{AudioCapture, AudioHost, AudioProcessingParams};
use crate::voice::stt::SttEngine;
use crate::voice::tts::TtsEngine;

/// Состояние голосового движка.
#[derive(Debug, Clone, PartialEq)]
pub enum VoiceState {
    Idle,
    Listening,
    Hearing,
    Speaking,
    Error(String),
}

/// События голосового движка.
#[derive(Debug, Clone)]
pub enum VoiceEvent {
    StateChanged(VoiceState),
    WakeWordDetected(String),
    CommandRecognized(String),
    ResponseSpoken(String),
    Error(String),
}

/// Голосовой движок Лексы.
pub struct VoiceEngine {
    state: Arc<Mutex<VoiceState>>,
    event_sender: Option<mpsc::Sender<VoiceEvent>>,
    stt_model_path: PathBuf,
    audio_host: AudioHost,
    listening: Arc<AtomicBool>,
    worker_handle: Option<JoinHandle<()>>,
}

impl VoiceEngine {
    pub fn new(stt_model_path: Option<PathBuf>, audio_host: AudioHost) -> Result<Self> {
        let stt_path = match stt_model_path {
            Some(path) => path,
            None => crate::voice::stt::find_vosk_model()?,
        };

        Ok(Self {
            state: Arc::new(Mutex::new(VoiceState::Idle)),
            event_sender: None,
            stt_model_path: stt_path,
            audio_host,
            listening: Arc::new(AtomicBool::new(false)),
            worker_handle: None,
        })
    }

    /// Запуск постоянного прослушивания.
    pub fn start_listening(
        &mut self,
        event_sender: mpsc::Sender<VoiceEvent>,
        audio_host: AudioHost,
        device_name: Option<String>,
    ) -> Result<()> {
        eprintln!("[Engine] start_listening CALLED");
        if self.listening.load(Ordering::Relaxed) {
            return Ok(());
        }

        let stt_model_path = self.stt_model_path.clone();
        let listening = Arc::clone(&self.listening);
        let state = Arc::clone(&self.state);
        let worker_events = event_sender.clone();
        let device_name_clone = device_name.clone();
        let audio_host_clone = audio_host;

        self.listening.store(true, Ordering::Relaxed);
        self.event_sender = Some(event_sender);

        Self::set_state_shared(
            &state,
            &worker_events,
            VoiceState::Listening,
        );

        let worker_listening = Arc::clone(&listening);

        let worker = thread::spawn(move || {
            Self::audio_worker(
                stt_model_path,
                worker_listening,
                state,
                worker_events,
                audio_host_clone,
                device_name_clone,
            );
        });

        self.worker_handle = Some(worker);

        Ok(())
    }

    /// Рабочий поток: захват аудио, STT, TTS.
    fn audio_worker(
        stt_model_path: PathBuf,
        listening: Arc<AtomicBool>,
        state: Arc<Mutex<VoiceState>>,
        events: mpsc::Sender<VoiceEvent>,
        audio_host: AudioHost,
        device_name: Option<String>,
    ) {
        // Keep diagnostics portable; never assume a Unix-only /tmp path.
        fn log(msg: &str) {
            eprintln!("[Lexa voice] {msg}");
        }

        log("[Engine] audio_worker START");

        // Инициализация STT и TTS в рабочем потоке.
        let stt = match SttEngine::new_continuous(&stt_model_path) {
            Ok(engine) => engine,
            Err(error) => {
                log(&format!("[Engine] STT init FAILED: {}", error));
                let message = format!("Не удалось инициализировать STT: {error}");
                let _ = events.send(VoiceEvent::Error(message));
                listening.store(false, Ordering::Release);
                Self::set_state_shared(&state, &events, VoiceState::Idle);
                return;
            }
        };
        log("[Engine] STT initialized OK");

        log("[Engine] About to init TTS...");
        let tts = match TtsEngine::new(None) {
            Ok(engine) => engine,
            Err(error) => {
                log(&format!("[Engine] TTS init FAILED: {}", error));
                let message = format!("Не удалось инициализировать TTS: {error}");
                let _ = events.send(VoiceEvent::Error(message));
                listening.store(false, Ordering::Release);
                Self::set_state_shared(&state, &events, VoiceState::Idle);
                return;
            }
        };
        log("[Engine] TTS initialized OK");

        let (audio_tx, audio_rx) = mpsc::sync_channel::<Vec<f32>>(16);

        // Запуск захвата аудио.
        log("[Engine] Starting AudioCapture...");
        let capture = match AudioCapture::start(
            Arc::new(move |audio: &[f32]| {
            // The real-time audio callback must not perform disk I/O or logging.
            // Keep it bounded: copy the frame and enqueue it without blocking.
            let _ = audio_tx.try_send(audio.to_vec());
        }),
            audio_host,
            device_name.as_deref(),
            AudioProcessingParams::default(),
        ) {
            Ok(capture) => capture,
            Err(error) => {
                log(&format!("[Engine] AudioCapture FAILED: {}", error));
                let message = format!("Не удалось запустить микрофон: {error}");
                let _ = events.send(VoiceEvent::Error(message));
                listening.store(false, Ordering::Release);
                Self::set_state_shared(
                    &state,
                    &events,
                    VoiceState::Idle,
                );
                return;
            }
        };
        log("[Engine] AudioCapture started OK");

        let mut stt = stt;
        let mut wake_armed = false;
        let mut frame_count = 0u64;

        while listening.load(Ordering::Relaxed) {
            let audio = match audio_rx.recv_timeout(std::time::Duration::from_millis(100)) {
                Ok(audio) => audio,
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            };

            frame_count += 1;
            if frame_count <= 3 || frame_count % 10 == 0 {
                let rms = (audio.iter().map(|sample| sample * sample).sum::<f32>()
                    / audio.len().max(1) as f32).sqrt();
                let peak = audio.iter().fold(0.0_f32, |max, sample| max.max(sample.abs()));
                eprintln!(
                    "[Engine] audio block #{}: {} samples at 16 kHz, RMS={:.5}, peak={:.5}",
                    frame_count, audio.len(), rms, peak
                );
            }

            if !listening.load(Ordering::Relaxed) {
                break;
            }

            let recognized = stt.process_audio(&audio);

            if let Some(ref text) = recognized {
                eprintln!("[Engine] STT recognized: \"{}\" (wake_armed={})", text, wake_armed);
            }

            let Some(recognized_text) = recognized else {
                continue;
            };

            let text = recognized_text.trim().to_string();

            if text.is_empty() {
                continue;
            }

            // Ожидаем ключевое слово.
            if !wake_armed {
                if !SttEngine::is_wake_word(&text.to_lowercase()) {
                    continue;
                }

                let _ = events.send(VoiceEvent::WakeWordDetected(text.clone()));

                Self::set_state_shared(
                    &state,
                    &events,
                    VoiceState::Hearing,
                );

                wake_armed = true;

                // Команда могла быть произнесена вместе с ключевым словом.
                if let Some(command) = SttEngine::extract_command_after_wake_word(&text) {
                    Self::handle_command(
                        command,
                        &tts,
                        &listening,
                        &state,
                        &events,
                    );
                    wake_armed = false;
                }

                continue;
            }

            // Ключевое слово уже было произнесено:
            // следующая распознанная фраза считается командой.
            let command = text.trim().to_string();

            if command.is_empty() {
                continue;
            }

            Self::handle_command(
                command,
                &tts,
                &listening,
                &state,
                &events,
            );

            wake_armed = false;
        }

        drop(capture);
    }

    /// Обработка команды и озвучивание ответа.
    fn handle_command(
        command: String,
        tts: &TtsEngine,
        listening: &Arc<AtomicBool>,
        state: &Arc<Mutex<VoiceState>>,
        events: &mpsc::Sender<VoiceEvent>,
    ) {
        let _ = events.send(VoiceEvent::CommandRecognized(command.clone()));

        let response = crate::utils::parse_demo_command(&command);

        Self::set_state_shared(
            state,
            events,
            VoiceState::Speaking,
        );

        let speech_result = tts.speak(&response);

        match speech_result {
            Ok(()) => {
                let _ = events.send(VoiceEvent::ResponseSpoken(response));
            }
            Err(error) => {
                let message = format!("Ошибка озвучивания: {error}");
                let _ = events.send(VoiceEvent::Error(message));
            }
        }

        // После ответа продолжаем слушать микрофон.
        if listening.load(Ordering::Relaxed) {
            Self::set_state_shared(
                state,
                events,
                VoiceState::Listening,
            );
        }
    }

    /// Изменение состояния и отправка события.
    fn set_state_shared(
        state: &Arc<Mutex<VoiceState>>,
        events: &mpsc::Sender<VoiceEvent>,
        new_state: VoiceState,
    ) {
        if let Ok(mut current) = state.lock() {
            *current = new_state.clone();
        }
        let _ = events.send(VoiceEvent::StateChanged(new_state));
    }

    /// Остановка прослушивания.
    pub fn stop_listening(&mut self) {
        self.listening.store(false, Ordering::Relaxed);

        if let Some(worker) = self.worker_handle.take() {
            let _ = worker.join();
        }

        if let Some(sender) = &self.event_sender {
            Self::set_state_shared(
                &self.state,
                sender,
                VoiceState::Idle,
            );
        } else if let Ok(mut state) = self.state.lock() {
            *state = VoiceState::Idle;
        }
    }

    /// Произнести произвольный текст.
    pub fn speak(&self, text: &str) -> Result<()> {
        let sender = self.event_sender.as_ref();

        if let Some(sender) = sender {
            Self::set_state_shared(
                &self.state,
                sender,
                VoiceState::Speaking,
            );
        } else if let Ok(mut state) = self.state.lock() {
            *state = VoiceState::Speaking;
        }

        let result = (|| {
            let tts = TtsEngine::new(None)
                .context("Не удалось инициализировать TTS для озвучивания")?;
            tts.speak(text)?;
            Ok(())
        })();

        if let Some(sender) = sender {
            if result.is_ok() {
                let _ = sender.send(VoiceEvent::ResponseSpoken(text.to_string()));
            }

            Self::set_state_shared(
                &self.state,
                sender,
                if result.is_ok() {
                    VoiceState::Idle
                } else {
                    VoiceState::Error("Ошибка озвучивания".to_string())
                },
            );
        } else if let Ok(mut state) = self.state.lock() {
            *state = if result.is_ok() {
                VoiceState::Idle
            } else {
                VoiceState::Error("Ошибка озвучивания".to_string())
            };
        }

        result
    }

    /// Обработка текстовой команды.
    pub fn process_command(&self, command: &str) -> Result<String> {
        Ok(crate::utils::parse_demo_command(command))
    }

    /// Получить текущее состояние.
    pub fn get_state(&self) -> VoiceState {
        self.state
            .lock()
            .map(|state| state.clone())
            .unwrap_or_else(|_| {
                VoiceState::Error("Ошибка доступа к состоянию".to_string())
            })
    }

    /// Проверить, активно ли прослушивание.
    pub fn is_active(&self) -> bool {
        self.listening.load(Ordering::Relaxed)
    }
}

impl Drop for VoiceEngine {
    fn drop(&mut self) {
        self.stop_listening();
    }
}
