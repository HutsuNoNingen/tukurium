import {spawn} from 'node:child_process';
import {createServer,createConnection} from 'node:net';
import {mkdirSync,readFileSync,writeFileSync} from 'node:fs';
import {dirname,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {setTimeout as delay} from 'node:timers/promises';
import {randomUUID} from 'node:crypto';
import assert from 'node:assert/strict';

const root=resolve(dirname(fileURLToPath(import.meta.url)),'..'),children=[];
const session=`localization-${randomUUID()}`,work=resolve(root,'.run',session);
const preference=resolve(work,'language.cfg'),screenshot=resolve(work,'korean-settings.png');
mkdirSync(work,{recursive:true});
async function freePort(){const server=createServer();await new Promise(r=>server.listen(0,'127.0.0.1',r));const port=server.address().port;await new Promise(r=>server.close(r));return port;}
function start(file,args){const child=spawn(resolve(root,file),args,{cwd:root,windowsHide:true,stdio:['ignore','pipe','pipe']});child.logs='';child.stdout.on('data',d=>child.logs+=d);child.stderr.on('data',d=>child.logs+=d);children.push(child);return child;}
async function smoke(port,phase){
  const rendered=process.argv.includes('--rendered');
  const child=start('.tools/godot/Godot_v4.7.2-stable_win64_console.exe',[
    ...(rendered?['--rendering-method','gl_compatibility']:['--headless']),
    '--path',process.env.KOKOVERSE_TEST_CLIENT||'client-godot','--script','res://localization_smoke.gd','--log-file',resolve(work,`${phase}.log`),'--',
    `--port=${port}`,`--session=${session}`,`--language-settings=${preference}`,`--locale-phase=${phase}`,
    `--locale-input-helper=${resolve(root,'tests/localization-popup-input.ps1')}`,
    ...(rendered&&phase==='select'?[`--screenshot=${screenshot}`]:[])
  ]);
  const deadline=Date.now()+90000;
  while(child.exitCode===null&&Date.now()<deadline)await delay(20);
  assert.equal(child.exitCode,0,child.logs||`Localization ${phase} timed out`);
  assert.doesNotMatch(child.logs,/SCRIPT ERROR:|\bERROR:/);
  assert.match(child.logs,new RegExp(`LOCALIZATION SMOKE PASS: ${phase}`));
}

try{
  const port=await freePort(),transport=await freePort();
  const engine=start('target/release/kokoverse-engine.exe',['--role','host','--port',String(port),'--transport-port',String(transport),'--session',session]);
  let ready=false;
  for(let i=0;i<400;i++){
    const socket=createConnection({host:'127.0.0.1',port});
    ready=await new Promise(r=>{socket.once('connect',()=>r(true));socket.once('error',()=>r(false));});
    socket.destroy();if(ready)break;
    assert.equal(engine.exitCode,null,engine.logs);
    await delay(50);
  }
  assert.ok(ready,'Engine did not listen');
  await smoke(port,'select');
  assert.match(readFileSync(preference,'utf8'),/language\s*=\s*"ko"/,'Korean preference was not saved');
  await smoke(port,'restart');
  assert.match(readFileSync(preference,'utf8'),/language\s*=\s*"ja"/,'Japanese preference was not saved');
  writeFileSync(preference,'[interface]\nlanguage="unsupported"\n');
  await smoke(port,'fallback');
  console.log('PASS: language selector input, immediate Korean UI, authored text unchanged, preference across restart, Japanese return and unsupported preference fallback');
  if(process.argv.includes('--rendered'))console.log(`SCREENSHOT=${screenshot}`);
}catch(error){
  for(const child of children)if(child.logs)console.error(child.logs.slice(-5500));
  throw error;
}finally{
  for(const child of children)if(child.exitCode===null)child.kill();
}
