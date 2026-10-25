import {spawn} from 'node:child_process';
import {createConnection} from 'node:net';
import {mkdirSync} from 'node:fs';
import {resolve} from 'node:path';
import {randomUUID} from 'node:crypto';
import {setTimeout as delay} from 'node:timers/promises';
import assert from 'node:assert/strict';

const root=resolve(import.meta.dirname,'..'),session=`reach-${randomUUID()}`;
const base=54000+Math.floor(Math.random()*700),work=resolve(root,'.run',session);
mkdirSync(work,{recursive:true});const children=[],sockets=[];
function start(file,args){const child=spawn(resolve(root,file),args,{cwd:root,windowsHide:true,stdio:['ignore','pipe','pipe']});child.logs='';child.stdout.on('data',d=>child.logs+=d);child.stderr.on('data',d=>child.logs+=d);children.push(child);return child;}
async function until(test,message,ms=15000){const end=Date.now()+ms;while(Date.now()<end){const value=test();if(value)return value;await delay(20);}throw Error(message);}
async function connect(port){
  for(let attempt=0;attempt<100;attempt++){
    const socket=createConnection({host:'127.0.0.1',port});
    if(!await new Promise(r=>{socket.once('connect',()=>r(true));socket.once('error',()=>r(false));})){socket.destroy();await delay(50);continue;}
    sockets.push(socket);const messages=[];let buffer=Buffer.alloc(0),epoch='';
    socket.on('data',d=>{buffer=Buffer.concat([buffer,d]);while(buffer.length>=4&&buffer.length>=buffer.readUInt32BE()+4){const n=buffer.readUInt32BE(),e=JSON.parse(buffer.subarray(4,n+4));buffer=buffer.subarray(n+4);epoch=e.authority_epoch;messages.push(e.body);}});
    return {messages,latest:()=>messages.filter(m=>m.kind==='snapshot').at(-1),send(body){const data=Buffer.from(JSON.stringify({protocol_version:3,session_id:session,authority_epoch:epoch,sequence:'1',body}));const h=Buffer.alloc(4);h.writeUInt32BE(data.length);socket.write(Buffer.concat([h,data]));}};
  }throw Error('Engine did not start');
}
async function peer(role,index){const port=base+2+index*2;start('target/release/kokoverse-engine.exe',['--role',role,'--session',session,'--port',String(port),'--transport-port',String(port+1),'--asset-dir',resolve(work,`assets-${index}`),...(role==='host'?['--save',resolve(work,'world.json')]:[])]);const client=await connect(port);start('.run/build/kokosync-adapter.exe',['--session',session,'--peer',role==='host'?'host':`guest-${index}`,'--engine',`127.0.0.1:${port+1}`,'--address',`127.0.0.1:${base+1}`,'--insecure-development']);await until(()=>client.latest()?.editable,'Peer did not sync');return {client,port};}
try {
  start('.tools/kokosync-relay.exe',['--http',`127.0.0.1:${base}`,'--quic',`127.0.0.1:${base+1}`]);
  const host=await peer('host',0),guest=await peer('guest',1);
  const rendered=process.argv.includes('--rendered');
  const godot=start('.tools/godot/Godot_v4.7.2-stable_win64_console.exe',[...(rendered?['--rendering-method','gl_compatibility']:['--headless']),'--path','client-godot','--script','res://interaction_smoke.gd','--',`--port=${guest.port}`,`--session=${session}`,...(rendered?[`--screenshot=${resolve(work,'distant-laser.png')}`]:[])]);
  let lines='';
  godot.stdout.on('data',chunk=>{lines+=chunk.toString();for(;;){const end=lines.indexOf('\n');if(end<0)break;const line=lines.slice(0,end).trim();lines=lines.slice(end+1);if(!line.startsWith('INTERACTION_SETUP '))continue;const event=JSON.parse(line.slice('INTERACTION_SETUP '.length));const object=host.client.latest().snapshot.objects.find(o=>o.address.instance_id===event.target.instance_id);if(!object)continue;const rev=id=>object.fields.find(f=>f.id===id).revision;host.client.send({kind:'transform',command:{command_id:randomUUID(),target:event.target,expected_position_revision:rev('position'),expected_rotation_revision:rev('rotation'),expected_scale_revision:rev('scale'),position:event.position,rotation:[0,0,0,1],scale:[1,1,1]}});}});
  await until(()=>godot.exitCode!==null,'Interaction UI timed out',90000);
  assert.equal(godot.exitCode,0,godot.logs);assert.doesNotMatch(godot.logs,/SCRIPT ERROR:|\bERROR:/);assert.match(godot.logs,/INTERACTION SMOKE PASS/);
  assert.ok(guest.client.messages.some(m=>m.kind==='interaction_rays'&&m.rays.some(r=>r.ray.target&&r.ray.point[2]<-20)),'Distant interaction ray did not reach the client');
  console.log('PASS: rendered FPS laser reaches, grabs and releases a 30 m object');
  if(rendered)console.log(`SCREENSHOT=${resolve(work,'distant-laser.png')}`);
}catch(error){for(const child of children)if(child.logs)console.error(child.logs.slice(-5000));throw error;}
finally{for(const socket of sockets)socket.destroy();for(const child of children)if(child.exitCode===null)child.kill();}
