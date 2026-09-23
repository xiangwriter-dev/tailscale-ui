import type { CSSProperties } from 'react';

export type IconName = 'devices' | 'tasks' | 'repair' | 'settings' | 'plus' | 'search' | 'refresh' | 'close' | 'chevron' | 'star' | 'database' | 'shield' | 'check' | 'info' | 'network' | 'arrow';
const paths: Record<IconName, string> = {
  devices:'M4 4h16v12H4z M9 20h6 M12 16v4',
  tasks:'M6 3h9l3 3v15H6z M9 10h6 M9 14h6 M9 18h4',
  repair:'M14.5 6.5a5 5 0 0 0-6 6L3 18l3 3 5.5-5.5a5 5 0 0 0 6-6L14 13l-3-3z',
  settings:'M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8 M9 3h6l1 3 3 1 2 5-2 5-3 1-1 3H9l-1-3-3-1-2-5 2-5 3-1z',
  plus:'M12 5v14 M5 12h14',search:'M16 16l5 5 M10 3a7 7 0 1 0 0 14 7 7 0 0 0 0-14',
  refresh:'M20 8a8 8 0 0 0-14-2L3 9 M3 4v5h5 M4 16a8 8 0 0 0 14 2l3-3 M21 20v-5h-5',
  close:'M6 6l12 12 M18 6L6 18',chevron:'M9 5l7 7-7 7',
  star:'m12 3 2.8 5.7 6.2.9-4.5 4.4 1.1 6.2-5.6-3-5.6 3 1.1-6.2L3 9.6l6.2-.9z',
  database:'M4 6c0-4 16-4 16 0s-16 4-16 0v12c0 4 16 4 16 0V6 M4 12c0 4 16 4 16 0',
  shield:'m12 3 8 3v6c0 5-8 9-8 9s-8-4-8-9V6z M8 12l3 3 5-6',
  check:'M5 12l4 4L19 6', info:'M12 11v6 M12 7h.01 M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20',
  network:'M12 5v6 M12 11l-6 7 M12 11l6 7 M12 2a3 3 0 1 0 0 6 3 3 0 0 0 0-6 M5 16a3 3 0 1 0 0 6 3 3 0 0 0 0-6 M19 16a3 3 0 1 0 0 6 3 3 0 0 0 0-6',
  arrow:'M4 12h16 M14 6l6 6-6 6',
};
export function Icon({name,size=20,className='',style}:{name:IconName;size?:number;className?:string;style?:CSSProperties}) {
  return <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.65" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" className={className} style={style}><path d={paths[name]}/></svg>;
}
export function NetworkArt() {
  return <svg viewBox="0 0 240 170" aria-hidden="true" className="network-art"><g fill="none" stroke="currentColor" strokeWidth="1"><path d="M20 130 75 90 125 132 205 83 144 28 75 90 205 83 M125 132 144 28 M20 130 125 132 230 153 205 83"/></g>{[[20,130,4],[75,90,8],[125,132,5],[205,83,7],[144,28,5],[230,153,4]].map(([x,y,r])=><circle key={x} cx={x} cy={y} r={r} fill="currentColor"/>)}</svg>;
}
