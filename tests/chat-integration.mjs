import {spawn} from 'node:child_process';
import {createConnection} from 'node:net';
import {mkdirSync} from 'node:fs';
import {resolve} from 'node:path';
import {randomUUID} from 'node:crypto';
import {setTimeout as delay} from 'node:timers/promises';
import assert from 'node:assert/strict';

const root=resolve(import.meta.dirname,'..'), session=`chat-${randomUUID()}`;
const base=49000+Math.floor(Math.random()*800), work=resolve(root,'.run',session);
mkdirSync(work,{recursive:true});
const processes=[],sockets=[];
function start(file,args){
  const child=spawn(resolve(root,file),args,{cwd:root,windowsHide:true,stdio:['ignore','pipe','pipe']});
  child.logs='';child.stdout.on('data',data=>child.logs+=data);child.stderr.on('data',data=>child.logs+=data);
  processes.push(child);return child;
}
async function until(check,message,timeout=30000){
  const end=Date.now()+timeout;
  while(Date.now()<end){const value=check();if(value)return value;await delay(25);}
  throw Error(message);
}
async function connect(port){
  let socket;
  for(let i=0;i<100;i++){
    socket=createConnection({host:'127.0.0.1',port});
    if(await new Promise(done=>{socket.once('connect',()=>done(true));socket.once('error',()=>done(false));}))break;
    socket.destroy();await delay(50);
  }
  if(!socket||socket.destroyed)throw Error('Engine did not listen');
  sockets.push(socket);
  let buffer=Buffer.alloc(0), sequence=0;
  const messages=[];
  socket.on('data',data=>{
    buffer=Buffer.concat([buffer,data]);
    while(buffer.length>=4&&buffer.length>=buffer.readUInt32BE()+4){
      const size=buffer.readUInt32BE();messages.push(JSON.parse(buffer.subarray(4,size+4)));
      buffer=buffer.subarray(size+4);
    }
  });
  return {
    messages,
    latest(kind){return messages.filter(message=>message.body.kind===kind).at(-1);},
    send(body){
      const state=this.latest('snapshot');
      const data=Buffer.from(JSON.stringify({protocol_version:3,session_id:session,authority_epoch:state.authority_epoch,sequence:String(++sequence),body}));
      const frame=Buffer.alloc(4);frame.writeUInt32BE(data.length);
      socket.write(Buffer.concat([frame,data]));
    },
  };
}
async function peer(index){
  const port=base+2+index*2;
  start('target/release/kokoverse-engine.exe',['--role','auto','--peer','auto','--name',`Person ${index}`,'--port',String(port),'--transport-port',String(port+1),'--session',session,'--asset-dir',resolve(work,`cache-${index}`)]);
  const client=await connect(port);
  start('.run/build/kokosync-adapter.exe',['--peer','auto','--engine',`127.0.0.1:${port+1}`,'--address',`127.0.0.1:${base+1}`,'--session',session,'--insecure-development']);
  await until(()=>client.latest('snapshot')?.body.editable,`Peer ${index} did not synchronize`);
  return client;
}
try{
  start('.tools/kokosync-relay.exe',['--http',`127.0.0.1:${base}`,'--quic',`127.0.0.1:${base+1}`]);
  const host=await peer(0),guest=await peer(1);
  assert.equal(host.latest('snapshot').body.role,'host');
  assert.equal(guest.latest('snapshot').body.role,'guest');
  await until(()=>host.latest('presence')?.body.participants.length===2,'Host did not see guest presence');
  guest.send({kind:'chat_send',text:'hello from guest',sender_id:'forged'});
  const message=await until(()=>host.latest('chat_state')?.body.messages.find(item=>item.text==='hello from guest'),'Host did not receive guest chat');
  await until(()=>guest.latest('chat_state')?.body.messages.some(item=>item.id===message.id),'Guest did not receive host-confirmed chat');
  assert.equal(message.sender_name,'Person 1');
  assert.notEqual(message.sender_id,'forged');
  assert.equal(host.latest('snapshot').body.snapshot.world_revision,'0');
  const late=await peer(2);
  await until(()=>late.latest('snapshot')?.body.chat.some(item=>item.id===message.id),'Late peer did not receive chat history');
  await delay(350);
  guest.send({kind:'chat_send',text:'bad\u0000text'});
  await until(()=>guest.latest('chat_error')?.body.message==='Invalid chat message','Invalid chat was not rejected');
  assert.equal(host.latest('chat_state').body.messages.length,1);
  console.log('PASS: host-confirmed chat, sender identity, guest delivery, late join, validation and unchanged world revision');
}catch(error){for(const child of processes)if(child.logs)console.error(child.logs.slice(-2500));throw error;}
finally{for(const socket of sockets)socket.destroy();for(const child of processes)if(child.exitCode===null)child.kill();}
