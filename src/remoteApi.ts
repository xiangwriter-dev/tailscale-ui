import { call } from './api';

export type Execution = {mode:'exec';program:string;args:string[]}|{mode:'script';interpreter:string;source:string;powershell_policy?:'inherit'|'process_remote_signed'};
export interface RemoteRequest {request_id:string;name:string;target:{agent_id:string;node_id:string};cwd:string;execution:Execution;timeout_seconds:number;environment:Record<string,string>;result_directory:string|null}
export interface Connection {id:string;context_id:string;node_id:string;agent_id:string;address:string;port:number;fingerprint:string;name:string}
export interface Capabilities {identity:{agent_id:string;node_id:string};os:string;account:string;allowed_directories:string[];interpreters:string[];concurrency:number;accepting:boolean}
export interface RemoteTask {id:string;controller_id:string;request:RemoteRequest;state:string;created_at:string;started_at:string|null;finished_at:string|null;exit_code:number|null;progress:number|null;result_status:string;error:string|null}
export interface History {id:string;connection_id:string;target:Connection;request:RemoteRequest;submission_state:string;remote_id:string|null;task:RemoteTask|null;sync_error:string|null;created_at:string;synced_at:string|null}
export interface RemoteEvent {seq:number;task_id:string;kind:string;text:string;occurred_at:string}
export interface Artifact {id:string;name:string;task_id:string;size:number;sha256:string}
export interface RemoteDetail {history:History;events:RemoteEvent[];artifacts:Artifact[]}
export interface Controller {id:string;name:string;created_at:string;revoked_at:string|null}
export interface AgentState {state:'not_configured'|'running'|'unreachable';status:{capabilities:Capabilities;active_tasks:number;waiting_tasks:number;fault:string|null;controllers:Controller[]}|null;config:{allowed_directories:string[];concurrency:number;address:string;port:number}|null;error:string|null}
export const remoteApi = {
  connections:()=>call<Connection[]>('remote_connections'),
  pair:(contextId:string,nodeId:string,offer:string)=>call<Connection>('pair_remote',{contextId,nodeId,offer}),
  capabilities:(connectionId:string)=>call<Capabilities>('remote_capabilities',{connectionId}),
  submit:(connectionId:string,request:RemoteRequest)=>call<History>('submit_remote',{connectionId,request}),
  histories:(offset=0)=>call<History[]>('remote_histories',{offset}),
  detail:(id:string,after=0)=>call<RemoteDetail>('remote_detail',{id,after}),
  cancel:(id:string)=>call<History>('cancel_remote',{id}),
  download:(id:string,artifactId:string)=>call<string|null>('download_remote',{id,artifactId}),
  agent:()=>call<AgentState>('local_agent_status'),
  directory:()=>call<string|null>('choose_agent_directory'),
  start:(allowedDirectories:string[],concurrency:number)=>call<AgentState>('start_local_agent',{allowedDirectories,concurrency}),
  pairing:()=>call<string>('local_agent_pairing'),
  revoke:(id:string)=>call<void>('revoke_controller',{id}),
  stop:(abort:boolean)=>call<void>('stop_local_agent',{abort}),
  autostart:()=>call<boolean>('agent_autostart'),
  setAutostart:(enabled:boolean)=>call<boolean>('set_agent_autostart',{enabled}),
};
export const remoteLabels:Record<string,string>={queued:'排队中',starting:'正在启动',running:'运行中',cancelling:'取消中',succeeded:'已完成',failed:'执行失败',cancelled:'已取消',timed_out:'已超时',recovery_required:'结果待核实',prepared:'待提交',submission_unknown:'提交结果未知',accepted:'已接受',rejected:'已拒绝'};
export const isTerminal=(state:string)=>['succeeded','failed','cancelled','timed_out','recovery_required'].includes(state);
