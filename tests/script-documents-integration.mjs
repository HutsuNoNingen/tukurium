import {spawn,execFileSync} from 'node:child_process';
import {createConnection} from 'node:net';
import {mkdirSync,readFileSync,writeFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {randomUUID} from 'node:crypto';
import {setTimeout as delay} from 'node:timers/promises';
import assert from 'node:assert/strict';

const root=resolve(import.meta.dirname,'..'),session=`script-docs-${randomUUID()}`;
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
async function doc(client, input) {
  const request_id=input.request_id||randomUUID();
  client.send({kind:'script_document',request:{request_id,client_id:'network-editor',name:'Tester',...input}});
  return client.wait(m=>m.kind==='script_document_result'&&m.request_id===request_id);
}
try {
  start('.tools/kokosync-relay.exe',['--http',`127.0.0.1:${base}`,'--quic',`127.0.0.1:${base+1}`]);
  const host=await peer('host',0),guest=await peer('guest',1);
  const target=host.client.latest().snapshot.objects[0].address;
  assert.equal((await command(guest.client,{action:'attach',source:host.client.latest().script_templates[0].source},target)).ok,true);
  await until(()=>host.client.latest().snapshot.objects[0].scripts.length===1,'Script not shared');
  const script=host.client.latest().snapshot.objects[0].scripts[0];
  const common={target,script_id:script.id};
  const opened=await doc(host.client,{...common,action:'open'});assert.equal(opened.ok,true,JSON.stringify(opened));
  assert.equal((await doc(guest.client,{...common,action:'open'})).ok,true);
  const generation=opened.document.generation;
  const a=doc(host.client,{...common,generation,action:'edit',after:'0',insert:'-- Network Alice\n'});
  const b=doc(guest.client,{...common,generation,action:'edit',after:'0',insert:'-- Network Bob\n'});
  for(const result of await Promise.all([a,b]))assert.equal(result.ok,true,JSON.stringify(result));
  const merged=await doc(guest.client,{...common,action:'open'});
  assert.ok(merged.document.text.includes('-- Network Alice')&&merged.document.text.includes('-- Network Bob'));
  assert.equal(host.client.latest().snapshot.objects[0].scripts[0].source,script.source,'Drafts must not execute before Apply');
  console.log('PASS: host/guest concurrent inserts converge without changing executable code');
  execFileSync(process.execPath,[resolve(root,'scripts/publish-workshop-examples.mjs'),'--prepare'],{cwd:root});
  const godot=start('.tools/godot/Godot_v4.7.2-stable_win64_console.exe',[...(process.argv.includes('--rendered')?['--rendering-method','gl_compatibility']:['--headless']),'--path','client-godot','--script','res://shared_script_smoke.gd','--',`--port=${host.port}`,`--session=${session}`,`--graph-fixture=${resolve(root,'.run/workshop-examples/graphs.json')}`,...(process.argv.includes('--rendered')?[`--screenshot=${resolve(work,'shared-script.png')}`]:[])]);
  await until(()=>godot.exitCode!==null,'Shared script UI timed out',60000);
  assert.equal(godot.exitCode,0,godot.logs);assert.doesNotMatch(godot.logs,/SCRIPT ERROR:|\bERROR:/);assert.match(godot.logs,/SHARED SCRIPT SMOKE PASS/);
  console.log('PASS: host world-panel drafts, Field reference binding, and workshop graph number edit');
  if(process.argv.includes('--rendered'))console.log(`SCREENSHOT=${resolve(work,'shared-script.png')}`);
}catch(error){for(const child of children)if(child.logs)console.error(child.logs.slice(-5000));throw error;}
finally{for(const socket of sockets)socket.destroy();for(const child of children)if(child.exitCode===null)child.kill();}
