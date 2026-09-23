import { mkdir,readdir,copyFile,readFile,writeFile } from 'node:fs/promises';
import { join,basename } from 'node:path';
import { createHash } from 'node:crypto';
const platform=process.argv[2];
if(!['windows-x64','macos-arm64','macos-x64','linux-x64'].includes(platform)) throw new Error('Expected a known platform name');
async function walk(path){const out=[];for(const entry of await readdir(path,{withFileTypes:true})){const name=join(path,entry.name);if(entry.isDirectory())out.push(...await walk(name));else out.push(name);}return out;}
const candidates=(await walk('target/release/bundle')).filter(path=>/(\.exe|\.dmg|\.deb|\.AppImage)$/.test(path));
if(!candidates.length)throw new Error('No installer was produced');
await mkdir('release',{recursive:true});
const manifest={version:'0.1.0',platform,signature:'unsigned-test-build',validation:'Build only. Manual installation and UI validation are tracked separately.',files:[]};
for(const source of candidates){const name=basename(source);await copyFile(source,join('release',name));const data=await readFile(source);manifest.files.push({name,size:data.length,sha256:createHash('sha256').update(data).digest('hex')});}
await writeFile(join('release',platform+'-manifest.json'),JSON.stringify(manifest,null,2)+'\n');
await writeFile(join('release',platform+'-SHA256SUMS.txt'),manifest.files.map(f=>f.sha256+'  '+f.name).join('\n')+'\n');
console.log(JSON.stringify(manifest,null,2));
