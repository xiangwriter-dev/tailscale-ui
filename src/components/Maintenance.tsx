import { isDesktop } from '../api';
import type { TailTaskState } from '../useTailTask';
import { Icon } from './Icon';
import { Status } from './Common';

export function Maintenance({state}:{state:TailTaskState}) {
  return <><div className="repair-intro panel"><div className="intro-symbol"><Icon name="shield" size={34}/></div><div><h2>保持应用数据健康</h2><p>检查或修复这台电脑上的 xiangwriter远程器 数据。每次执行都有记录，结果可随时回看。</p></div><Status tone="info">仅作用于本机</Status></div><div className="repair-grid">{[
    {action:'check_database' as const,title:'数据库检查',tag:'只读诊断',body:'检查本机应用数据库的完整性，帮助识别数据异常。检查本身不会修改业务记录。',button:'检查数据库',icon:'database' as const},
    {action:'rebuild_indexes' as const,title:'重建应用索引',tag:'限定修复',body:'重建本产品的数据库索引。设备偏好、任务记录和事件都会保留。',button:'重建索引',icon:'repair' as const},
  ].map(item=><section key={item.action} className="panel repair-card"><div className="repair-card-top"><span className="repair-icon"><Icon name={item.icon} size={28}/></span><span className="small-tag">{item.tag}</span></div><h2>{item.title}</h2><p>{item.body}</p><div className="repair-card-foot"><span><Icon name="devices" size={15}/>本机应用</span><button className={item.action==='check_database'?'primary':''} disabled={!isDesktop||state.repairBusy||(state.pendingAction!==null&&state.pendingAction!==item.action)} onClick={()=>void state.repair(item.action)}>{state.repairBusy?'提交中…':state.pendingAction===item.action?'重试提交':item.button}<Icon name="arrow" size={16}/></button></div></section>)}</div><div className="subtle-note"><Icon name="info" size={18}/><p>只有你主动发起时才会执行。选择其他设备不会改变这里的执行目标。中断的任务不会自动重试。</p></div></>;
}
export function Settings({state}:{state:TailTaskState}) {
  return <div className="settings-layout"><section className="panel settings-section"><div className="panel-title"><h2>常规设置</h2></div><div className="setting-row"><div><h3>设备刷新间隔</h3><p>自动读取可见设备状态；窗口不可见时暂停定时刷新。</p></div><select aria-label="设备刷新间隔" value={state.intervalSeconds} disabled={!isDesktop||state.savingSetting} onChange={e=>void state.saveInterval(e.target.value)}><option value="10">10 秒</option><option value="30">30 秒</option><option value="60">60 秒</option></select></div></section><section className="panel settings-section"><div className="panel-title"><h2>应用信息</h2><span className="small-tag">基础测试版</span></div><dl className="settings-fields"><div><dt>当前版本</dt><dd>{state.info?.version||'0.1.0'}</dd></div><div><dt>修复权限</dt><dd>本机应用检查与限定修复</dd></div><div><dt>远程修复</dt><dd>尚未开放</dd></div><div><dt>数据保存位置</dt><dd><code>{state.info?.data_dir||'桌面客户端启动后显示'}</code></dd></div><div><dt>界面风格</dt><dd>简约 · 科技元素</dd></div></dl><div className="panel-foot"><Icon name="database" size={16}/>设备偏好与任务记录保存在本机 SQLite 数据库。</div></section></div>;
}
