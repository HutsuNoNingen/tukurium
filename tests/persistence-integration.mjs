import {spawn} from 'node:child_process';
import {createConnection} from 'node:net';
import {mkdirSync,writeFileSync,readFileSync,copyFileSync,rmSync} from 'node:fs';
import {resolve} from 'node:path';
import {randomUUID,createHash} from 'node:crypto';
import {setTimeout as delay} from 'node:timers/promises';
import assert from 'node:assert/strict';
import {modelFixture} from './asset-fixture.mjs';

const root=resolve(import.meta.dirname,'..'), session=`save-${randomUUID()}`;
const base=51000+Math.floor(Math.random()*1000), work=resolve(root,'.run',session);
const run=resolve(work,'.run'), world=resolve(run,'world.json'), assets=resolve(run,'assets/shared');
const children=[], sockets=[];
mkdirSync(run,{recursive:true}); mkdirSync(resolve(work,'scripts'));
copyFileSync(resolve(root,'scripts/stop-demo.ps1'),resolve(work,'scripts/stop.ps1'));
function start(file,args){
  const child=spawn(resolve(root,file),args,{cwd:root,windowsHide:true,stdio:['ignore','pipe','pipe']});
  child.logs=''; child.stdout.on('data',d=>child.logs+=d); child.stderr.on('data',d=>child.logs+=d);
  children.push(child); return child;
}
async function until(test,message,ms=10000){
  const end=Date.now()+ms;
  while(Date.now()<end){const value=test(); if(value)return value; await delay(20);}
  throw Error(message);
}
async function kill(child){if(child.exitCode===null){child.kill(); await until(()=>child.exitCode!==null||child.signalCode!==null,'Process did not stop');}}
async function connect(port){
  for(let i=0;i<80;i++){
    const socket=createConnection({host:'127.0.0.1',port});
    if(!await new Promise(r=>{socket.once('connect',()=>r(true));socket.once('error',()=>r(false));})){socket.destroy();await delay(50);continue;}
    sockets.push(socket); let buffer=Buffer.alloc(0),epoch=''; const messages=[];
    socket.on('data',data=>{
      buffer=Buffer.concat([buffer,data]);
      while(buffer.length>=4&&buffer.length>=buffer.readUInt32BE()+4){
        const n=buffer.readUInt32BE(),envelope=JSON.parse(buffer.subarray(4,n+4));
        buffer=buffer.subarray(n+4); epoch=envelope.authority_epoch; messages.push(envelope.body);
      }
    });
    return {messages,latest:()=>messages.filter(m=>m.kind==='snapshot').at(-1),
      send(body){const data=Buffer.from(JSON.stringify({protocol_version:2,session_id:session,authority_epoch:epoch,sequence:'1',body}));const h=Buffer.alloc(4);h.writeUInt32BE(data.length);socket.write(Buffer.concat([h,data]));},
      wait(predicate){return until(()=>messages.find(predicate),'Engine response timeout');}};
  }
  throw Error(`Engine not listening on ${port}`);
}
function engine(role,port,path=world,cache=assets){return start('target/release/kokoverse-engine.exe',['--role',role,'--session',session,'--port',String(port),'--transport-port',String(port+1),'--save',path,'--asset-dir',cache]);}
function adapter(port){return start('.run/build/kokosync-adapter.exe',['--peer','auto','--session',session,'--engine',`127.0.0.1:${port+1}`,'--address',`127.0.0.1:${base+1}`,'--insecure-development']);}
async function edit(client,field,value){
  const object=client.latest().snapshot.objects[0],command_id=randomUUID();
  client.send({kind:'edit',edit:{command_id,target:object.address,field_id:field,expected_revision:object.fields.find(f=>f.id===field).revision,value}});
  assert.equal((await client.wait(m=>m.kind==='result'&&m.command_id===command_id)).ok,true);
}
async function save(client){
  const request_id=randomUUID();client.send({kind:'save_world',request_id});
  return client.wait(m=>m.kind==='save_result'&&m.request_id===request_id);
}
function readWorld(){try{return JSON.parse(readFileSync(world));}catch{return null;}}
async function stopScript(child,port){
  // Match the launcher's ownership records, including exact process start time.
  const prepare=resolve(work,'records.ps1');
  writeFileSync(prepare,`param([int]$EngineId,[string]$EnginePath,[string]$Run,[int]$Port,[string]$Session)
$p=Get-Process -Id $EngineId
ConvertTo-Json -InputObject @(@{id=$EngineId;path=$EnginePath;startTicks=$p.StartTime.ToUniversalTime().Ticks.ToString()}) | Set-Content -LiteralPath (Join-Path $Run 'processes.json') -Encoding UTF8
@{ui_port=$Port;session=$Session;autosave=$true} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $Run 'connection.json') -Encoding UTF8
`, 'utf8');
  const windowsPowerShell=resolve(process.env.WINDIR,'System32/WindowsPowerShell/v1.0/powershell.exe');
  const prep=start(windowsPowerShell,['-NoProfile','-ExecutionPolicy','Bypass','-File',prepare,'-EngineId',String(child.pid),'-EnginePath',resolve(root,'target/release/kokoverse-engine.exe'),'-Run',run,'-Port',String(port),'-Session',session]);
  await until(()=>prep.exitCode!==null,'Preparing stop records timed out');assert.equal(prep.exitCode,0,prep.logs);
  const stop=start(windowsPowerShell,['-NoProfile','-ExecutionPolicy','Bypass','-File',resolve(work,'scripts/stop.ps1')]);
  await until(()=>stop.exitCode!==null,'Stop script timed out',18000);return stop;
}

try {
  const relayArgs=['--http',`127.0.0.1:${base}`,'--quic',`127.0.0.1:${base+1}`];
  const relay=start('.tools/kokosync-relay.exe',relayArgs);
  const first=engine('auto',base+2),host=await connect(base+2),hostAdapter=adapter(base+2);
  await until(()=>host.latest()?.editable,'Auto host not ready');assert.equal(host.latest().role,'host');
  const guestPath=resolve(run,'guest-world.json'), sentinel='Unrelated damaged checkpoint from an older hosting session';
  writeFileSync(guestPath,sentinel);
  const guestEngine=engine('auto',base+4,guestPath,resolve(run,'guest-assets'));
  const guest=await connect(base+4),guestAdapter=adapter(base+4);
  await until(()=>guest.latest()?.editable,'Auto guest not ready');assert.equal(guest.latest().role,'guest');

  const bytes=modelFixture(90000),id=createHash('sha256').update(bytes).digest('hex');
  host.send({kind:'asset_offer',asset_id:id,size:bytes.length});
  await host.wait(m=>m.kind==='asset_chunk_request'&&m.asset_id===id);
  host.send({kind:'asset_chunk',asset_id:id,offset:0,data:bytes.toString('base64')});
  await host.wait(m=>m.kind==='asset_ready'&&m.asset_id===id);
  const command_id=randomUUID();
  host.send({kind:'structure',command:{command_id,action:'import_asset',name:'Persistent model',position:[2,1,0],rotation:[0,0,0,1],visual:{kind:'model',asset_id:id,half_extents:[0.5,0.5,0.5]}}});
  assert.equal((await host.wait(m=>m.kind==='result'&&m.command_id===command_id)).ok,true);
  await edit(host,'position',[4,2,-3]);
  const expected=host.latest().snapshot;
  await until(()=>readWorld()?.world_revision===expected.world_revision,'Autosave did not persist edits');
  await delay(2200);
  assert.equal(readFileSync(guestPath,'utf8'),sentinel,'Guest must not replace its local host checkpoint');
  assert.equal((await save(guest)).saved,false);
  await kill(first);
  for(const child of [hostAdapter,guestEngine,guestAdapter,relay])await kill(child);
  start('.tools/kokosync-relay.exe',relayArgs);
  const restarted=engine('auto',base+6),restored=await connect(base+6);adapter(base+6);
  await until(()=>restored.latest()?.editable,'Restored host not ready');
  assert.deepEqual(restored.latest().snapshot,expected,'Crash/restart must preserve IDs, revisions, transforms and model references');
  restored.send({kind:'asset_get',asset_id:id});
  await restored.wait(m=>m.kind==='asset_offer'&&m.asset_id===id);
  restored.send({kind:'asset_chunk_request',asset_id:id,offset:0});
  const chunk=await restored.wait(m=>m.kind==='asset_chunk'&&m.asset_id===id);
  assert.deepEqual(Buffer.from(chunk.data,'base64'),bytes,'Stable asset cache survives a different IPC port');
  await edit(restored,'name','Saved at shutdown');
  const shutdownExpected=restored.latest().snapshot;
  const stopped=await stopScript(restarted,base+6);
  assert.equal(stopped.exitCode,0,stopped.logs);assert.match(stopped.logs,/World saved/);
  await until(()=>restarted.exitCode!==null||restarted.signalCode!==null,'Stop did not terminate engine');
  assert.deepEqual(readWorld(),shutdownExpected,'Stop.bat must flush latest revision before killing the engine');
  console.log('PASS: automatic host saves; guest preserves its checkpoint; crash restart restores objects and asset bytes; Stop.bat flushes latest edits');

  // Corrupt only the primary checkpoint; recovery must keep the good backup.
  const backup=JSON.parse(readFileSync(world+'.bak'));
  writeFileSync(world,'truncated');
  const recovered=engine('host',base+8),recovery=await connect(base+8);
  await until(()=>recovery.latest()?.editable,'Backup recovery did not start');
  assert.deepEqual(recovery.latest().snapshot,backup);
  assert.equal((await save(recovery)).ok,true);
  assert.deepEqual(JSON.parse(readFileSync(world+'.bak')),backup);
  rmSync(world+'.bak');mkdirSync(world+'.bak');
  await edit(recovery,'name','Cannot save yet');
  const failedStop=await stopScript(recovered,base+8);
  assert.notEqual(failedStop.exitCode,0);assert.match(failedStop.logs,/left running/);
  assert.equal(recovered.exitCode,null,'A save failure must keep the engine alive');
  rmSync(world+'.bak',{recursive:true});
  assert.equal((await save(recovery)).ok,true,'Save retries after storage error is fixed');
  console.log('PASS: corrupt primary recovers backup; failed shutdown save leaves engine running; retry succeeds');
} catch(error) {
  for(const child of children)if(child.logs)console.error(child.logs.slice(-2400));throw error;
} finally {
  for(const socket of sockets)socket.destroy();
  for(const child of children)await kill(child);
}
