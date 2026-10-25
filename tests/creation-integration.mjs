import {spawn} from 'node:child_process';
import {createConnection} from 'node:net';
import {mkdirSync,readdirSync} from 'node:fs';
import {resolve} from 'node:path';
import {randomUUID} from 'node:crypto';
import {setTimeout as delay} from 'node:timers/promises';
import assert from 'node:assert/strict';

const root=resolve(import.meta.dirname,'..'),session=`creation-${randomUUID()}`;
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
try {
  start('.tools/kokosync-relay.exe',['--http',`127.0.0.1:${base}`,'--quic',`127.0.0.1:${base+1}`]);
  const host=await peer('host',0),guest=await peer('guest',1);
  const godot=start('.tools/godot/Godot_v4.7.2-stable_win64_console.exe',[...(process.argv.includes('--rendered')?['--rendering-method','gl_compatibility']:['--headless']),'--path','client-godot','--script','res://creation_smoke.gd','--',`--port=${guest.port}`,`--session=${session}`,`--photo-dir=${resolve(work,'photos')}`,...(process.argv.includes('--rendered')?[`--screenshot=${resolve(work,'creation.png')}`]:[])]);
  await until(()=>godot.exitCode!==null,'Creation UI timed out',60000);
  assert.equal(godot.exitCode,0,godot.logs);assert.doesNotMatch(godot.logs,/SCRIPT ERROR:|\bERROR:/);assert.match(godot.logs,/CREATION SMOKE PASS/);
  const shapes=['box','sphere','capsule','cylinder','cone','torus','grid','quad','camera','mirror'];
  await until(()=>shapes.every(shape=>host.client.latest().snapshot.objects.some(o=>o.visual.kind==='primitive'&&o.visual.shape===shape)),'Primitives did not reach host');
  await until(()=>host.client.latest().snapshot.objects.some(o=>o.visual.kind==='none'&&o.renderer===undefined&&o.collider===null),'Empty Slot did not reach host');
  const observed=host.client.messages.filter(m=>m.kind==='snapshot').flatMap(m=>m.snapshot.objects);
  const camera=host.client.latest().snapshot.objects.find(o=>o.visual.kind==='primitive'&&o.visual.shape==='camera');
  assert.ok(camera,'Camera item did not reach host');
  assert.ok(host.client.messages.some(m=>m.kind==='object_previews'&&m.previews?.some(entry=>entry.preview.target.instance_id===camera.address.instance_id&&entry.preview.rotation)),'Drone live pose did not reach another client');
  const slot=host.client.latest().snapshot.objects.find(o=>o.visual.kind==='none'&&o.renderer===undefined&&o.fields.some(f=>f.id==='name'&&f.value==='Slot'));
  assert.ok(slot,'Empty Slot was not retained');
  assert.ok(observed.some(o=>o.address.instance_id===slot.address.instance_id&&o.visual.kind==='primitive'&&o.renderer),'Renderer attachment did not reach host');
  assert.equal(slot.world_text?.text,'Text','WorldText did not reach host');
  assert.ok(observed.some(o=>o.address.instance_id===slot.address.instance_id&&o.world_text?.text==='Text'),'WorldText attachment was not observed');
  assert.ok(host.client.latest().snapshot.objects.some(o=>o.parent===null&&o.world_text?.text==='Text'&&o.address.instance_id!==slot.address.instance_id),'Standalone text did not reach host');
  assert.ok(host.client.latest().snapshot.objects.some(o=>o.parent?.instance_id===slot.address.instance_id&&o.world_text?.text==='Text'),'Text child did not reach host');
  const manipulated=observed.find(o=>o.creation && o.world_scale[0]>1.05 && Math.abs(o.world_rotation[1])>0.01);
  assert.ok(manipulated?.panel && manipulated.collider && !manipulated.inspector,'Grabbed creation surface did not reach host');
  assert.ok(shapes.filter(shape=>!['camera','mirror'].includes(shape)).every(shape=>observed.some(o=>o.creation?.shape===shape)),'Selection did not reach host');
  assert.ok(readdirSync(resolve(work,'photos')).some(name=>name.endsWith('.png')),'Camera photo was not saved locally');
  assert.ok(host.client.latest().snapshot.objects.some(o=>o.visual.kind==='image'&&o.fields.some(f=>f.id==='name'&&f.value.startsWith('Photo '))),'Camera photo item did not reach host');
  // Each successful creation consumes its source panel; the script opens one more to test Esc.
  await until(()=>host.client.latest().snapshot.objects.filter(o=>o.creation).length===1,'Esc panel missing');
  const panel=host.client.latest().snapshot.objects.find(o=>o.creation);
  host.client.send({kind:'structure',command:{command_id:randomUUID(),action:'duplicate',target:panel.address,expected_revision:panel.object_revision}});
  await until(()=>host.client.latest().snapshot.objects.filter(o=>o.creation).length===2,'Shared creation panel did not duplicate');
  const late=await peer('guest',2);
  await until(()=>shapes.every(shape=>late.client.latest().snapshot.objects.some(o=>o.visual.kind==='primitive'&&o.visual.shape===shape)),'Late join missed primitives');
  assert.ok(late.client.latest().snapshot.objects.some(o=>o.visual.kind==='none'&&o.renderer===undefined),'Late join missed empty Slot');
  assert.ok(late.client.latest().snapshot.objects.some(o=>o.address.instance_id===slot.address.instance_id&&o.world_text?.text==='Text'),'Late join missed WorldText');
  assert.ok(late.client.latest().snapshot.objects.some(o=>o.parent?.instance_id===slot.address.instance_id&&o.world_text?.text==='Text'),'Late join missed text child');
  assert.ok(late.client.latest().snapshot.objects.some(o=>o.visual.kind==='image'&&o.fields.some(f=>f.id==='name'&&f.value.startsWith('Photo '))),'Late join missed camera photo');
  await until(()=>late.client.latest().snapshot.objects.filter(o=>o.creation?.shape==='box').length===2,'Late join missed creation panels or selection');
  const copy=host.client.latest().snapshot.objects.find(o=>o.creation&&o.address.instance_id!==panel.address.instance_id);
  host.client.send({kind:'structure',command:{command_id:randomUUID(),action:'delete',target:copy.address,expected_revision:copy.object_revision}});
  await until(()=>late.client.latest().snapshot.objects.filter(o=>o.creation).length===1,'Panel removal did not replicate');
  assert.ok(host.client.latest().snapshot.objects.every(o=>o.visual.asset_id===undefined));
  console.log('PASS: shared creation surface, distant interaction, drone camera, mirror reflection, local/shared photo, duplicate/delete and late join');
  if(process.argv.includes('--rendered'))console.log(`SCREENSHOT=${resolve(work,'creation.png')}`);
}catch(error){for(const child of children)if(child.logs)console.error(child.logs.slice(-5000));throw error;}
finally{for(const socket of sockets)socket.destroy();for(const child of children)if(child.exitCode===null)child.kill();}
