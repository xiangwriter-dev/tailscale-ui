// @vitest-environment jsdom
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {cleanup,render,screen,waitFor} from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type {TailTaskState} from './useTailTask';
const mocks=vi.hoisted(()=>({connections:vi.fn(),capabilities:vi.fn(),histories:vi.fn(),submit:vi.fn(),detail:vi.fn(),autostart:vi.fn(),setAutostart:vi.fn()}));
vi.mock('./api',()=>({isDesktop:true}));
vi.mock('./remoteApi',async original=>({...await original<typeof import('./remoteApi')>(),remoteApi:mocks}));
import {RemoteWorkspace} from './components/Remote';
import {Autostart} from './components/Autostart';
const connection={id:'connection',context_id:'network',node_id:'node',agent_id:'agent',address:'100.64.0.1',port:47321,fingerprint:'test-only',name:'test'};
const state={cached:false,selectedId:'node',snapshot:{state:'ready',context_id:'network'},devices:[{node_id:'node',name:'合成目标设备',alias:'',addresses:['100.64.0.1'],visible:true}]} as unknown as TailTaskState;
beforeEach(()=>{vi.resetAllMocks();mocks.connections.mockResolvedValue([connection]);mocks.capabilities.mockResolvedValue({identity:{agent_id:'agent',node_id:'node'},os:'windows',account:'合成账户',allowed_directories:['C:\\jobs'],interpreters:['powershell'],concurrency:1,accepting:true});mocks.histories.mockResolvedValue([]);Object.defineProperty(HTMLDialogElement.prototype,'showModal',{configurable:true,value:function(this:HTMLDialogElement){this.open=true;}});Object.defineProperty(HTMLDialogElement.prototype,'close',{configurable:true,value:function(this:HTMLDialogElement){this.open=false;}});});
afterEach(cleanup);

it('explains that system remote desktop needs no task pairing and returns to devices',async()=>{
  const user=userEvent.setup(),onDevices=vi.fn();
  render(<RemoteWorkspace state={state} onDevices={onDevices}/>);
  await screen.findByText('执行端已连接');
  await user.click(screen.getByRole('button',{name:'配对设备'}));
  expect(screen.getByText(/连接 Windows 远程桌面无需配对/)).toBeTruthy();
  await user.click(screen.getByRole('button',{name:'返回设备，打开远程桌面'}));
  expect(onDevices).toHaveBeenCalledTimes(1);
});

it('retains the original request after response loss and detail failure never submits a second task',async()=>{
  const user=userEvent.setup();let captured:unknown;
  mocks.submit.mockImplementation(async(_connection,request)=>{captured=request;return {id:'history',connection_id:'connection',target:connection,request,submission_state:'submission_unknown',remote_id:null,task:null,sync_error:'连接中断',created_at:'2026-01-01',synced_at:null};});
  mocks.detail.mockRejectedValue(new Error('详情暂时不可读'));
  render(<RemoteWorkspace state={state}/>);await screen.findByText('执行端已连接');
  await user.type(screen.getByLabelText('任务名称'),'合成任务');await user.type(screen.getByLabelText('可执行程序'),'test-helper');await user.click(screen.getByRole('button',{name:'在目标设备运行'}));await screen.findByText(/详情暂时不可读/);expect(mocks.submit).toHaveBeenCalledTimes(1);
  await user.click(screen.getByRole('button',{name:'关闭远程任务详情'}));await user.click(screen.getByRole('button',{name:'用原请求重试'}));await waitFor(()=>expect(mocks.submit).toHaveBeenCalledTimes(2));expect(mocks.submit.mock.calls[1][1]).toEqual(captured);
});

it('preserves the draft while a changed network disables submission',async()=>{
  const user=userEvent.setup();const view=render(<RemoteWorkspace state={state}/>);await screen.findByText('执行端已连接');await user.type(screen.getByLabelText('任务名称'),'保留的草稿');
  view.rerender(<RemoteWorkspace state={{...state,snapshot:{...state.snapshot!,context_id:'other-network'}}}/>);
  await screen.findByText('网络身份或目标可见性变化，草稿已保留。恢复并核对目标后才能提交。');expect((screen.getByLabelText('任务名称') as HTMLInputElement).value).toBe('保留的草稿');expect((screen.getByRole('button',{name:'在目标设备运行'}) as HTMLButtonElement).disabled).toBe(true);expect(mocks.submit).not.toHaveBeenCalled();
});

it('does not show login startup enabled when system registration fails',async()=>{
  const user=userEvent.setup();mocks.autostart.mockResolvedValue(false);mocks.setAutostart.mockRejectedValue(new Error('系统拒绝注册'));render(<Autostart/>);await user.click(screen.getByRole('checkbox'));await screen.findByRole('alert');expect((screen.getByRole('checkbox') as HTMLInputElement).checked).toBe(false);
});
