import {spawn} from 'node:child_process';
import {createServer,createConnection} from 'node:net';
import {mkdirSync} from 'node:fs';
import {resolve} from 'node:path';
import {setTimeout as delay} from 'node:timers/promises';
import {randomUUID} from 'node:crypto';
import assert from 'node:assert/strict';
const root=resolve(import.meta.dirname,'..'),children=[];
const session=`hud-${randomUUID()}`,work=resolve(root,'.run',session);
mkdirSync(work,{recursive:true});
async function freePort(){const s=createServer();await new Promise(r=>s.listen(0,'127.0.0.1',r));const port=s.address().port;await new Promise(r=>s.close(r));return port;}
function start(file,args){const child=spawn(resolve(root,file),args,{cwd:root,windowsHide:true,stdio:['ignore','pipe','pipe']});child.logs='';child.stdout.on('data',d=>child.logs+=d);child.stderr.on('data',d=>child.logs+=d);children.push(child);return child;}
try {
  const port=await freePort(),transport=await freePort();
  start('target/release/kokoverse-engine.exe',['--role','host','--port',String(port),'--transport-port',String(transport),'--session',session]);
  for(let i=0;i<100;i++){const socket=createConnection({host:'127.0.0.1',port});const ready=await new Promise(r=>{socket.once('connect',()=>r(true));socket.once('error',()=>r(false));});socket.destroy();if(ready)break;await delay(50);}
  const screenshot=resolve(work,'hud.png');
  const godot=start('.tools/godot/Godot_v4.7.2-stable_win64_console.exe',[...(process.argv.includes('--rendered')?['--rendering-method','gl_compatibility']:['--headless']),'--path',process.env.KOKOVERSE_TEST_CLIENT||'client-godot','--script','res://hud_smoke.gd','--',`--port=${port}`,`--session=${session}`,...(process.argv.includes('--rendered')?[`--screenshot=${screenshot}`]:[])]);
  const deadline=Date.now()+45000;while(godot.exitCode===null&&Date.now()<deadline)await delay(20);
  assert.equal(godot.exitCode,0,godot.logs);assert.doesNotMatch(godot.logs,/SCRIPT ERROR:|\bERROR:/);assert.match(godot.logs,/HUD SMOKE PASS/);
  console.log('PASS: cursor-first FPS, click to capture, F5 cycle, Esc release, dashboard, stopped movement, live status and middle-click inspectors');
  console.log(`SCREENSHOT=${screenshot}`);
}catch(error){for(const child of children)if(child.logs)console.error(child.logs.slice(-4500));throw error;}
finally{for(const child of children)if(child.exitCode===null)child.kill();}
