export type PageId = 'overview' | 'smart-home' | 'tools' | 'activity' | 'settings';
export type DeviceKind = 'light' | 'thermostat' | 'plug' | 'sensor';

export interface SmartDevice {
  id: string;
  name: string;
  room: string;
  kind: DeviceKind;
  isOn: boolean;
  brightness?: number;
  temperature?: number;
  reading?: string;
}

export interface ToolDefinition {
  name: string;
  description: string;
  permission: 'read' | 'device-control' | 'system';
}

export interface ActivityItem {
  id: string;
  time: string;
  title: string;
  detail: string;
  tone: 'success' | 'info' | 'warning';
}

export interface AppSnapshot {
  mode: 'normal' | 'gaming';
  listening: boolean;
  devices: SmartDevice[];
  tools: ToolDefinition[];
  activity: ActivityItem[];
  runtime: {
    stt: 'not-connected' | 'connected';
    llm: 'not-connected' | 'connected';
    tts: 'not-connected' | 'connected';
  };
}

export interface ToolRequest {
  toolName: string;
  arguments: Record<string, unknown>;
}

export interface ToolResult {
  ok: boolean;
  message: string;
  snapshot: AppSnapshot;
}

export interface CommandResult {
  message: string;
  snapshot: AppSnapshot;
}
