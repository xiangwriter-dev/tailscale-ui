import { useState } from 'react';
import { RemoteDesktopButton } from './RemoteDesktopButton';
import { isDesktop } from '../api';
import { deviceName,filterDevices,networkLabels,onlineLabel } from '../domain';
import type { Device } from '../types';
import type { TailTaskState } from '../useTailTask';
import { Icon,NetworkArt } from './Icon';
import { EmptyState,formatTime,Modal,OsBadge,Status } from './Common';

function DeviceStatus({device,cached}:{device:Device;cached:boolean}) {
  return <Status tone={!cached&&device.visible&&device.online===true?'success':'muted'}>{onlineLabel(device,cached)}</Status>;
}
export function DeviceDetail({state,onRemote}:{state:TailTaskState;onRemote?:()=>void}) {
  const device=state.selected!;
  const [alias,setAlias]=useState(device.alias);
  const [saving,setSaving]=useState(false);
  return <aside className="panel device-detail" aria-label="所选设备详情">
    <div className="detail-top"><OsBadge os={device.os} large/><div className="min-zero"><h2 className="wrap">{deviceName(device)}</h2><p>{device.os||'未知系统'}{device.is_self?' · 本机':''}</p></div><button className="icon-button detail-close" onClick={()=>state.setSelectedId(null)} aria-label="关闭设备详情"><Icon name="close"/></button></div>
    <DeviceStatus device={device} cached={state.cached}/>
    <dl className="detail-fields"><div><dt>原始名称</dt><dd>{device.name}</dd></div><div><dt>设备标识</dt><dd><code>{device.node_id}</code></dd></div><div><dt>网络地址</dt><dd>{device.addresses.length?device.addresses.map(address=><code key={address}>{address}<br/></code>):'未知'}</dd></div><div><dt>DNS 名称</dt><dd>{device.dns_name||'未提供'}</dd></div><div><dt>最后在线</dt><dd>{formatTime(device.last_seen)}</dd></div><div><dt>观测时间</dt><dd>{formatTime(device.observed_at)}</dd></div></dl>
    <form className="alias-form" onSubmit={async e=>{e.preventDefault();setSaving(true);await state.updateDevice(device,alias,device.favorite);setSaving(false);}}><label htmlFor="device-alias">本地别名</label><div className="input-action"><input id="device-alias" value={alias} onChange={e=>setAlias(e.target.value)} maxLength={120} placeholder="给设备取个好记的名字" disabled={state.cached||!device.visible}/><button disabled={saving||state.cached||!device.visible} type="submit">保存</button></div></form>
    <section className="rdp-detail"><h3>Windows 远程桌面</h3><p>已开启远程桌面的设备可直接连接，无需配对任务执行端。</p><RemoteDesktopButton state={state} device={device} primary/>{state.desktopUnavailable(device)&&<small>{state.desktopUnavailable(device)}</small>}</section>
    <div className="permission-note"><Icon name="shield"/><div><strong>配对后运行远程任务</strong><p>目标设备开启执行端后，可以配对并提交命令或脚本。</p><button className="text-button" disabled={!isDesktop||state.cached||!device.visible} onClick={onRemote}>前往远程任务 <Icon name="arrow" size={16}/></button></div></div>
  </aside>;
}
export function Devices({state,onAdd,onRemote}:{state:TailTaskState;onAdd:()=>void;onRemote?:()=>void}) {
  const [search,setSearch]=useState(''),[filter,setFilter]=useState('all');
  const items=filterDevices(state.devices,search,filter,state.cached);
  const liveCount=state.cached?0:state.devices.filter(d=>d.visible&&d.online===true).length;
  return <>
    <div className="network-strip"><span className="network-icon"><Icon name="network"/></span><div><strong>{state.snapshot?.network||'Tailscale 网络'}</strong><span>{state.cached?'历史观测':state.snapshot?networkLabels[state.snapshot.state]||'未知状态':isDesktop?'正在读取网络状态…':'桌面客户端连接后可用'}</span></div><span className="observed-time">{state.snapshot?'观测于 '+formatTime(state.snapshot.observed_at):'等待首次观测'}</span></div>
    <div className={'device-grid '+(state.selected?'has-selection':'')}>
      <section className="panel device-list" aria-label="设备目录"><div className="panel-title"><h2>设备列表 <span className="count">{state.devices.length}</span></h2><span className="muted">{liveCount} 台网络在线</span></div>
        <div className="device-toolbar"><label className="search-field"><Icon name="search" size={18}/><input aria-label="搜索设备" placeholder="搜索名称、别名或 IP 地址…" value={search} onChange={e=>setSearch(e.target.value)}/>{search&&<button className="icon-button" aria-label="清除搜索" onClick={()=>setSearch('')}><Icon name="close" size={16}/></button>}</label><select aria-label="筛选设备" value={filter} onChange={e=>setFilter(e.target.value)}><option value="all">全部设备</option><option value="online">网络在线</option><option value="offline">离线或未知</option><option value="favorites">已收藏</option></select></div>
        {state.cached&&<div className="inline-note"><Icon name="info" size={16}/>刷新未成功，以下保留上次观测，不代表实时在线。</div>}
        {items.length>0?<div className="table-wrap"><table className="device-table"><thead><tr><th>设备名称</th><th>系统</th><th>网络状态</th><th className="address-column">地址</th><th className="rdp-column">连接</th><th><span className="sr-only">收藏</span></th></tr></thead><tbody>{items.map(device=><tr key={device.node_id} className={state.selectedId===device.node_id?'selected':''}><td><button className="device-select" aria-pressed={state.selectedId===device.node_id} onClick={()=>state.setSelectedId(device.node_id)}><OsBadge os={device.os}/><span className="device-name" title={deviceName(device)}><strong>{deviceName(device)}</strong><small>{device.is_self?'当前设备':device.dns_name||device.node_id}</small></span></button></td><td className="os-text">{device.os||'未知'}</td><td><DeviceStatus device={device} cached={state.cached}/></td><td className="address-column"><code>{device.addresses[0]||'—'}</code></td><td className="rdp-column"><RemoteDesktopButton state={state} device={device}/></td><td><button className={'icon-button favorite '+(device.favorite?'is-favorite':'')} aria-label={(device.favorite?'取消收藏 ':'收藏 ')+deviceName(device)} aria-pressed={device.favorite} disabled={state.cached||!device.visible} onClick={()=>void state.updateDevice(device,device.alias,!device.favorite)}><Icon name="star" size={18}/></button></td></tr>)}</tbody></table></div>:<EmptyState title={state.loading?'正在读取设备…':state.devices.length?'没有匹配的设备':'连接你的第一台设备'}>{state.devices.length?'试试其他名称或调整筛选条件。':'在设备上安装并登录 Tailscale，即可在这里查看网络中可见的设备。'}<br/>{!state.devices.length&&<button className="text-button" onClick={onAdd}>查看添加步骤 <Icon name="arrow" size={16}/></button>}</EmptyState>}
        <div className="panel-foot"><Icon name="shield" size={15}/><span>设备可见性由你的 Tailscale 网络策略决定</span><span className="push-right">{items.length} 项</span></div>
      </section>
      {state.selected?<DeviceDetail key={state.selected.node_id} state={state} onRemote={onRemote}/>:<aside className="panel selection-placeholder"><NetworkArt/><div><span className="eyebrow">你的设备，一目了然</span><h2>选择设备，查看详情</h2><p>查看地址、连接状态与本地别名。<br/>点击“远程桌面”直接连接 Windows 设备。</p></div></aside>}
    </div>
  </>;
}
export function AddDeviceModal({state,onClose}:{state:TailTaskState;onClose:()=>void}) {
  return <Modal title="添加设备" onClose={onClose}><p className="modal-intro">让新设备加入同一个 Tailscale 网络，然后回来重新发现。</p><ol className="steps"><li><span>1</span><div><h3>安装官方 Tailscale</h3><p>在要加入的 Windows、macOS 或 Linux 设备上安装客户端。</p><code className="copyable-url">https://tailscale.com/download</code></div></li><li><span>2</span><div><h3>登录预期网络</h3><p>在新设备完成登录。如果网络需要设备审批，请由管理员批准后再继续。</p></div></li><li><span>3</span><div><h3>重新发现设备</h3><p>新设备可见后会出现在目录中；如果没有出现，请检查登录、审批和网络策略。</p></div></li></ol><div className="inline-note"><Icon name="info"/>设备加入后会自动读取。连接 Windows 桌面无需任务配对；运行命令和脚本需另外配对执行端。</div><div className="modal-actions"><button onClick={onClose}>完成</button><button className="primary" disabled={!isDesktop||state.loading} onClick={()=>void state.refresh()}><Icon name="refresh" className={state.loading?'spin':''}/>{state.loading?'发现中…':'重新发现'}</button></div>{state.snapshot&&<p className="muted">最近观测：{formatTime(state.snapshot.observed_at)} · {state.devices.filter(d=>d.visible).length} 台设备</p>}</Modal>;
}
