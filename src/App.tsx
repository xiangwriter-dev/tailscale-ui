import { useState } from 'react';
import { isDesktop } from './api';
import { networkLabels } from './domain';
import { useTailTask } from './useTailTask';
import { Icon,NetworkArt,type IconName } from './components/Icon';
import { Devices,AddDeviceModal } from './components/Devices';
import { RecentTasks,Tasks,TaskDetailModal } from './components/Tasks';
import { Maintenance,Settings } from './components/Maintenance';
import { Status } from './components/Common';
import { RemoteWorkspace } from './components/Remote';
import { AgentPanel } from './components/Agent';
import './remote.css';
import { Autostart } from './components/Autostart';

type Page='devices'|'remote'|'agent'|'tasks'|'repair'|'settings';
const pages:{id:Page;label:string;description:string;icon:IconName}[]=[
  {id:'devices',label:'设备',description:'查看 Tailscale 网络中的设备及其状态',icon:'devices'},
  {id:'remote',label:'远程任务',description:'在已配对的设备上运行命令，查看日志与执行结果',icon:'tasks'},
  {id:'agent',label:'本机执行端',description:'管理这台设备接收远程任务的方式与授权',icon:'network'},
  {id:'tasks',label:'任务记录',description:'每一次本机修复，都有清晰的进展与结果',icon:'tasks'},
  {id:'repair',label:'本机修复',description:'按需检查和修复这台电脑上的应用数据',icon:'repair'},
  {id:'settings',label:'设置',description:'管理刷新偏好，查看版本与本地数据位置',icon:'settings'},
];
export default function App() {
  const [page,setPage]=useState<Page>('devices'),[adding,setAdding]=useState(false);
  const state=useTailTask();
  const current=pages.find(item=>item.id===page)!;
  const online=state.snapshot?.state==='ready'&&!state.cached;
  const navigate=(next:Page)=>{setPage(next);state.setNotice('');};
  return <div className="app-shell">
    <aside className="sidebar"><div className="brand"><span className="brand-mark"><Icon name="network" size={27}/></span><span>xiangwriter<small>远程器 · 设备与任务</small></span></div><nav aria-label="主导航">{pages.map(item=><button key={item.id} className={page===item.id?'nav-item active':'nav-item'} aria-current={page===item.id?'page':undefined} onClick={()=>navigate(item.id)}><Icon name={item.icon}/><span>{item.label}</span>{page===item.id&&<span className="nav-marker"/>}</button>)}</nav><div className="sidebar-bottom"><NetworkArt/><div className="connection-label"><Status tone={online?'success':'muted'}>{online?'网络已连接':state.cached?'历史观测':state.snapshot?networkLabels[state.snapshot.state]:'等待网络连接'}</Status><small>xiangwriter远程器 {state.info?.version||'0.2.0'}</small></div></div></aside>
    <main className="workspace"><header className="page-header"><div><span className="eyebrow">XIANGWRITER WORKSPACE</span><h1>{current.label}</h1><p>{current.description}</p></div>{page==='devices'&&<div className="header-actions"><button className="icon-button refresh-button" aria-label="刷新设备" title="刷新设备" disabled={!isDesktop||state.loading} onClick={()=>void state.refresh()}><Icon name="refresh" className={state.loading?'spin':''}/></button><button className="primary" onClick={()=>setAdding(true)}><Icon name="plus"/>添加设备</button></div>}</header>
    {!isDesktop&&<div className="preview-note" role="status"><Icon name="info" size={18}/><span>界面预览 · 设备读取和本机修复需要在桌面客户端使用。此处不展示模拟设备。</span></div>}
    {state.error&&<div className="alert alert-error" role="alert"><Icon name="info"/><div><strong>操作未完成</strong><p>{state.error}</p></div><button className="icon-button" onClick={()=>state.setError('')} aria-label="关闭错误提示"><Icon name="close" size={18}/></button></div>}
    <div className="notice-region" role="status" aria-live="polite">{state.notice&&<div className="notice"><Icon name="check" size={16}/>{state.notice}<button className="icon-button" aria-label="关闭提示" onClick={()=>state.setNotice('')}><Icon name="close" size={15}/></button></div>}</div>
    {page==='devices'&&<><Devices state={state} onAdd={()=>setAdding(true)} onRemote={()=>navigate('remote')}/><RecentTasks state={state} onAll={()=>navigate('tasks')} onRepair={()=>navigate('repair')}/></>}
    {page==='remote'&&<RemoteWorkspace state={state}/>}
    {page==='agent'&&<><AgentPanel/><Autostart/></>}
    {page==='tasks'&&<Tasks state={state} onRepair={()=>navigate('repair')}/>}
    {page==='repair'&&<Maintenance state={state}/>}
    {page==='settings'&&<Settings state={state}/>}
    <footer className="workspace-footer"><span><Icon name="shield" size={14}/>配对授权 · 本地保存</span><span>xiangwriter远程器 测试版</span></footer>
    </main>
    {adding&&<AddDeviceModal state={state} onClose={()=>setAdding(false)}/>}
    {state.detailId&&<TaskDetailModal state={state}/>}
  </div>;
}
