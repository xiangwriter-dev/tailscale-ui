import { useCallback, useEffect, useRef, useState } from 'react';
import { api, isDesktop } from './api';
import type { AppInfo, Device, NetworkSnapshot, RepairAction, Task, TaskDetail } from './types';

export function useTailTask() {
  const [snapshot,setSnapshot]=useState<NetworkSnapshot|null>(null);
  const [devices,setDevices]=useState<Device[]>([]);
  const [selectedId,setSelectedId]=useState<string|null>(null);
  const [cached,setCached]=useState(false);
  const [loading,setLoading]=useState(false);
  const [error,setError]=useState('');
  const [notice,setNotice]=useState('');
  const [info,setInfo]=useState<AppInfo|null>(null);
  const [intervalSeconds,setIntervalSeconds]=useState('30');
  const [savingSetting,setSavingSetting]=useState(false);
  const [tasks,setTasks]=useState<Task[]>([]);
  const [recent,setRecent]=useState<Task[]>([]);
  const [taskPage,setTaskPage]=useState(0);
  const [tasksLoading,setTasksLoading]=useState(false);
  const [detail,setDetail]=useState<TaskDetail|null>(null);
  const [detailId,setDetailId]=useState<string|null>(null);
  const [detailError,setDetailError]=useState('');
  const [repairBusy,setRepairBusy]=useState(false);
  const [pendingAction,setPendingAction]=useState<RepairAction|null>(null);
  const refreshBusy=useRef(false), submitBusy=useRef(false), settingsBusy=useRef(false);
  const context=useRef('');
  const listSequence=useRef(0), detailSequence=useRef(0);
  const pendingRequests=useRef<Partial<Record<RepairAction,string>>>({});
  const mounted=useRef(true);
  const report=useCallback((value:unknown)=>setError(String(value).slice(0,1200)),[]);
  const refresh=useCallback(async()=>{
    if(!isDesktop||refreshBusy.current) return;
    refreshBusy.current=true;setLoading(true);
    try {
      const next=await api.refresh();
      if(!mounted.current) return;
      if(context.current!==next.context_id) setSelectedId(null);
      setSnapshot(next);
      if(next.state==='ready'&&next.context_id) {
        context.current=next.context_id;setDevices(next.devices);setCached(false);
        setSelectedId(id=>next.devices.some(device=>device.node_id===id&&device.visible)?id:null);
      } else {
        context.current='';setDevices([]);setCached(false);setSelectedId(null);
      }
      if(next.error) report(next.error);
    } catch(e) {
      if(mounted.current) {setCached(true);setSelectedId(null);report(e);}
    } finally { refreshBusy.current=false;if(mounted.current) setLoading(false); }
  },[report]);
  const loadTasks=useCallback(async()=>{
    if(!isDesktop) return;
    const seq=++listSequence.current;setTasksLoading(true);
    try {
      const [page,first]=await Promise.all([api.tasks(taskPage*50),taskPage===0?Promise.resolve(null):api.tasks(0)]);
      if(mounted.current&&seq===listSequence.current) {setTasks(page);setRecent((first||page).slice(0,4));}
    } catch(e) {if(seq===listSequence.current) report(e);}
    finally {if(mounted.current&&seq===listSequence.current) setTasksLoading(false);}
  },[taskPage,report]);
  const readDetail=useCallback(async(id:string)=>{
    const seq=++detailSequence.current;setDetailError('');
    try {
      const result=await api.detail(id);
      if(mounted.current&&seq===detailSequence.current) setDetail(result);
    } catch(e) {if(mounted.current&&seq===detailSequence.current) setDetailError(String(e));}
  },[]);
  const openTask=useCallback((id:string)=>{
    setDetail(null);setDetailId(id);void readDetail(id);
  },[readDetail]);
  const closeTask=useCallback(()=>{++detailSequence.current;setDetailId(null);setDetail(null);setDetailError('');},[]);
  useEffect(()=>{
    mounted.current=true;
    if(!isDesktop) return ()=>{mounted.current=false;};
    void api.info().then(value=>{if(mounted.current)setInfo(value);}).catch(report);
    void api.settings().then(value=>{if(mounted.current)setIntervalSeconds(['10','30','60'].includes(value.refresh_seconds)?value.refresh_seconds:'30');}).catch(report);
    void refresh();
    let dispose:(()=>void)|undefined;
    void api.onStorageError(report).then(unlisten=>{if(!mounted.current)unlisten();else dispose=unlisten;}).catch(report);
    return ()=>{mounted.current=false;dispose?.();};
  },[refresh,report]);
  useEffect(()=>{
    if(!isDesktop) return;
    const timer=window.setInterval(()=>{if(document.visibilityState==='visible')void refresh();},Number(intervalSeconds)*1000);
    return ()=>window.clearInterval(timer);
  },[intervalSeconds,refresh]);
  useEffect(()=>{
    if(!isDesktop) return;
    void loadTasks();
    const timer=window.setInterval(()=>{if(document.visibilityState==='visible')void loadTasks();},3000);
    return ()=>window.clearInterval(timer);
  },[loadTasks]);
  useEffect(()=>{
    if(!detailId||!detail||!['queued','running'].includes(detail.task.state)||detail.task.persistence_warning) return;
    const timer=window.setInterval(()=>{if(document.visibilityState==='visible')void readDetail(detailId);},1200);
    return ()=>window.clearInterval(timer);
  },[detailId,detail,readDetail]);
  async function updateDevice(device:Device,alias:string,favorite:boolean) {
    const current=context.current;
    if(!current||cached||!device.visible) return;
    try {
      await api.devices(current,device.node_id,alias,favorite);
      if(context.current===current) setDevices(list=>list.map(item=>item.node_id===device.node_id?{...item,alias,favorite}:item));
      setNotice('设备偏好已保存');
    } catch(e) {report(e);}
  }
  async function saveInterval(value:string) {
    if(settingsBusy.current||!isDesktop) return;
    settingsBusy.current=true;setSavingSetting(true);
    try {await api.saveSetting(value);setIntervalSeconds(value);setNotice('刷新间隔已保存');}
    catch(e) {report(e);}
    finally {settingsBusy.current=false;setSavingSetting(false);}
  }
  async function repair(action:RepairAction) {
    if(submitBusy.current||!isDesktop) return;
    submitBusy.current=true;setRepairBusy(true);setError('');
    const requestId=pendingRequests.current[action]||crypto.randomUUID();
    pendingRequests.current[action]=requestId;
    try {
      const task=await api.repair(action,requestId);
      // The submit is acknowledged before reading any details. A detail error is never a submit retry.
      delete pendingRequests.current[action];setPendingAction(null);
      setNotice('本机修复任务已接受');setDetailId(task.id);setDetail({task,events:[]});setDetailError('');
      void loadTasks();void readDetail(task.id);
    } catch(e) {
      setPendingAction(action);report(String(e)+'。再次点击同一操作将复用请求编号。');
    } finally {submitBusy.current=false;setRepairBusy(false);}
  }
  return {snapshot,devices,selectedId,setSelectedId,cached,loading,error,setError,notice,setNotice,info,intervalSeconds,savingSetting,
    tasks,recent,taskPage,setTaskPage,tasksLoading,detail,detailId,detailError,repairBusy,pendingAction,
    refresh,loadTasks,openTask,closeTask,readDetail,updateDevice,saveInterval,repair,
    selected:devices.find(device=>device.node_id===selectedId)||null};
}
export type TailTaskState=ReturnType<typeof useTailTask>;
