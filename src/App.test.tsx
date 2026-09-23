// @vitest-environment jsdom
import { afterEach,beforeEach,describe,expect,it,vi } from 'vitest';
import { act,cleanup,render,renderHook,screen,waitFor,within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { Device,NetworkSnapshot,Task } from './types';
const mocks=vi.hoisted(()=>({isDesktop:true,api:{info:vi.fn(),settings:vi.fn(),refresh:vi.fn(),tasks:vi.fn(),detail:vi.fn(),repair:vi.fn(),devices:vi.fn(),saveSetting:vi.fn(),onStorageError:vi.fn()}}));
vi.mock('./api',()=>mocks);
import App from './App';
import { useTailTask } from './useTailTask';

const device:Device={node_id:'test-node-1',name:'用于界面测试的中文设备名称',dns_name:'example.test',os:'windows',addresses:['100.64.0.10'],online:true,is_self:true,last_seen:null,alias:'',favorite:false,observed_at:'2026-01-01T00:00:00Z',visible:true};
const snapshot=(context_id='network-a'):NetworkSnapshot=>({state:'ready',version:'test',network:'测试网络',context_id,observed_at:device.observed_at,devices:[{...device}],error:null});
const task:Task={id:'test-task-1',request_id:'test-request',action:'check_database',scope:'local_application',label:'数据库检查',state:'queued',created_at:device.observed_at,started_at:null,finished_at:null,result:null,persistence_warning:null};
beforeEach(()=>{
  vi.resetAllMocks();mocks.isDesktop=true;
  mocks.api.info.mockResolvedValue({version:'0.1.0',data_dir:'test-only',agent_policy:'repair_only',remote_enabled:false,ui_design:'minimal_tech'});
  mocks.api.settings.mockResolvedValue({refresh_seconds:'30'});
  mocks.api.refresh.mockResolvedValue(snapshot());
  mocks.api.tasks.mockResolvedValue([]);
  mocks.api.devices.mockResolvedValue(undefined);
  mocks.api.saveSetting.mockResolvedValue(undefined);
  mocks.api.onStorageError.mockResolvedValue(()=>{});
  mocks.api.detail.mockResolvedValue({task:{...task,state:'succeeded',result:'检查正常'},events:[]});
  Object.defineProperty(HTMLDialogElement.prototype,'showModal',{configurable:true,value:function(this:HTMLDialogElement){this.open=true;}});
  Object.defineProperty(HTMLDialogElement.prototype,'close',{configurable:true,value:function(this:HTMLDialogElement){this.open=false;}});
});
afterEach(()=>{cleanup();vi.useRealTimers();});

describe('desktop interactions with a mocked IPC boundary',()=>{
  it('keeps effective setting after storage failure, saves only on success',async()=>{
    const user=userEvent.setup();render(<App/>);
    await user.click(screen.getByRole('button',{name:'设置'}));
    const select=screen.getByRole('combobox',{name:'设备刷新间隔'});
    mocks.api.saveSetting.mockRejectedValueOnce(new Error('storage unavailable'));
    await user.selectOptions(select,'10');
    await waitFor(()=>expect((select as HTMLSelectElement).value).toBe('30'));
    expect(screen.getByRole('alert').textContent).toContain('storage unavailable');
    expect(screen.queryByText('刷新间隔已保存')).toBeNull();
    await user.selectOptions(select,'60');
    await waitFor(()=>expect((select as HTMLSelectElement).value).toBe('60'));
    expect(screen.getByText('刷新间隔已保存')).toBeTruthy();
  });
  it('reuses a lost submission ID, and detail retry never submits again',async()=>{
    const user=userEvent.setup();render(<App/>);
    await user.click(screen.getByRole('button',{name:'本机修复'}));
    mocks.api.repair.mockRejectedValueOnce(new Error('response lost')).mockResolvedValueOnce(task);
    mocks.api.detail.mockRejectedValue(new Error('detail unavailable'));
    await user.click(screen.getByRole('button',{name:'检查数据库'}));
    await screen.findByRole('button',{name:'重试提交'});
    const request=mocks.api.repair.mock.calls[0][1];
    await user.click(screen.getByRole('button',{name:'重试提交'}));
    await screen.findByText('任务详情暂时无法读取');
    expect(mocks.api.repair.mock.calls[1][1]).toBe(request);
    mocks.api.detail.mockResolvedValue({task:{...task,state:'succeeded',result:'检查正常'},events:[]});
    await user.click(screen.getByRole('button',{name:'重试读取'}));
    await screen.findByText('检查正常');
    expect(mocks.api.repair).toHaveBeenCalledTimes(2);
    await user.click(screen.getByRole('button',{name:'关闭'}));
    mocks.api.repair.mockResolvedValue(task);
    await user.click(screen.getByRole('button',{name:'检查数据库'}));
    expect(mocks.api.repair.mock.calls[2][1]).not.toBe(request);
  });
  it('clears account selection and refuses to show old devices as live after an error',async()=>{
    const {result}=renderHook(()=>useTailTask());
    await waitFor(()=>expect(result.current.devices.length).toBe(1));
    act(()=>result.current.setSelectedId(device.node_id));
    mocks.api.refresh.mockResolvedValueOnce(snapshot('network-b'));
    await act(async()=>{await result.current.refresh();});
    expect(result.current.selectedId).toBeNull();
    act(()=>result.current.setSelectedId(device.node_id));
    mocks.api.refresh.mockRejectedValueOnce(new Error('timeout'));
    await act(async()=>{await result.current.refresh();});
    expect(result.current.cached).toBe(true);
    expect(result.current.selected).toBeNull();
    await act(async()=>{await result.current.updateDevice(device,'denied',true);});
    expect(mocks.api.devices).not.toHaveBeenCalled();
    mocks.api.refresh.mockResolvedValueOnce({...snapshot(),state:'login_required',context_id:'',devices:[]});
    await act(async()=>{await result.current.refresh();});
    expect(result.current.devices).toEqual([]);
  });
  it('selects same-name devices independently and restores dialog focus',async()=>{
    const user=userEvent.setup();
    mocks.api.refresh.mockResolvedValue({...snapshot(),devices:[device,{...device,node_id:'test-node-2',is_self:false,online:null,dns_name:'other.example.test'}]});
    render(<App/>);
    const selects=await screen.findAllByRole('button',{name:new RegExp(device.name)});
    const first=selects.find(item=>item.getAttribute('aria-pressed')==='false'&&!item.getAttribute('aria-label'))!;
    first.focus();await user.keyboard('{Enter}');
    const detail=screen.getByRole('complementary',{name:'所选设备详情'});
    expect(within(detail).getByText('test-node-1')).toBeTruthy();
    await user.click(screen.getByRole('button',{name:'添加设备'}));
    expect(screen.getByRole('dialog',{name:'添加设备'})).toBeTruthy();
    await user.click(screen.getByRole('button',{name:'关闭添加设备'}));
    expect(document.activeElement).toBe(screen.getByRole('button',{name:'添加设备'}));
  });
  it('browser preview contains no fabricated devices and disables local repair',async()=>{
    mocks.isDesktop=false;
    const user=userEvent.setup();render(<App/>);
    expect(screen.getByText(/界面预览 ·/)).toBeTruthy();
    expect(mocks.api.refresh).not.toHaveBeenCalled();
    expect(screen.queryByText(device.name)).toBeNull();
    await user.click(screen.getByRole('button',{name:'本机修复'}));
    expect((screen.getByRole('button',{name:'检查数据库'}) as HTMLButtonElement).disabled).toBe(true);
  });
});
