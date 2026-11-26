/// Голосовая подсистема Лексы.
///
/// Компоненты:
/// - ears: захват и обработка звука (VAD, шумоподавление, чувствительность);
/// - audio: захват звука с микрофона через CPAL;
/// - stt: распознавание речи через Vosk и обнаружение слова «Лекса»;
/// - tts: синтез речи через AnyTTS с использованием Qwen3-TTS;
/// - engine: главный голосовой пайплайн.

pub mod audio;
pub mod ears;
pub mod stt;
pub mod tts;
pub mod engine;

pub use engine::{VoiceEngine, VoiceEvent};
pub use stt::find_vosk_model;
