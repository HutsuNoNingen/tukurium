import {spawn} from 'node:child_process';
import {createConnection} from 'node:net';
import {mkdirSync,readFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {randomUUID,createHash} from 'node:crypto';
import {setTimeout as delay} from 'node:timers/promises';
import assert from 'node:assert/strict';

const root=resolve(import.meta.dirname,'..'),session=`playback-${randomUUID()}`,base=49000+Math.floor(Math.random()*800);
const work=resolve(root,'.run',session);mkdirSync(work,{recursive:true});
const children=[],sockets=[];
const rendered=process.argv.includes('--rendered');
const relay=process.argv.find(a=>a.startsWith('--relay='))?.slice(8);
const adapter=process.argv.find(a=>a.startsWith('--adapter='))?.slice(10) || '.tools/kokosync-adapter.exe';
function start(file,args) {
  const child=spawn(resolve(root,file),args,{cwd:root,windowsHide:true,stdio:['ignore','pipe','pipe']});
  child.logs='';child.stdout.on('data',d=>child.logs+=d);child.stderr.on('data',d=>child.logs+=d);
  children.push(child);return child;
}
async function until(predicate,message,timeout=25000) {
  const deadline=Date.now()+timeout;
  while(Date.now()<deadline){const value=predicate();if(value)return value;await delay(20);}
  throw Error(message);
}
async function connect(port) {
  for(let i=0;i<60;i++) {
    const socket=createConnection({host:'127.0.0.1',port});
    if(!await new Promise(r=>{socket.once('connect',()=>r(true));socket.once('error',()=>r(false));})){socket.destroy();await delay(100);continue;}
    sockets.push(socket);let buffer=Buffer.alloc(0),epoch='';const messages=[];
    socket.on('data',data=>{buffer=Buffer.concat([buffer,data]);while(buffer.length>=4&&buffer.length>=4+buffer.readUInt32BE()){const len=buffer.readUInt32BE(),e=JSON.parse(buffer.subarray(4,len+4));buffer=buffer.subarray(len+4);epoch=e.authority_epoch;messages.push(e.body);}});
    return {messages,send(body){const data=Buffer.from(JSON.stringify({protocol_version:2,session_id:session,authority_epoch:epoch,sequence:'1',body})),header=Buffer.alloc(4);header.writeUInt32BE(data.length);socket.write(Buffer.concat([header,data]));},wait(predicate){return until(()=>messages.find(predicate),'Message timeout: '+JSON.stringify(messages.slice(-1)));}};
  }
  throw Error('Engine connection failed');
}
async function peer(name,index) {
  const port=base+2+index*2;
  start('target/debug/kokoverse-engine.exe',['--role',name==='host'?'host':'guest','--port',String(port),'--transport-port',String(port+1),'--session',session,'--asset-dir',resolve(work,name)]);
  const client=await connect(port);
  start(adapter,['--engine',`127.0.0.1:${port+1}`,'--address',relay || `127.0.0.1:${base+1}`,'--session',session,'--peer',name,...(relay?[]:['--insecure-development'])]);
  await client.wait(m=>m.kind==='snapshot'&&m.editable);
  return {client,port};
}
function renderer(port,late=false) {
  return start('.tools/godot/Godot_v4.7.2-stable_win64_console.exe',[...(rendered?['--rendering-method','gl_compatibility']:['--headless']),'--path','client-godot','--script','res://playback_smoke.gd','--',`--port=${port}`,`--session=${session}`,...(late?['--late']:[])]);
}
function latest(client) {return client.messages.filter(m=>m.kind==='snapshot').at(-1).snapshot;}
function video(client) {return latest(client).objects.find(o=>o.visual.kind==='video');}
async function playback(client,action,extra={}) {
  const object=video(client),id=randomUUID(),start=client.messages.length;
  client.send({kind:'playback',command:{command_id:id,target:object.address,expected_revision:object.fields.find(f=>f.id==='playback').revision,action,...extra}});
  const result=await client.wait(m=>client.messages.indexOf(m)>=start&&m.kind==='result');assert.equal(result.ok,true,JSON.stringify(result));
  return latest(client).world_revision;
}
try {
  if(!relay)start('.tools/kokosync-relay.exe',['--http',`127.0.0.1:${base}`,'--quic',`127.0.0.1:${base+1}`]);
  const host=await peer('host',0),guest=await peer('guest',1);
  const path=resolve(work,'clock-test.ogv');
  const gen=start('.tools/ffmpeg/ffmpeg.exe',['-v','error','-y','-f','lavfi','-i','testsrc=size=320x180:rate=30','-f','lavfi','-i','sine=frequency=440','-t','12','-c:v','libtheora','-q:v','5','-c:a','libvorbis',path]);
  assert.equal(await new Promise(r=>gen.once('exit',r)),0,gen.logs);
  const bytes=readFileSync(path),hash=createHash('sha256').update(bytes).digest('hex');
  host.client.send({kind:'asset_offer',asset_id:hash,size:bytes.length});
  for(let offset=0;offset<bytes.length;offset+=128*1024){
    await host.client.wait(m=>m.kind==='asset_chunk_request'&&m.asset_id===hash&&m.offset===offset);
    host.client.send({kind:'asset_chunk',asset_id:hash,offset,data:bytes.subarray(offset,offset+128*1024).toString('base64')});
  }
  await host.client.wait(m=>m.kind==='asset_ready'&&m.asset_id===hash);
  const importId=randomUUID();
  host.client.send({kind:'structure',command:{command_id:importId,action:'import_asset',name:'Clock test',position:[0,1,0],rotation:[0,0,0,1],visual:{kind:'video',asset_id:hash,width:320,height:180,duration_seconds:12}}});
  assert.equal((await host.client.wait(m=>m.kind==='result'&&m.command_id===importId)).ok,true);
  await guest.client.wait(m=>m.kind==='snapshot'&&m.snapshot.objects.some(o=>o.visual.kind==='video'));
  let revision=await playback(host.client,'pause');
  await until(()=>latest(guest.client).world_revision===revision,'Pause not replicated');
  revision=await playback(guest.client,'seek',{position_seconds:3.25});
  await until(()=>latest(host.client).world_revision===revision,'Seek not replicated');
  const pausedState=video(host.client).playback;
  const guestState=video(guest.client).playback;
  assert.equal(pausedState.playing,guestState.playing);
  assert.equal(pausedState.looping,guestState.looping);
  assert.equal(pausedState.position_seconds,guestState.position_seconds);
  assert.ok(Math.abs(pausedState.anchor_seconds-guestState.anchor_seconds)<0.000001);
  assert.equal(pausedState.position_seconds,3.25);assert.equal(pausedState.playing,false);
  const viewers=[renderer(host.port),renderer(guest.port)];
  for(const viewer of viewers)await until(()=>viewer.logs.includes('PLAYBACK READY'),'Paused renderer not ready');
  // Each renderer uses a different monotonic timer; both must calibrate against the host.
  revision=await playback(guest.client,'play');
  await until(()=>latest(host.client).world_revision===revision,'Play not replicated');
  for(const viewer of viewers)await until(()=>viewer.logs.includes('PLAYBACK RUNNING'),'Renderer failed drift correction');
  const late=await peer('late',2);
  await late.client.wait(m=>m.kind==='snapshot'&&m.snapshot.objects.some(o=>o.visual.kind==='video'));
  viewers.push(renderer(late.port,true));
  await until(()=>viewers[2].logs.includes('PLAYBACK RUNNING'),'Late renderer did not catch up');
  assert.equal(latest(host.client).world_revision,revision,'Clock calibration/drift correction must not change world revisions');
  for(const viewer of viewers){
    const sample=JSON.parse(viewer.logs.match(/PLAYBACK RUNNING (.+)/)[1]);
    assert.ok(Math.abs(sample.position-sample.expected)<0.28,JSON.stringify(sample));
  }
  console.log('PASS: shared play and paused seek, separate renderer clocks, decoder drift repair, late join with empty cache');
  revision=await playback(host.client,'pause');
  await until(()=>latest(guest.client).world_revision===revision,'Second pause not replicated');
  await playback(guest.client,'seek',{position_seconds:5});
  for(const viewer of viewers){
    await until(()=>viewer.exitCode!==null,'Renderer did not finish');
    assert.equal(viewer.exitCode,0,viewer.logs);assert.doesNotMatch(viewer.logs,/SCRIPT ERROR:|\bERROR:/);assert.match(viewer.logs,/PLAYBACK SMOKE PASS/);
  }
  if(rendered)assert.equal(new Set(viewers.map(v=>v.logs.match(/PLAYBACK FRAME (\d+)/)[1])).size,1,'All renderers decode the same frame after seeking');
  assert.equal(video(host.client).playback.position_seconds,5);
  console.log('PASS: host pause and guest seek reach all three renderers and their inspector timelines');
} catch(error) {
  for(const child of children)if(child.logs)console.error(child.logs.slice(-3500));throw error;
} finally {
  for(const socket of sockets)socket.destroy();for(const child of children)if(child.exitCode===null)child.kill();
}
