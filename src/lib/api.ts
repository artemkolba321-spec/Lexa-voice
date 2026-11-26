import { invoke } from '@tauri-apps/api/core';
import type { AppSnapshot, CommandResult, SmartDevice, ToolRequest, ToolResult } from './types';

const isTauri = () => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

const seedSnapshot: AppSnapshot = {
  mode: 'normal',
  listening: false,
  devices: [
    { id: 'light.living_room', name: 'Свет в гостиной', room: 'Гостиная', kind: 'light', isOn: true, brightness: 70 },
    { id: 'light.bedroom', name: 'Свет в спальне', room: 'Спальня', kind: 'light', isOn: false, brightness: 0 },
    { id: 'climate.bedroom', name: 'Термостат', room: 'Спальня', kind: 'thermostat', isOn: true, temperature: 21 },
    { id: 'plug.desk', name: 'Розетка рабочего стола', room: 'Кабинет', kind: 'plug', isOn: false },
    { id: 'sensor.motion_living', name: 'Движение', room: 'Гостиная', kind: 'sensor', isOn: true, reading: 'Движения нет' },
  ],
  tools: [
    { name: 'home.list_devices', description: 'Список доступных устройств умного дома', permission: 'read' },
    { name: 'home.get_state', description: 'Прочитать состояние устройства', permission: 'read' },
    { name: 'home.set_power', description: 'Включить или выключить лампу и розетку', permission: 'device-control' },
    { name: 'home.set_temperature', description: 'Изменить целевую температуру термостата', permission: 'device-control' },
    { name: 'system.get_status', description: 'Проверить состояние ядра Лексы', permission: 'read' },
  ],
  activity: [
    { id: 'a1', time: 'СЕЙЧАС', title: 'Песочница умного дома готова', detail: '5 виртуальных устройств доступны для тестирования', tone: 'success' },
    { id: 'a2', time: 'СЕЙЧАС', title: 'Голосовой стек не подключён', detail: 'Сейчас команды проверяются текстовым демо-парсером', tone: 'info' },
  ],
  runtime: { stt: 'not-connected', llm: 'not-connected', tts: 'not-connected' },
};

let mockSnapshot: AppSnapshot = structuredClone(seedSnapshot);
const clone = <T,>(value: T): T => structuredClone(value);

export async function getSnapshot(): Promise<AppSnapshot> {
  if (isTauri()) return invoke<AppSnapshot>('get_snapshot');
  return clone(mockSnapshot);
}

export async function executeTool(request: ToolRequest): Promise<ToolResult> {
  if (isTauri()) return invoke<ToolResult>('execute_tool', { request });
  const args = request.arguments;
  if (request.toolName === 'home.set_power') {
    const id = String(args.deviceId ?? '');
    const device = mockSnapshot.devices.find((item) => item.id === id);
    if (!device || !['light', 'plug'].includes(device.kind)) {
      return { ok: false, message: `Нельзя управлять устройством ${id || 'без ID'}.`, snapshot: clone(mockSnapshot) };
    }
    device.isOn = Boolean(args.on);
    if (device.kind === 'light') device.brightness = device.isOn ? Number(args.brightness ?? 70) : 0;
    addMockActivity(`${device.name}: ${device.isOn ? 'включено' : 'выключено'}`, 'Управление через реестр инструментов', 'success');
    return { ok: true, message: `${device.name}: ${device.isOn ? 'включено' : 'выключено'}.`, snapshot: clone(mockSnapshot) };
  }
  if (request.toolName === 'home.set_temperature') {
    const device = mockSnapshot.devices.find((item) => item.id === String(args.deviceId ?? ''));
    const temperature = Number(args.temperature);
    if (!device || device.kind !== 'thermostat' || !Number.isFinite(temperature) || temperature < 5 || temperature > 35) {
      return { ok: false, message: 'Неверное устройство или температура.', snapshot: clone(mockSnapshot) };
    }
    device.temperature = temperature;
    addMockActivity(`Температура установлена: ${temperature} °C`, device.name, 'success');
    return { ok: true, message: `Для «${device.name}» установлено ${temperature} °C.`, snapshot: clone(mockSnapshot) };
  }
  if (request.toolName === 'home.list_devices') {
    return { ok: true, message: `В песочнице ${mockSnapshot.devices.length} виртуальных устройств доступно.`, snapshot: clone(mockSnapshot) };
  }
  return { ok: false, message: 'Этот инструмент пока не реализован в браузерной демоверсии.', snapshot: clone(mockSnapshot) };
}

export async function runDemoCommand(text: string): Promise<CommandResult> {
  if (isTauri()) return invoke<CommandResult>('run_demo_command', { text });
  const q = text.trim().toLowerCase();
  if (!q) return { message: 'Скажи или напиши что-нибудь.', snapshot: clone(mockSnapshot) };
  if (q.includes('список устройств') || q.includes('покажи устройства') || q.includes('что с домом')) {
    const names = mockSnapshot.devices.map((device) => device.name).join(', ');
    addMockActivity('Запрошен список устройств', `${mockSnapshot.devices.length} устройств`, 'info');
    return { message: `В песочнице есть: ${names}.`, snapshot: clone(mockSnapshot) };
  }
  const light = mockSnapshot.devices.find((device) => device.kind === 'light' && ((q.includes('гостин') && device.room === 'Гостиная') || (q.includes('спальн') && device.room === 'Спальня')));
  const wantsOn = /(включи|включить|зажги|зажечь)/.test(q);
  const wantsOff = /(выключи|выключить|погаси|погасить)/.test(q);
  if (light && (wantsOn || wantsOff)) {
    const result = await executeTool({ toolName: 'home.set_power', arguments: { deviceId: light.id, on: wantsOn } });
    return { message: result.message, snapshot: result.snapshot };
  }
  if (q.includes('температур') && q.includes('спальн') && /(какая|сколько|текущ)/.test(q)) {
    const device = mockSnapshot.devices.find((item) => item.kind === 'thermostat');
    return { message: `Сейчас у термостата установлено ${device?.temperature ?? 'неизвестно'} °C. Это виртуальное значение.`, snapshot: clone(mockSnapshot) };
  }
  if (q.includes('температур') && q.includes('спальн')) {
    const match = q.match(/(\d{1,2}(?:[.,]\d)?)/);
    if (match) {
      const temperature = Number(match[1].replace(',', '.'));
      const result = await executeTool({ toolName: 'home.set_temperature', arguments: { deviceId: 'climate.bedroom', temperature } });
      return { message: result.message, snapshot: result.snapshot };
    }
  }
  addMockActivity('Команда не распознана', text, 'warning');
  return { message: 'Демо-парсер не распознаёт эту команду. Это не LLM: позже здесь будет планировщик Лексы на локальной Qwen.', snapshot: clone(mockSnapshot) };
}

export async function setProfile(mode: 'normal' | 'gaming'): Promise<AppSnapshot> {
  if (isTauri()) return invoke<AppSnapshot>('set_profile', { mode });
  mockSnapshot.mode = mode;
  addMockActivity(mode === 'gaming' ? 'Игровой профиль включён (демо)' : 'Обычный профиль включён (демо)', 'Никакие системные настройки не изменялись', 'info');
  return clone(mockSnapshot);
}

function addMockActivity(title: string, detail: string, tone: 'success' | 'info' | 'warning') {
  const time = new Date().toLocaleTimeString('ru-RU', { hour: '2-digit', minute: '2-digit' });
  mockSnapshot.activity.unshift({ id: `${Date.now()}-${Math.random()}`, time, title, detail, tone });
  mockSnapshot.activity = mockSnapshot.activity.slice(0, 24);
}

export function updateMockDevice(device: SmartDevice) {
  const index = mockSnapshot.devices.findIndex((item) => item.id === device.id);
  if (index !== -1) mockSnapshot.devices[index] = device;
}
