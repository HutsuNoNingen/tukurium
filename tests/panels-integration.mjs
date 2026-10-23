import {spawn} from 'node:child_process';
import {createConnection} from 'node:net';
import {mkdirSync,readFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {randomUUID} from 'node:crypto';
import {setTimeout as delay} from 'node:timers/promises';
import assert from 'node:assert/strict';
const root=resolve(import.meta.dirname,'..'), session=`panels-${randomUUID()}`;
const base=53000+Math.floor(Math.random()*700), work=resolve(root,'.run',session), saved=resolve(work,'world.json');
mkdirSync(work,{recursive:true});const children=[],sockets=[],clients=[];
function start(file,args){const child=spawn(resolve(root,file),args,{cwd:root,windowsHide:true,stdio:['ignore','pipe','pipe']});child.logs='';child.stdout.on('data',d=>child.logs+=d);child.stderr.on('data',d=>child.logs+=d);children.push(child);return child;}
async function until(test,message,ms=15000){const end=Date.now()+ms;while(Date.now()<end){const value=test();if(value)return value;await delay(20);}throw Error(message);}
async function connect(port){
  for(let attempt=0;attempt<100;attempt++){
    const socket=createConnection({host:'127.0.0.1',port});
    if(!await new Promise(r=>{socket.once('connect',()=>r(true));socket.once('error',()=>r(false));})){socket.destroy();await delay(50);continue;}
    sockets.push(socket);const messages=[];let buffer=Buffer.alloc(0),epoch='';
    socket.on('data',d=>{buffer=Buffer.concat([buffer,d]);while(buffer.length>=4&&buffer.length>=buffer.readUInt32BE()+4){const n=buffer.readUInt32BE(),e=JSON.parse(buffer.subarray(4,n+4));buffer=buffer.subarray(n+4);epoch=e.authority_epoch;messages.push(e.body);}});
    const client={messages,latest:()=>messages.filter(m=>m.kind==='snapshot').at(-1),sessions:()=>messages.filter(m=>m.kind==='panel_sessions').at(-1)?.sessions??[],send(body){const data=Buffer.from(JSON.stringify({protocol_version:2,session_id:session,authority_epoch:epoch,sequence:'1',body}));const h=Buffer.alloc(4);h.writeUInt32BE(data.length);socket.write(Buffer.concat([h,data]));},wait(test){return until(()=>messages.find(test),'Panel response timed out');}};clients.push(client);return client;
  }throw Error('Engine did not start');
}
async function peer(role,index){const port=base+2+index*2;const child=start('target/release/kokoverse-engine.exe',['--role',role,'--session',session,'--port',String(port),'--transport-port',String(port+1),'--asset-dir',resolve(work,`assets-${index}`),...(role==='host'?['--save',saved]:[])]);const client=await connect(port);start('.run/build/kokosync-adapter.exe',['--session',session,'--peer',role==='host'?'host':`guest-${index}`,'--engine',`127.0.0.1:${port+1}`,'--address',`127.0.0.1:${base+1}`,'--insecure-development']);await until(()=>client.latest()?.editable,'Peer did not sync');return {child,client,port};}
async function command(client,kind,payload){const command_id=randomUUID();client.send({kind,[kind==='edit'?'edit':'command']:{command_id,...payload}});return client.wait(m=>m.kind==='result'&&m.command_id===command_id);}
async function edit(client,target,field_id,value){const object=client.latest().snapshot.objects.find(o=>o.address.instance_id===target.instance_id);return command(client,'edit',{target,field_id,value,expected_revision:object.fields.find(f=>f.id===field_id).revision});}
async function input(client,panel,target,action,client_id='writer'){const command_id=randomUUID();client.send({kind:'panel_input',input:{command_id,client_id,panel,target,field_id:'name',action,draft:{part:'0',text:'Shared draft',caret:7,selection:[0,6]}}});return client.wait(m=>m.kind==='panel_result'&&m.command_id===command_id);}
try {
  start('.tools/kokosync-relay.exe',['--http',`127.0.0.1:${base}`,'--quic',`127.0.0.1:${base+1}`]);
  const host=await peer('host',0),guest=await peer('guest',1);
  const target=host.client.latest().snapshot.objects[0].address;
  const created=await command(guest.client,'structure',{action:'create'});assert.equal(created.ok,true);
  const target2=created.selected;
  const panels=[];
  for(const t of [target,target2]){const result=await command(guest.client,'structure',{action:'create_inspector',target:t,position:[0,1.4,1],rotation:[0,0,0,1]});assert.equal(result.ok,true,JSON.stringify(result));panels.push(result.selected);}
  await until(()=>host.client.latest().snapshot.objects.filter(o=>o.panel).length===2,'Panels did not reach host');
  assert.equal((await input(guest.client,panels[0],target,'begin')).ok,true);
  await until(()=>host.client.sessions().some(s=>s.draft.text==='Shared draft'),'Draft/caret did not reach host');
  assert.equal((await input(host.client,panels[0],target,'begin','other')).ok,false);
  assert.equal((await edit(host.client,target,'name','Blocked edit')).ok,false);
  assert.equal((await edit(guest.client,target,'name','Guest edit')).ok,true);
  assert.equal((await input(guest.client,panels[0],target,'end')).ok,true);
  await until(()=>host.client.sessions().length===0,'Lease did not release');
  assert.equal((await edit(host.client,target,'name','Host edit')).ok,true);
  const observer=start('.tools/godot/Godot_v4.7.2-stable_win64_console.exe',['--headless','--path','client-godot','--script','res://panel_observer_smoke.gd','--',`--port=${host.port}`,`--session=${session}`]);
  await until(()=>observer.logs.includes('PANEL OBSERVER READY'),'Observer UI did not start');
  const godot=start('.tools/godot/Godot_v4.7.2-stable_win64_console.exe',[...(process.argv.includes('--rendered')?['--rendering-method','gl_compatibility']:['--headless']),'--path','client-godot','--script','res://panels_smoke.gd','--',`--port=${guest.port}`,`--session=${session}`,...(process.argv.includes('--rendered')?[`--screenshot=${resolve(work,'world-panels.png')}`]:[])]);
  await until(()=>godot.exitCode!==null,'Godot panels test did not finish',60000);
  assert.equal(godot.exitCode,0,godot.logs);assert.doesNotMatch(godot.logs,/SCRIPT ERROR:|\bERROR:/);assert.match(godot.logs,/WORLD PANELS SMOKE PASS/);
  await until(()=>observer.exitCode!==null,'Observer did not finish');assert.equal(observer.exitCode,0,observer.logs);assert.doesNotMatch(observer.logs,/SCRIPT ERROR:|\bERROR:/);assert.match(observer.logs,/PANEL OBSERVER PASS/);
  await until(()=>host.client.latest().snapshot.objects.some(o=>o.fields.find(f=>f.id==='name')?.value==='World panel edit '),'UI edit did not reach host');
  const late=await peer('guest',2);await until(()=>JSON.stringify(late.client.latest().snapshot)===JSON.stringify(host.client.latest().snapshot),'Late join panels differ');
  const pin=await edit(guest.client,panels[1],'panel_pin',{user_id:'$self',offset:[0,1.4,-2],rotation:[0,0,0,1]});assert.equal(pin.ok,true,JSON.stringify(pin));
  await until(()=>host.client.latest().snapshot.objects.find(o=>o.address.instance_id===panels[1].instance_id).panel.attachment!==null,'Pin did not sync');
  await delay(150);const beforePinMove=host.client.latest().snapshot.objects.find(o=>o.address.instance_id===panels[1].instance_id).world_position[0];
  guest.client.send({kind:'player_input',input:{movement:[1,0],yaw:0,pitch:0,jump:false,face_view:false,active:true}});await delay(150);
  guest.client.send({kind:'player_input',input:{movement:[0,0],yaw:0,pitch:0,jump:false,face_view:false,active:false}});
  await until(()=>Math.abs(host.client.latest().snapshot.objects.find(o=>o.address.instance_id===panels[1].instance_id).world_position[0]-beforePinMove)>0.02,'Pinned panel did not follow the moving user');
  await delay(150);assert.equal((await edit(guest.client,panels[1],'panel_pin',null)).ok,true);
  const request_id=randomUUID();host.client.send({kind:'save_world',request_id});assert.equal((await host.client.wait(m=>m.kind==='save_result'&&m.request_id===request_id)).ok,true);
  const expected=JSON.parse(readFileSync(saved));assert.equal(expected.objects.filter(o=>o.panel).length,2);assert.ok(!JSON.stringify(expected).includes('Shared draft'));
  start('target/release/kokoverse-engine.exe',['--role','host','--session',session,'--port',String(base+8),'--transport-port',String(base+9),'--save',saved]);
  const restored=await connect(base+8);await until(()=>restored.latest(),'Restored snapshot missing');assert.deepEqual(restored.latest().snapshot,expected);
  const object=guest.client.latest().snapshot.objects.find(o=>o.address.instance_id===target.instance_id);
  assert.equal((await command(guest.client,'structure',{action:'delete',target,expected_revision:object.object_revision})).ok,true);
  await until(()=>host.client.latest().snapshot.objects.find(o=>o.address.instance_id===panels[0].instance_id).inspector.target===null,'Deleted target not cleared');
  console.log('PASS: shared panels, 3D typing, independent views, scroll, shared gizmo previews, editing leases, pin, late join, save/restore and deleted targets');
  if(process.argv.includes('--rendered'))console.log(`SCREENSHOT=${resolve(work,'world-panels.png')}`);
}catch(error){for(const client of clients)console.error(JSON.stringify(client.messages.filter(m=>!['presence','interaction_rays'].includes(m.kind)).slice(-8).map(m=>m.kind==='snapshot'?{kind:m.kind,revision:m.snapshot.world_revision,objects:m.snapshot.objects.length,editable:m.editable}:m)));for(const child of children)if(child.logs)console.error(child.logs.slice(-6000));throw error;}
finally{for(const socket of sockets)socket.destroy();for(const child of children)if(child.exitCode===null)child.kill();}
