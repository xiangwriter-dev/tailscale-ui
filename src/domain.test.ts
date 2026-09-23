import { describe, expect, it } from 'vitest';
import { filterDevices, onlineLabel } from './domain';
import type { Device } from './types';

const base: Device = { node_id:'one',name:'工作站',dns_name:'host.example.ts.net',os:'windows',addresses:['100.64.0.1'],
  online:true,is_self:false,last_seen:null,alias:'',favorite:false,observed_at:'2026-09-23T00:00:00Z',visible:true };
describe('device observations', () => {
  it('never describes stale cache as online', () => { expect(onlineLabel(base,true)).toBe('历史观测'); });
  it('keeps missing online distinct from offline', () => { expect(onlineLabel({...base,online:null},false)).toBe('状态未知'); });
  it('filters by IP and retains distinct same-name nodes', () => {
    const other = {...base,node_id:'two',addresses:['100.64.0.2']};
    expect(filterDevices([base,other],'100.64.0.2','all').map(d=>d.node_id)).toEqual(['two']);
    expect(filterDevices([base,other],'工作站','all')).toHaveLength(2);
  });
  it('does not include no-longer-visible nodes in online results', () => {
    expect(filterDevices([{...base,visible:false}],'','online')).toHaveLength(0);
  });
});
