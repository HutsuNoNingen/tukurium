import {spawn} from 'node:child_process';
import {createConnection} from 'node:net';
import {mkdirSync,writeFileSync,cpSync,readFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {randomUUID,createHash} from 'node:crypto';
import {setTimeout as delay} from 'node:timers/promises';
import assert from 'node:assert/strict';
const root=resolve(import.meta.dirname,'..'),session=`avatar-${randomUUID()}`,base=49000+Math.floor(Math.random()*800);
const work=resolve(root,'.run',session),project=resolve(work,'client-godot');mkdirSync(work,{recursive:true});cpSync(resolve(root,'client-godot'),project,{recursive:true});
const relay=process.argv.find(a=>a.startsWith('--relay='))?.slice(8),children=[],sockets=[];
function start(file,args){const c=spawn(resolve(root,file),args,{cwd:root,windowsHide:true,stdio:['ignore','pipe','pipe']});c.logs='';c.stdout.on('data',d=>c.logs+=d);c.stderr.on('data',d=>c.logs+=d);children.push(c);return c;}
async function until(test,message,ms=30000){const end=Date.now()+ms;while(Date.now()<end){const r=test();if(r)return r;await delay(20);}throw Error(message);}
async function connect(port){for(let i=0;i<60;i++){const s=createConnection({host:'127.0.0.1',port});if(!await new Promise(r=>{s.once('connect',()=>r(true));s.once('error',()=>r(false));})){s.destroy();await delay(100);continue;}sockets.push(s);let buffer=Buffer.alloc(0);const messages=[];s.on('data',d=>{buffer=Buffer.concat([buffer,d]);while(buffer.length>=4&&buffer.length>=buffer.readUInt32BE()+4){const n=buffer.readUInt32BE();messages.push(JSON.parse(buffer.subarray(4,n+4)).body);buffer=buffer.subarray(n+4);}});return {messages,latest:()=>messages.filter(m=>m.kind==='snapshot').at(-1)};}throw Error('No engine');}
async function peer(index){const port=base+2+index*2;start('target/release/kokoverse-engine.exe',['--role','auto','--peer','auto','--name',`Name ${index}`,'--port',String(port),'--transport-port',String(port+1),'--session',session,'--asset-dir',resolve(work,`cache-${index}`)]);const client=await connect(port);start('.run/build/kokosync-adapter.exe',['--peer','auto','--engine',`127.0.0.1:${port+1}`,'--address',relay||`127.0.0.1:${base+1}`,'--session',session,...(relay?[]:['--insecure-development'])]);await until(()=>client.latest()?.editable,'Auto role did not synchronize');return {port,client};}
function viewer(port,side){return start('.tools/godot/Godot_v4.7.2-stable_win64_console.exe',['--rendering-method','gl_compatibility','--path',project,'--script','res://avatar_smoke.gd','--',`--port=${port}`,`--session=${session}`,`--side=${side}`,`--signals=${work}`]);}
try {
  if(!relay)start('.tools/kokosync-relay.exe',['--http',`127.0.0.1:${base}`,'--quic',`127.0.0.1:${base+1}`]);
  const first=await peer(0),second=await peer(1);
  assert.equal(first.client.latest().role,'host');assert.equal(second.client.latest().role,'guest');
  const views=[viewer(first.port,'owner'),viewer(second.port,'observer')];
  for(const v of views)await until(()=>v.logs.includes('AVATAR READY'),'View not ready');
  writeFileSync(resolve(work,'change'),'go');
  for(const v of views)await until(()=>v.logs.includes('AVATAR MATCH'),'Avatar texture not rendered');
  assert.equal(first.client.latest().snapshot.world_revision,'0','Avatar change must not mutate the world');
  const late=await peer(2);views.push(viewer(late.port,'late'));
  await until(()=>views[2].logs.includes('AVATAR MATCH'),'Late join failed to get avatar');
  const ids=views.map(v=>v.logs.match(/AVATAR MATCH ([a-f0-9]{64})/)[1]);assert.equal(new Set(ids).size,1);
  const bytes=readFileSync(resolve(work,'cache-0',ids[0]+'.png'));assert.equal(createHash('sha256').update(bytes).digest('hex'),ids[0]);
  writeFileSync(resolve(work,'reset'),'go');
  for(const v of views){await until(()=>v.exitCode!==null,'Viewer did not finish');assert.equal(v.exitCode,0,v.logs);assert.doesNotMatch(v.logs,/SCRIPT ERROR:|\bERROR:/);assert.match(v.logs,/AVATAR SMOKE PASS/);}
  console.log('PASS: automatic host/guest roles; custom avatar PNG material matches on three renderers, including late join; reset replicates without world edits');
}catch(e){for(const c of children)if(c.logs)console.error(c.logs.slice(-3500));throw e;}
finally{for(const s of sockets)s.destroy();for(const c of children)if(c.exitCode===null)c.kill();}
