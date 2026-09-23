import type { Device } from './types';

export function deviceName(device: Device): string { return device.alias || device.name; }
export function filterDevices(devices: Device[], search: string, status: string, cached = false): Device[] {
  const query = search.trim().toLocaleLowerCase();
  return devices.filter(device => {
    const match = [device.alias, device.name, device.dns_name, ...device.addresses].join(' ').toLocaleLowerCase().includes(query);
    return match && (status === 'all' || (status === 'online' && !cached && device.online === true && device.visible)
      || (status === 'offline' && (cached || device.online !== true || !device.visible)) || (status === 'favorites' && device.favorite));
  }).sort((a,b) => Number(b.favorite) - Number(a.favorite) || deviceName(a).localeCompare(deviceName(b),'zh-CN'));
}
export function onlineLabel(device: Device, cached: boolean): string {
  if (cached) return '历史观测';
  if (!device.visible) return '历史观测';
  return device.online === true ? '网络在线' : device.online === false ? '网络离线' : '状态未知';
}
export const taskLabels: Record<string,string> = { queued:'已排队',running:'运行中',succeeded:'已完成',failed:'失败',interrupted:'已中断' };
export const networkLabels: Record<string,string> = { ready:'已连接',not_installed:'未安装 Tailscale',login_required:'需要登录',
  stopped:'连接已停止',service_unavailable:'服务不可用',permission_denied:'读取权限不足',unknown:'未知状态',approval_required:'等待设备审批',starting:'正在连接' };
