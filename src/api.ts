import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { AppInfo, Device, NetworkSnapshot, RepairAction, Task, TaskDetail } from './types';

export const isDesktop = isTauri();

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!isDesktop) throw new Error('这是浏览器预览，不能访问 Tailscale 或本地数据库。请启动桌面客户端验证功能。');
  return invoke<T>(command, args);
}

export const api = {
  onStorageError: (handler: (message: string) => void): Promise<() => void> => isDesktop
    ? listen<{message: string}>('repair-storage-error', event => handler(event.payload.message))
    : Promise.resolve(() => {}),
  info: () => call<AppInfo>('app_info'),
  refresh: () => call<NetworkSnapshot>('refresh_network'),
  cached: (contextId: string) => call<Device[]>('cached_devices', { contextId }),
  devices: (contextId: string, nodeId: string, alias: string, favorite: boolean) =>
    call<void>('update_device', { contextId, nodeId, alias, favorite }),
  tasks: (offset = 0) => call<Task[]>('list_tasks', { offset }),
  detail: (id: string) => call<TaskDetail>('task_detail', { id }),
  repair: (action: RepairAction, requestId: string) =>
    call<Task>('submit_repair', { request: { request_id: requestId, action } }),
  settings: () => call<{ refresh_seconds: string }>('get_settings'),
  saveSetting: (value: string) => call<void>('save_setting', { key: 'refresh_seconds', value }),
};
