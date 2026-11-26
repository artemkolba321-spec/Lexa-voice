import { useEffect, useState, useRef } from 'react';
import type { ReactNode } from 'react';
import {
  AudioWaveform, Bot, Check, Cpu, Mic, MicOff, MessageSquare, Puzzle,
  Send, ShieldCheck, Sparkles, Volume2, Zap,
} from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';

const isTauri = () => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

type Page = 'home' | 'modules' | 'settings';
type AudioDevice = { name: string; is_default: boolean };
type AudioHost = { name: string; available: boolean };
type Preferences = {
  wakePhrase: string; voice: string; model: string; audioHost: string; microphone: string;
  launchAtLogin: boolean; startListening: boolean; soundEffects: boolean;
  pushToTalk: boolean; confirmActions: boolean; communityModules: boolean;
  theme: string; hotkey: string;
};
const DEFAULTS: Preferences = {
  wakePhrase: 'Лекса', voice: 'Байя', model: 'Qwen · локальная модель', audioHost: 'ALSA', microphone: 'Системное устройство по умолчанию',
  launchAtLogin: true, startListening: true, soundEffects: true, pushToTalk: false,
  confirmActions: true, communityModules: false,   theme: 'Ночная синь', hotkey: 'Правый Alt',
};
type ChatMessage = {
  id: string;
  role: 'user' | 'lexa';
  text: string;
  time: string;
};
const MODULES = [
  { icon: Puzzle, name: 'Модули сообщества', description: 'Подключай новые инструменты и расширения', tag: 'COMING SOON' },
];

function loadPreferences(): Preferences {
  try { return { ...DEFAULTS, ...JSON.parse(localStorage.getItem('lexa-preferences') || '{}') }; }
  catch { return DEFAULTS; }
}

export default function App() {
  const [page, setPage] = useState<Page>('home');
  const [active, setActive] = useState(false);
  const [prefs, setPrefs] = useState<Preferences>(loadPreferences);
  const [toast, setToast] = useState('');
  const [chat, setChat] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState('');
  const [listening, setListening] = useState(false);
  const [voiceStatus, setVoiceStatus] = useState<string>('idle');
  const [audioDevices, setAudioDevices] = useState<AudioDevice[]>([]);
  const [audioHosts, setAudioHosts] = useState<AudioHost[]>([]);
  const chatRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    localStorage.setItem('lexa-preferences', JSON.stringify(prefs));
  }, [prefs]);
  // Загрузка списка аудиоустройств и хостов при входе в Tauri
  useEffect(() => {
    if (!isTauri()) return;
    // Загружаем список хостов
    invoke('list_audio_hosts').then((hosts: AudioHost[]) => {
      setAudioHosts(hosts);
    }).catch(() => {});
    // Загружаем устройства для выбранного хоста
    const reloadDevices = () => {
      const host = prefs.audioHost.toLowerCase();
      invoke('list_audio_devices', { audioHostName: host }).then((devices: AudioDevice[]) => {
        setAudioDevices(devices);
      }).catch(() => {});
    };
    reloadDevices();
  }, [prefs.audioHost]);
  useEffect(() => {
    const keyHandler = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        setActive(false);
        stopListening();
      }
      if (event.code === 'Space' && !(event.target instanceof HTMLInputElement) && !(event.target instanceof HTMLSelectElement) && !(event.target instanceof HTMLTextAreaElement)) {
        event.preventDefault(); setActive(value => !value);
      }
    };
    window.addEventListener('keydown', keyHandler);
    return () => window.removeEventListener('keydown', keyHandler);
  }, []);
  useEffect(() => {
    if (!toast) return;
    const timer = window.setTimeout(() => setToast(''), 2800);
    return () => window.clearTimeout(timer);
  }, [toast]);
  // Автопрокрутка чата
  useEffect(() => {
    if (chatRef.current) {
      chatRef.current.scrollTop = chatRef.current.scrollHeight;
    }
  }, [chat]);

  const update = <K extends keyof Preferences>(key: K, value: Preferences[K]) => setPrefs(old => ({ ...old, [key]: value }));
  
  const activate = async () => {
    if (active) {
      setActive(false);
      await stopListening();
      setToast('Лекса вернулась в режим ожидания');
    } else {
      setActive(true);
      // Try to start the Tauri voice engine
      if (isTauri()) {
        try {
          const mic = prefs.microphone === 'Системное устройство по умолчанию' ? null : prefs.microphone;
          await invoke('start_listen', { sttModelPath: null, audioHostName: prefs.audioHost.toLowerCase(), deviceName: mic });
          setToast('Лекса слушает. Скажи "Лекса" и команду.');
        } catch (error) {
          const msg = error instanceof Error ? error.message : String(error);
          setToast(`Голосовой движок не подключён — ${msg}`);
        }
      } else {
        setToast('Демо-активация: голосовое ядро пока не подключено');
      }
    }
  };

  const stopListening = async () => {
    setListening(false);
    setVoiceStatus('idle');
    if (isTauri()) {
      try { await invoke('stop_listen'); } catch {}
    }
  };

  const startListeningVoice = async () => {
    setListening(true);
    setVoiceStatus('listening');
    if (isTauri()) {
      try {
        const mic = prefs.microphone === 'Системное устройство по умолчанию' ? null : prefs.microphone;
        await invoke('start_listen', { sttModelPath: null, audioHostName: prefs.audioHost.toLowerCase(), deviceName: mic });
      } catch (e) {
        setListening(false);
        setVoiceStatus('error');
        setToast('Не удалось запустить микрофон');
      }
    }
  };

  const sendCommand = async () => {
    const text = input.trim();
    if (!text) return;
    
    const userMsg: ChatMessage = {
      id: Date.now().toString(),
      role: 'user',
      text,
      time: new Date().toLocaleTimeString('ru-RU', { hour: '2-digit', minute: '2-digit' }),
    };
    setChat(prev => [...prev, userMsg]);
    setInput('');

    let response = '';
    if (isTauri()) {
      try {
        const result = await invoke<{ response: string }>('text_command', { text });
        response = result.response;
      } catch (error) {
        const msg = error instanceof Error ? error.message : String(error);
        setToast(`Ошибка ядра: ${msg}`);
        response = 'Не удалось обработать команду.';
      }
    } else {
      // Демо-ответы для браузера
      const lower = text.toLowerCase();
      if (lower.includes('привет')) response = 'Привет! Я Лекса, твой локальный голосовой помощник.';
      else if (lower.includes('как дела')) response = 'У меня всё отлично! Готова помогать.';
      else if (lower.includes('кто ты')) response = 'Я Лекса — локальный голосовой помощник на Rust. Чем могу помочь?';
      else if (lower.includes('время')) response = `Сейчас ${new Date().toLocaleTimeString('ru-RU')}.`;
      else response = 'Я пока не знаю, как выполнить эту команду. Это демо-режим.';
    }

    const lexaMsg: ChatMessage = {
      id: (Date.now() + 1).toString(),
      role: 'lexa',
      text: response,
      time: new Date().toLocaleTimeString('ru-RU', { hour: '2-digit', minute: '2-digit' }),
    };
    setChat(prev => [...prev, lexaMsg]);
  };

  const handleVoiceCommand = async () => {
    // Use Tauri voice engine if available
    if (isTauri()) {
      try {
        const mic = prefs.microphone === 'Системное устройство по умолчанию' ? null : prefs.microphone;
        await invoke('start_listen', { sttModelPath: null, audioHostName: prefs.audioHost.toLowerCase(), deviceName: mic });
        setListening(true);
        setVoiceStatus('listening');
        setToast('Лекса слушает. Скажи "Лекса" и команду.');
      } catch (e) {
        setToast('Не удалось запустить голосовой движок');
        setVoiceStatus('error');
      }
      return;
    }
    
    // Fallback to Web Speech API in browser
    if ('webkitSpeechRecognition' in window || 'SpeechRecognition' in window) {
      const SpeechRecognition = (window as any).SpeechRecognition || (window as any).webkitSpeechRecognition;
      const recognition = new SpeechRecognition();
      recognition.lang = 'ru-RU';
      recognition.interimResults = false;
      recognition.maxAlternatives = 1;

      recognition.onresult = (event: any) => {
        const transcript = event.results[0][0].transcript;
        setInput(transcript);
        setTimeout(() => sendCommand(), 300);
      };

      recognition.onerror = () => {
        setListening(false);
        setVoiceStatus('error');
      };

      recognition.onend = () => {
        setListening(false);
        setVoiceStatus('idle');
      };

      recognition.start();
      setVoiceStatus('hearing');
    } else {
      setToast('Web Speech API не поддерживается в этом браузере');
    }
  };

  return <main className="lexa-shell">
    <header className="header">
      <button className="brand" onClick={() => setPage('home')} aria-label="Lexa — главная">
        <span className="brand-symbol"><span /></span><span className="brand-name">lexa<span className="brand-period">.</span></span>
      </button>
      <nav className="page-switch" aria-label="Разделы">
        <button className={page === 'home' ? 'switch active' : 'switch'} onClick={() => setPage('home')}>Главная</button>
        <button className={page === 'modules' ? 'switch active' : 'switch'} onClick={() => setPage('modules')}>Модули</button>
        <button className={page === 'settings' ? 'switch active' : 'switch'} onClick={() => setPage('settings')}>Настройки</button>
      </nav>
    </header>

    {page === 'home' && <section className="page home-page">
      <div className="home-heading">
        <div className="overline"><span className={active ? 'status-dot listening' : 'status-dot'} /> {active ? 'АКТИВНОЕ ПРОСЛУШИВАНИЕ' : 'ЛОКАЛЬНЫЙ АССИСТЕНТ'}</div>
        <h1>{active ? 'Я слушаю.' : 'Я рядом.'}</h1>
        <p>{active ? 'Говори, я здесь.' : 'Чем могу помочь?'}</p>
      </div>
      <div className="orb-zone">
        <div className={`orb-aura ${active ? 'expanded' : ''}`} />
        <button className={`orb-control ${active ? 'orb-active' : ''}`} onClick={activate} aria-label={active ? 'Остановить Лексу' : 'Активировать Лексу'}>
          <span className="orb-shell">
            <span className="orb-gloss" />
            <span className="orb-inner-ring" />
            <span className="orb-white-core" />
          </span>
          <span className="orb-ripple ripple-one" /><span className="orb-ripple ripple-two" />
        </button>
      </div>
      <div className="wake-prompt ${active ? 'wake-prompt-active' : ''}">
        <span className="waveform"><i/><i/><i/><i/><i/></span>
        <span>{active ? 'Лекса активирована' : <>Скажи <strong>«{prefs.wakePhrase || 'Лекса'}»</strong></>}</span>
      </div>
      <p className="orb-hint">{active ? 'Нажми на шар или Esc, чтобы остановить' : 'Нажми на шар, чтобы попробовать'}</p>
      
      {/* Чат и голосовой ввод */}
      <div className="chat-section">
        <div className="chat-messages" ref={chatRef}>
          {chat.length === 0 && <div className="chat-placeholder">
            <MessageSquare size={20}/>
            <span>Начни диалог с Лексой — напиши или скажи команду</span>
          </div>}
          {chat.map(msg => (
            <div key={msg.id} className={`chat-bubble ${msg.role}`}>
              <div className="chat-bubble-header">
                <span className="chat-bubble-role">{msg.role === 'user' ? 'Ты' : 'Лекса'}</span>
                <span className="chat-bubble-time">{msg.time}</span>
              </div>
              <div className="chat-bubble-text">{msg.text}</div>
            </div>
          ))}
        </div>
        <div className="chat-input-row">
          <button
            className={`mic-btn ${listening ? 'mic-active' : ''}`}
            onClick={listening ? stopListening : handleVoiceCommand}
            aria-label={listening ? 'Остановить запись' : 'Голосовой ввод'}
            title={listening ? 'Остановить' : 'Голосовой ввод (Web Speech API)'}
          >
            {listening ? <MicOff size={18}/> : <Mic size={18}/>}
          </button>
          <input
            className="chat-input"
            value={input}
            onChange={e => setInput(e.target.value)}
            onKeyDown={e => e.key === 'Enter' && sendCommand()}
            placeholder="Напиши команду Лексе..."
            aria-label="Ввод команды"
          />
          <button className="send-btn" onClick={sendCommand} disabled={!input.trim()} aria-label="Отправить">
            <Send size={16}/>
          </button>
        </div>
        {listening && <div className="voice-status">
          <span className="status-dot listening"/>
          <span>{voiceStatus === 'hearing' ? 'Слушаю...' : voiceStatus === 'error' ? 'Ошибка микрофона' : 'Готово. Жду команду...'}</span>
        </div>}
      </div>
      <div className="privacy-strip"><span className="privacy-icon"><ShieldCheck size={16}/></span><span><strong>Твоё пространство — твоё</strong><small>Локальная архитектура · без облака по умолчанию</small></span><span className="privacy-status"><i/> LOCAL</span></div>
      <div className="home-bottom-note"><Sparkles size={13}/> Создана, чтобы расти вместе с тобой</div>
    </section>}

    {page === 'modules' && <section className="page subpage modules-page">
      <div className="overline"><span className="status-dot"/> РАСШИРЯЕМОЕ ЯДРО</div>
      <h1>Модули</h1><p className="page-description">Возможности Лексы можно будет подключать по мере готовности.</p>
      <div className="coming-hero"><div className="coming-orb"><Puzzle size={22}/></div><div><span className="coming-label">THE NEXT CHAPTER</span><h2>Больше, чем ассистент.</h2><p>Модульная система в разработке</p></div></div>
      <div className="module-stack">{MODULES.map(({icon: Icon, name, description, tag}) => <article className="module-row" key={name}>
        <span className="module-icon"><Icon size={18}/></span><span className="module-text"><strong>{name}</strong><small>{description}</small></span><span className="coming-tag">{tag}</span>
      </article>)}</div>
      <div className="module-note"><Zap size={15}/><span>Каждый модуль будет подключаться отдельно, с собственными разрешениями.</span></div>
    </section>}

    {page === 'settings' && <section className="page subpage settings-page">
      <div className="overline"><span className="status-dot"/> ПЕРСОНАЛИЗАЦИЯ</div>
      <h1>Настройки</h1><p className="page-description">Настрой Лексу под себя. Изменения сохраняются локально.</p>

      <SettingsGroup title="Голос и активация" icon={<AudioWaveform size={15}/> }>
        <SettingRow title="Аудиосервер" description="Драйвер захвата звука"><select className="field select-field" value={prefs.audioHost} onChange={e => update('audioHost', e.target.value)}>{audioHosts.map(h => <option key={h.name} value={h.name} disabled={!h.available}>{h.name}{h.available ? '' : ' (недоступен)'}</option>)}</select></SettingRow>
        <SettingRow title="Устройство ввода" description="Микрофон для распознавания"><select className="field select-field" value={prefs.microphone} onChange={e => update('microphone', e.target.value)}>{audioDevices.map(d => <option key={d.name} value={d.name}>{d.name}{d.is_default ? ' (по умолчанию)' : ''}</option>)}</select></SettingRow>
        <SettingRow title="Активационная фраза" description="Как позвать Лексу"><input className="field" value={prefs.wakePhrase} onChange={e => update('wakePhrase', e.target.value)} /></SettingRow>
        <SettingRow title="Режим прослушивания" description="Ожидать активационную фразу"><Toggle checked={prefs.startListening} onChange={v => update('startListening', v)}/></SettingRow>
        <SettingRow title="Режим push-to-talk" description="Удерживать клавишу для речи"><Toggle checked={prefs.pushToTalk} onChange={v => update('pushToTalk', v)}/></SettingRow>
        <SettingRow title="Горячая клавиша" description="Быстрая активация"><select className="field" value={prefs.hotkey} onChange={e => update('hotkey', e.target.value)}><option>Правый Alt</option><option>Правый Ctrl</option><option>F9</option><option>Caps Lock</option></select></SettingRow>
      </SettingsGroup>

      <SettingsGroup title="Голосовой ответ" icon={<Volume2 size={15}/> }>
        <SettingRow title="Голос" description="Голос локального синтеза речи"><select className="field" value={prefs.voice} onChange={e => update('voice', e.target.value)}><option>Байя</option><option>Алиса</option><option>Нейтральный</option></select></SettingRow>
        <SettingRow title="Звуки интерфейса" description="Короткие звуковые сигналы"><Toggle checked={prefs.soundEffects} onChange={v => update('soundEffects', v)}/></SettingRow>
      </SettingsGroup>

      <SettingsGroup title="Локальный интеллект" icon={<Bot size={15}/> }>
        <SettingRow title="Модель ИИ" description="Модель для генерации ответов"><select className="field select-field" value={prefs.model} onChange={e => update('model', e.target.value)}><option>Qwen · локальная модель</option><option>Qwen3 · MoE</option><option>Другая GGUF-модель</option></select></SettingRow>
        <SettingRow title="Подтверждать действия" description="Спрашивать перед системными командами"><Toggle checked={prefs.confirmActions} onChange={v => update('confirmActions', v)}/></SettingRow>
      </SettingsGroup>

      <SettingsGroup title="Приложение" icon={<Cpu size={15}/> }>
        <SettingRow title="Оформление" description="Цветовая тема интерфейса"><select className="field" value={prefs.theme} onChange={e => update('theme', e.target.value)}><option>Ночная синь</option><option>Графит</option><option>Светлая</option></select></SettingRow>
        <SettingRow title="Запуск вместе с системой" description="Автозапуск при входе в Omarchy"><Toggle checked={prefs.launchAtLogin} onChange={v => update('launchAtLogin', v)}/></SettingRow>
        <SettingRow title="Модули сообщества" description="Разрешить сторонние расширения"><Toggle checked={prefs.communityModules} onChange={v => update('communityModules', v)}/></SettingRow>
      </SettingsGroup>

      <div className="settings-disclaimer"><ShieldCheck size={16}/><p><strong>Настройки аудио применяются при запуске голосового движка</strong><br/>Выберите аудиосервер и устройство ввода, затем нажмите на шар на главной странице.</p></div>
      <button className="reset-button" onClick={() => { setPrefs(DEFAULTS); setToast('Настройки сброшены'); }}>Сбросить настройки</button>
      <div className="version-line">LEXA <span>0.1.0 · LOCAL BUILD</span></div>
    </section>}
    {toast && <div className="toast"><Check size={14}/>{toast}</div>}
  </main>;
}

function SettingsGroup({title, icon, children}: {title: string; icon: ReactNode; children: ReactNode}) {
  return <section className="settings-group"><div className="group-heading"><span>{icon}</span>{title}</div>{children}</section>;
}
function SettingRow({title, description, children}: {title: string; description: string; children: ReactNode}) {
  return <div className="setting-row"><span className="setting-copy"><strong>{title}</strong><small>{description}</small></span>{children}</div>;
}
function Toggle({checked, onChange}: {checked: boolean; onChange: (value: boolean) => void}) {
  return <button type="button" className={`toggle ${checked ? 'toggle-on' : ''}`} role="switch" aria-checked={checked} onClick={() => onChange(!checked)}><span/></button>;
}
