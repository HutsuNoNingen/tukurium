import {spawn} from 'node:child_process';
import {createConnection} from 'node:net';
import {mkdirSync,writeFileSync,readFileSync,existsSync} from 'node:fs';
import {resolve} from 'node:path';
import {randomUUID,createHash} from 'node:crypto';
import {setTimeout as delay} from 'node:timers/promises';
import assert from 'node:assert/strict';
import {modelFixture} from './asset-fixture.mjs';
const root=resolve(import.meta.dirname,'..'), session=`assets-${randomUUID()}`, base=49000+Math.floor(Math.random()*800);
const work=resolve(root,'.run',session);mkdirSync(work,{recursive:true});
const mediaMode=process.argv.includes('--media');
const fixture=resolve(work,'drop-model.glb'), bytes=modelFixture(process.argv.includes('--large')?100_000_000:150000), hash=createHash('sha256').update(bytes).digest('hex');writeFileSync(fixture,bytes);
const children=[], sockets=[];
function start(file,args) {const c=spawn(resolve(root,file),args,{cwd:root,windowsHide:true,stdio:['ignore','pipe','pipe']});c.logs='';c.stdout.on('data',d=>c.logs+=d);c.stderr.on('data',d=>c.logs+=d);children.push(c);return c;}
async function connect(port) {
  for(let n=0;n<60;n++) {
    const socket=createConnection({host:'127.0.0.1',port});
    if(!await new Promise(r=>{socket.once('connect',()=>r(true));socket.once('error',()=>r(false));})){socket.destroy();await delay(100);continue;}
    sockets.push(socket);let buffer=Buffer.alloc(0),epoch='';const messages=[];
    socket.on('data',data=>{buffer=Buffer.concat([buffer,data]);while(buffer.length>=4&&buffer.length>=4+buffer.readUInt32BE()){const len=buffer.readUInt32BE(),e=JSON.parse(buffer.subarray(4,len+4));buffer=buffer.subarray(len+4);epoch=e.authority_epoch;messages.push(e.body);}});
    return {messages,send(body){const data=Buffer.from(JSON.stringify({protocol_version:3,session_id:session,authority_epoch:epoch,sequence:'1',body})),h=Buffer.alloc(4);h.writeUInt32BE(data.length);socket.write(Buffer.concat([h,data]));},async wait(predicate,timeout=15000){const until=Date.now()+timeout;while(Date.now()<until){const found=messages.find(predicate);if(found)return found;await delay(20);}throw Error('Asset wait timeout: '+JSON.stringify(messages.slice(-2)));}};
  }throw Error('No engine');
}
async function godot(port,extra) {const c=start('.tools/godot/Godot_v4.7.2-stable_win64_console.exe',['--headless','--path','client-godot','--script',mediaMode?'res://media_smoke.gd':'res://asset_smoke.gd','--',`--port=${port}`,`--session=${session}`,...extra]);const code=await new Promise(r=>c.once('exit',r));assert.equal(code,0,c.logs);assert.doesNotMatch(c.logs,/SCRIPT ERROR:|\bERROR:/);assert.match(c.logs,mediaMode?/MEDIA SMOKE PASS/:/ASSET SMOKE PASS/);return c;}
try {
  start('.tools/kokosync-relay.exe',['--http',`127.0.0.1:${base}`,'--quic',`127.0.0.1:${base+1}`]);
  for(const [role,port] of [['host',base+2],['guest',base+4]]) {
    start('target/debug/kokoverse-engine.exe',['--role',role,'--port',String(port),'--transport-port',String(port+1),'--session',session,'--asset-dir',resolve(work,role)]);
  }
  const host=await connect(base+2), guest=await connect(base+4);
  for(const [peer,port] of [['host',base+3],['guest',base+5]]) start('.tools/kokosync-adapter.exe',['--engine',`127.0.0.1:${port}`,'--address',`127.0.0.1:${base+1}`,'--session',session,'--peer',peer,'--insecure-development']);
  await guest.wait(m=>m.kind==='snapshot'&&m.editable);
  let extra=[`--fixture=${fixture}`];
  if(mediaMode){
    const fbxSource=process.argv.find(a=>a.startsWith('--fbx='))?.slice(6);
    assert.ok(fbxSource,'Pass --fbx=<ASCII FBX fixture> for media tests');
    const source=readFileSync(fbxSource),large=Buffer.alloc(100*1024*1024,32);source.copy(large);large.write('\n;',source.length);
    const fbx=resolve(work,'100MiB.fbx'),video=resolve(work,'test.mp4');writeFileSync(fbx,large);
    const gen=start('.tools/ffmpeg/ffmpeg.exe',['-v','error','-y','-f','lavfi','-i','testsrc=size=320x180:rate=30','-f','lavfi','-i','sine=frequency=440','-t','2','-c:v','libx264','-pix_fmt','yuv420p','-c:a','aac',video]);
    assert.equal(await new Promise(r=>gen.once('exit',r)),0,gen.logs);
    extra=[`--fbx=${fbx}`,`--video=${video}`];
  }
  await godot(base+4,extra);
  const snapshot=(await host.wait(m=>m.kind==='snapshot'&&m.snapshot.objects.some(o=>o.visual.kind=== (mediaMode?'video':'model')&&(mediaMode || o.world_scale[0]===1.5)))).snapshot;
  assert.equal(snapshot.objects.length,3);
  const model=snapshot.objects.find(o=>o.visual.kind==='model');if(!mediaMode)assert.equal(model.visual.asset_id,hash);
  assert.ok(JSON.stringify(model).length<2500,'World holds reference, not model bytes');
  if(!mediaMode)assert.deepEqual(readFileSync(resolve(work,'host',hash+'.glb')),bytes,'Guest upload reached host with exact checksum');
  else for(const item of snapshot.objects.filter(o=>o.visual.asset_id)) {const data=readFileSync(resolve(work,'host',item.visual.asset_id+(item.visual.kind==='video'?'.ogv':'.glb')));assert.equal(createHash('sha256').update(data).digest('hex'),item.visual.asset_id);}
  console.log(mediaMode?'PASS: 100 MiB ASCII FBX converts and renders; MP4 converts, uploads and plays with advancing video frames':'PASS: Explorer drop signal imports image + multi-chunk GLB, model transform replicates, snapshot stores only asset reference');
  // A completely new engine/cache must fetch bytes from the host after reading the snapshot.
  const latePort=base+6,lateCache=resolve(work,'late');
  assert.ok(!existsSync(resolve(lateCache,hash+'.glb')));
  start('target/debug/kokoverse-engine.exe',['--role','guest','--port',String(latePort),'--transport-port',String(latePort+1),'--session',session,'--asset-dir',lateCache]);
  const late=await connect(latePort);
  start('.tools/kokosync-adapter.exe',['--engine',`127.0.0.1:${latePort+1}`,'--address',`127.0.0.1:${base+1}`,'--session',session,'--peer','late','--insecure-development']);
  await late.wait(m=>m.kind==='snapshot'&&m.editable&&m.snapshot.objects.length===3);
  await godot(latePort,mediaMode?[]:[`--asset=${hash}`]);
  if(!mediaMode)assert.deepEqual(readFileSync(resolve(lateCache,hash+'.glb')),bytes);
  console.log(mediaMode?'PASS: late-joining engine receives both model and video; video decodes and plays':'PASS: late-joining engine downloads GLB on demand and Godot renders it from a separate asset cache');
  const before=host.messages.filter(m=>m.kind==='snapshot').at(-1).snapshot.world_revision;
  const bad='0'.repeat(64),at=host.messages.length;
  host.send({kind:'asset_offer',asset_id:bad,size:bytes.length});
  await host.wait((m)=>host.messages.indexOf(m)>=at&&m.kind==='asset_chunk_request'&&m.asset_id===bad);
  host.send({kind:'asset_chunk',asset_id:bad,offset:1,data:bytes.subarray(0,1024).toString('base64')});
  await host.wait(m=>host.messages.indexOf(m)>=at&&m.kind==='asset_error'&&m.asset_id===bad);
  assert.equal(host.messages.filter(m=>m.kind==='snapshot').at(-1).snapshot.world_revision,before);
  console.log('PASS: out-of-order asset chunks are rejected without changing the world');
} catch(e) {for(const c of children) if(c.logs) console.error(c.logs.slice(-5000));throw e;}
finally {for(const s of sockets)s.destroy();for(const c of children)if(c.exitCode===null)c.kill();}
