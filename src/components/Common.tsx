import { useEffect, useId, useRef, type ReactNode } from 'react';
import { Icon, type IconName } from './Icon';
import { taskLabels } from '../domain';
import type { Task } from '../types';

export function Status({tone='muted',children}:{tone?:'success'|'warning'|'danger'|'info'|'muted';children:ReactNode}) {
  return <span className={'status status-'+tone}><span className="status-dot"/>{children}</span>;
}
export function TaskStatus({task}:{task:Task}) {
  if(task.persistence_warning) return <Status tone="warning">状态待核实</Status>;
  const tone=task.state==='succeeded'?'success':task.state==='failed'?'danger':task.state==='interrupted'?'warning':task.state==='running'?'info':'muted';
  return <Status tone={tone}>{taskLabels[task.state] || '状态未知'}</Status>;
}
export function EmptyState({icon='devices',title,children,action}:{icon?:IconName;title:string;children:ReactNode;action?:ReactNode}) {
  return <div className="empty-state"><div className="empty-icon"><Icon name={icon} size={30}/></div><h3>{title}</h3><p>{children}</p>{action}</div>;
}
export function Modal({title,children,onClose,wide=false}:{title:string;children:ReactNode;onClose:()=>void;wide?:boolean}) {
  const ref=useRef<HTMLDialogElement>(null);
  const titleId=useId();
  const close=useRef(onClose); close.current=onClose;
  useEffect(()=>{
    const prior=document.activeElement as HTMLElement|null;
    const dialog=ref.current;
    dialog?.showModal();
    return ()=>{ dialog?.close(); prior?.focus(); };
  },[]);
  return <dialog ref={ref} aria-labelledby={titleId} className={wide?'modal modal-wide':'modal'} onCancel={e=>{e.preventDefault();close.current();}} onClick={e=>{if(e.target===e.currentTarget) close.current();}}>
    <div className="modal-header"><h2 id={titleId}>{title}</h2><button className="icon-button" onClick={onClose} aria-label={'关闭'+title}><Icon name="close"/></button></div>
    <div className="modal-body">{children}</div>
  </dialog>;
}
export function formatTime(value?:string|null) {
  if(!value) return '—';
  const date=new Date(value);
  return Number.isNaN(date.getTime())?'未知时间':date.toLocaleString('zh-CN',{month:'2-digit',day:'2-digit',hour:'2-digit',minute:'2-digit',hour12:false});
}
export function OsBadge({os,large=false}:{os:string;large?:boolean}) {
  const name=os.toLowerCase();
  return <span className={'os-badge '+(large?'os-badge-large ':'')+(name.includes('windows')?'os-windows':name.includes('mac')?'os-mac':'os-other')} aria-hidden="true">
    {name.includes('windows')?<svg viewBox="0 0 24 24" fill="currentColor"><path d="M2 4l9-1v8H2zm11-1 9-1v9h-9zM2 13h9v8l-9-1zm11 0h9v9l-9-1z"/></svg>:<Icon name="devices" size={large?30:21}/>}
  </span>;
}
