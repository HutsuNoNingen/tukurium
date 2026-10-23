import {spawn} from 'node:child_process';
import {createConnection} from 'node:net';
import {mkdirSync,readFileSync,writeFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {randomUUID} from 'node:crypto';
import {setTimeout as delay} from 'node:timers/promises';
import assert from 'node:assert/strict';

const root=resolve(import.meta.dirname,'..'),session=`ugc-${randomUUID()}`;
const base=52000+Math.floor(Math.random()*700),work=resolve(root,'.run',session),saved=resolve(work,'world.json');
const relay=process.argv.find(arg=>arg.startsWith('--relay='))?.slice(8);
mkdirSync(work,{recursive:true});const children=[],sockets=[];
function start(file,args){const child=spawn(resolve(root,file),args,{cwd:root,windowsHide:true,stdio:['ignore','pipe','pipe']});child.logs='';child.stdout.on('data',d=>child.logs+=d);child.stderr.on('data',d=>child.logs+=d);children.push(child);return child;}
async function until(test,message,ms=15000){const end=Date.now()+ms;while(Date.now()<end){const value=test();if(value)return value;await delay(20);}throw Error(message);}
async function connect(port){
  for(let attempt=0;attempt<100;attempt++){
    const socket=createConnection({host:'127.0.0.1',port});
    if(!await new Promise(r=>{socket.once('connect',()=>r(true));socket.once('error',()=>r(false));})){socket.destroy();await delay(50);continue;}
    sockets.push(socket);const messages=[];let buffer=Buffer.alloc(0),epoch='';
    socket.on('data',d=>{buffer=Buffer.concat([buffer,d]);while(buffer.length>=4&&buffer.length>=buffer.readUInt32BE()+4){const n=buffer.readUInt32BE(),e=JSON.parse(buffer.subarray(4,n+4));buffer=buffer.subarray(n+4);epoch=e.authority_epoch;messages.push(e.body);}});
    return {messages,latest:()=>messages.filter(m=>m.kind==='snapshot').at(-1),send(body){const data=Buffer.from(JSON.stringify({protocol_version:3,session_id:session,authority_epoch:epoch,sequence:'1',body}));const h=Buffer.alloc(4);h.writeUInt32BE(data.length);socket.write(Buffer.concat([h,data]));},wait(test){return until(()=>messages.find(test),'UGC response timed out');}};
  }throw Error('Engine did not start');
}
async function peer(role,index){const port=base+2+index*2;const child=start('target/release/kokoverse-engine.exe',['--role',role,'--session',session,'--port',String(port),'--transport-port',String(port+1),'--asset-dir',resolve(work,`assets-${index}`),...(role==='host'?['--save',saved]:[])]);const client=await connect(port);start('.run/build/kokosync-adapter.exe',['--session',session,'--peer',role==='host'?'host':`guest-${index}`,'--engine',`127.0.0.1:${port+1}`,'--address',relay||`127.0.0.1:${base+1}`,...(relay?[]:['--insecure-development'])]);await until(()=>client.latest()?.editable,'Peer did not sync');return {child,client,port};}
async function command(client,action,target){const command_id=randomUUID();client.send({kind:'script',command:{command_id,target, ...action}});const result=await client.wait(m=>m.kind==='result'&&m.command_id===command_id);return result;}
try {
  if(!relay)start('.tools/kokosync-relay.exe',['--http',`127.0.0.1:${base}`,'--quic',`127.0.0.1:${base+1}`]);
  const host=await peer('host',0),guest=await peer('guest',1);
  const probe=await command(guest.client,{action:'attach',source:host.client.latest().script_templates[0].source},host.client.latest().snapshot.objects[0].address);
  assert.equal(probe.ok,true,JSON.stringify(probe));
  await until(()=>host.client.latest().snapshot.objects[0].scripts?.length===1,'Probe script did not arrive');
  const probeScript=host.client.latest().snapshot.objects[0].scripts[0];
  const driveFor=(client,id)=>client.messages.filter(m=>m.kind==='drive_frame'&&m.frame.objects.some(o=>o.target.instance_id===id));
  const objectId=host.client.latest().snapshot.objects[0].address.instance_id;
  await until(()=>driveFor(host.client,objectId).length>2&&driveFor(guest.client,objectId).length>2,'Both Engines must evaluate the local Drive after calibrating their clocks');
  const baseRevision=host.client.latest().snapshot.world_revision;
  await delay(250);
  assert.equal(host.client.latest().snapshot.world_revision,baseRevision,'Local rotation must not produce shared world commits');
  for(const client of [host.client,guest.client]) {
    const frame=driveFor(client,objectId).at(-1).frame;
    const q=frame.objects.find(o=>o.target.instance_id===objectId).values.rotation;
    const seconds=probeScript.clock.elapsed+Math.max(0,frame.world_time-probeScript.clock.anchor);
    const half=(seconds*probeScript.parameters.speed%360)*Math.PI/360;
    assert.ok(Math.abs(q[1]*Math.sin(half)+q[3]*Math.cos(half))>1-1e-10,'Drive rotation must be derived from calibrated WorldTime');
  }
  const guestFrame=driveFor(guest.client,objectId).at(-1).frame;
  assert.ok(driveFor(host.client,objectId).some(m=>Math.abs(m.frame.world_time-guestFrame.world_time)<0.1),'Host and guest clocks must share a timeline');
  console.log('PASS: host and guest evaluate local Drives from a shared clock without changing the replicated world');

  assert.equal((await command(guest.client,{action:'remove',script_id:probeScript.id,expected_revision:probeScript.revision},host.client.latest().snapshot.objects[0].address)).ok,true);
  const rendered=process.argv.includes('--rendered');
  const godot=start('.tools/godot/Godot_v4.7.2-stable_win64_console.exe',[...(rendered?['--rendering-method','gl_compatibility']:['--headless']),'--path','client-godot','--script','res://script_smoke.gd','--',`--port=${guest.port}`,`--session=${session}`,...(rendered?[`--screenshot=${resolve(work,'script-editor.png')}`]:[])]);
  await until(()=>godot.exitCode!==null,'Godot UGC test did not finish',45000);
  assert.equal(godot.exitCode,0,godot.logs);assert.doesNotMatch(godot.logs,/SCRIPT ERROR:|\bERROR:/);assert.match(godot.logs,/SCRIPT EDITOR SMOKE PASS/);
  await until(()=>host.client.latest().snapshot.objects[0].scripts?.length===2,'Host did not receive script edits');
  let object=host.client.latest().snapshot.objects[0];
  assert.equal(object.scripts[0].parameters.speed,120);assert.equal(object.scripts[0].enabled,false);assert.equal(object.scripts[1].state.active,true);
  try {
    await until(()=>JSON.stringify(host.client.latest().snapshot)===JSON.stringify(guest.client.latest().snapshot),'Script results differ across peers');
  } catch(error) {
    writeFileSync(resolve(work,'host.json'),JSON.stringify(host.client.latest().snapshot,null,2));
    writeFileSync(resolve(work,'guest.json'),JSON.stringify(guest.client.latest().snapshot,null,2));
    console.error(`UGC snapshots: ${work}`);throw error;
  }
  console.log('PASS: Godot code editor attaches and edits Luau; public parameter auto-sync; rotation, pause and click state replicate; invalid code preserves working component');

  const failed=await command(guest.client,{action:'attach',source:'while true do end'},object.address);
  assert.equal(failed.ok,false);assert.match(failed.message,/budget/);
  const attach=await command(guest.client,{action:'attach',source:'return {name="Broken",update=function(ctx) ctx:set_color(ctx.self,{0,0,0,1}); while true do end end}'},object.address);
  assert.equal(attach.ok,true);
  await until(()=>guest.client.messages.some(m=>m.kind==='script_statuses'&&m.statuses.some(s=>s.error?.includes('budget'))),'Guest never received stopped-script diagnostics');
  object=host.client.latest().snapshot.objects[0];
  assert.deepEqual(object.fields.find(f=>f.id==='color').value,[1,0.3,0.1,1],'Failed callback must not partly change color');
  const broken=object.scripts.find(s=>s.schema.name==='Broken');
  assert.equal((await command(guest.client,{action:'remove',script_id:broken.id,expected_revision:broken.revision},object.address)).ok,true);

  object=host.client.latest().snapshot.objects[0];
  const copyId=randomUUID();guest.client.send({kind:'structure',command:{command_id:copyId,action:'duplicate',target:object.address,expected_revision:object.object_revision}});
  const copied=await guest.client.wait(m=>m.kind==='result'&&m.command_id===copyId);assert.equal(copied.ok,true);
  await until(()=>host.client.latest().snapshot.objects.length===2,'Copy not replicated');
  const copy=host.client.latest().snapshot.objects.find(o=>o.address.instance_id===copied.selected.instance_id);
  assert.notEqual(copy.scripts[0].id,object.scripts[0].id);
  await command(guest.client,{action:'interact'},copy.address);
  await until(()=>host.client.latest().snapshot.objects.find(o=>o.address.instance_id===copy.address.instance_id).scripts[1].state.active===false,'Copy state did not change');
  assert.equal(host.client.latest().snapshot.objects.find(o=>o.address.instance_id===object.address.instance_id).scripts[1].state.active,true);
  const late=await peer('guest',2);
  assert.deepEqual(late.client.latest().snapshot,host.client.latest().snapshot);
  const request_id=randomUUID();host.client.send({kind:'save_world',request_id});assert.equal((await host.client.wait(m=>m.kind==='save_result'&&m.request_id===request_id)).ok,true);
  const expected=host.client.latest().snapshot;
  assert.equal(JSON.parse(readFileSync(saved)).format_version,1);
  start('target/release/kokoverse-engine.exe',['--role','host','--session',session,'--port',String(base+8),'--transport-port',String(base+9),'--save',saved,'--asset-dir',resolve(work,'restored-assets')]);
  const restored=await connect(base+8);await until(()=>restored.latest(),'Restored snapshot missing');
  assert.deepEqual(restored.latest().snapshot,expected);
  console.log('PASS: execution limits stop only the bad script; diagnostics reach guests; copies have independent IDs/state; late join and disk restore preserve scripts');
  if(rendered)console.log(`SCREENSHOT=${resolve(work,'script-editor.png')}`);
}catch(error){for(const child of children)if(child.logs)console.error(child.logs.slice(-3500));throw error;}
finally{for(const socket of sockets)socket.destroy();for(const child of children)if(child.exitCode===null)child.kill();}
