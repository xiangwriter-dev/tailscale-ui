import type { Device } from '../types';
import type { TailTaskState } from '../useTailTask';
import { deviceName } from '../domain';
import { Icon } from './Icon';

export function RemoteDesktopButton({state,device,primary=false}:{state:TailTaskState;device:Device;primary?:boolean}) {
  const reason=state.desktopUnavailable(device);
  const busy=state.desktopNode===device.node_id;
  return <span title={reason||'打开 Windows 系统远程桌面，无需任务执行端配对'} className="rdp-button-wrap">
    <button className={primary?'primary rdp-button':'rdp-button'} aria-label={'远程桌面 '+deviceName(device)}
      disabled={!!reason||state.desktopNode!==null} onClick={()=>void state.connectDesktop(device)}>
      {primary&&<Icon name="devices" size={17}/>} {busy?'正在打开…':'远程桌面'}
    </button>
  </span>;
}
