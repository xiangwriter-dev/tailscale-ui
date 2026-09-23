import { mkdir,readdir,copyFile,readFile,writeFile } from 'node:fs/promises';
import { join,basename } from 'node:path';
import { createHash } from 'node:crypto';
const platform=process.argv[2];
if(!['windows-x64','macos-arm64','macos-x64','linux-x64'].includes(platform)) throw new Error('Expected a known platform name');
async function walk(path){const out=[];for(const entry of await readdir(path,{withFileTypes:true})){const name=join(path,entry.name);if(entry.isDirectory())out.push(...await walk(name));else out.push(name);}return out;}
const {version}=JSON.parse(await readFile('package.json','utf8'));
const candidates=(await walk('target/release/bundle')).filter(path=>/(\.exe|\.dmg|\.deb|\.AppImage)$/.test(path)&&basename(path).includes('_'+version+'_'));
if(!candidates.length)throw new Error('No installer was produced');
const output=join('release','v'+version);
await mkdir(output,{recursive:true});
const manifest={version,platform,signature:'unsigned-test-build',sourceCommit:process.env.GITHUB_SHA,sourceRun:process.env.GITHUB_RUN_ID?`https://github.com/${process.env.GITHUB_REPOSITORY}/actions/runs/${process.env.GITHUB_RUN_ID}`:undefined,validation:'Build artifact; see docs/validation.md for native installation and execution evidence.',files:[]};
for(const source of candidates){const sourceName=basename(source),name=sourceName.replace(/^xiangwriter远程器_/, 'xiangwriter-remote_');await copyFile(source,join(output,name));const data=await readFile(source);manifest.files.push({name,sourceName,size:data.length,sha256:createHash('sha256').update(data).digest('hex')});}
await writeFile(join(output,platform+'-manifest.json'),JSON.stringify(manifest,null,2)+'\n');
await writeFile(join(output,platform+'-SHA256SUMS.txt'),manifest.files.map(f=>f.sha256+'  '+f.name).join('\n')+'\n');
console.log(JSON.stringify(manifest,null,2));
