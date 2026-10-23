import { spawn } from 'node:child_process';
import { createConnection } from 'node:net';
import { setTimeout as delay } from 'node:timers/promises';
import { randomUUID } from 'node:crypto';
import assert from 'node:assert/strict';
import { resolve } from 'node:path';

const root = resolve(import.meta.dirname, '..');
const engine = process.argv.find(arg => arg.startsWith('--engine='))?.slice(9) || 'target/debug/kokoverse-engine.exe';
const session = `test-${randomUUID()}`;
const children = [];
const sockets = [];
const base = 49000 + Math.floor(Math.random() * 900);
function start(file, args) {
  const child = spawn(resolve(root, file), args, { cwd: root, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'] });
  child.logs = '';
  child.stdout.on('data', x => { child.logs += x; });
  child.stderr.on('data', x => { child.logs += x; });
  children.push(child);
  return child;
}
async function connect(port) {
  for (let attempt = 0; attempt < 80; attempt++) {
    const socket = createConnection({ host: '127.0.0.1', port });
    const ok = await new Promise(r => { socket.once('connect', () => r(true)); socket.once('error', () => r(false)); });
    if (!ok) { socket.destroy(); await delay(100); continue; }
    sockets.push(socket);
    let buffer = Buffer.alloc(0);
    const messages = [];
    let epoch = '';
    socket.on('data', data => {
      buffer = Buffer.concat([buffer, data]);
      while (buffer.length >= 4 && buffer.length >= buffer.readUInt32BE(0) + 4) {
        const size = buffer.readUInt32BE(0);
        const envelope = JSON.parse(buffer.subarray(4, size + 4));
        buffer = buffer.subarray(size + 4);
        epoch = envelope.authority_epoch;
        messages.push(envelope.body);
      }
    });
    return {
      socket, messages,
      send(body) {
        const data = Buffer.from(JSON.stringify({ protocol_version: 3, session_id: session, authority_epoch: epoch, sequence: '1', body }));
        const header = Buffer.alloc(4); header.writeUInt32BE(data.length);
        socket.write(Buffer.concat([header, data]));
      },
      async wait(predicate, ms = 10000) {
        const end = Date.now() + ms;
        while (Date.now() < end) {
          const found = messages.find(predicate);
          if (found) return found;
          await delay(20);
        }
        throw new Error(`Timed out waiting on port ${port}; recent=${JSON.stringify(messages.slice(-8).map(m=>m.kind==='snapshot'?{kind:m.kind,revision:m.snapshot.world_revision}:m)).slice(0,4000)}`);
      },
      latest() { return messages.filter(m => m.kind === 'snapshot').at(-1)?.snapshot; }
    };
  }
  throw new Error(`Could not connect to ${port}`);
}
const snapshot = m => m.kind === 'snapshot' && m.editable;
const revision = n => m => snapshot(m) && m.snapshot.world_revision === String(n);
const edit = (world, field, value, id = randomUUID()) => ({ kind: 'edit', edit: {
  command_id: id, target: {...world.objects[0].address,local_id:world.objects[0].fields.find(f=>f.id===field).component_id}, field_id: field,
  expected_revision: world.objects[0].fields.find(f => f.id === field).revision, value
}});
const fieldValue = (world, name) => world.objects[0].fields.find(f => f.id === name).value;
const playerInput = (movement = [0,0], options = {}) => ({kind:'player_input',input:{movement,yaw:0,pitch:0,jump:false,face_view:true,active:true,...options}});
const latestPerson = (client, id = 'local') => client.messages.filter(m=>m.kind==='presence').at(-1)?.participants.find(p=>p.id===id);

try {
  start('.tools/kokosync-relay.exe', ['--http', `127.0.0.1:${base}`, '--quic', `127.0.0.1:${base + 1}`]);
  start(engine, ['--port', String(base+2), '--transport-port', String(base+3), '--session', session]);
  const guestEngine = start(engine, ['--role', 'guest', '--port', String(base+4), '--transport-port', String(base+5), '--session', session]);
  const host = await connect(base+2);
  await host.wait(snapshot);
  const guest = await connect(base+4);
  await delay(300);
  start('.tools/kokosync-adapter.exe', ['--engine', `127.0.0.1:${base+3}`, '--address', `127.0.0.1:${base+1}`, '--session', session, '--peer', 'host', '--insecure-development']);
  await delay(350);
  let guestAdapter = start('.tools/kokosync-adapter.exe', ['--engine', `127.0.0.1:${base+5}`, '--address', `127.0.0.1:${base+1}`, '--session', session, '--peer', 'guest', '--insecure-development']);
  const initial = (await guest.wait(snapshot)).snapshot;
  assert.deepEqual(initial, host.latest(), 'initial snapshot matches across distinct ECS engines');
  await host.wait(m => m.kind === 'presence' && m.participants.some(p => p.name === 'Guest'));
  await guest.wait(m => m.kind === 'presence' && m.participants.some(p => p.name === 'Host'));
  console.log('PASS: both ECS engines discover the other avatar via latest');

  const preview = {target:initial.objects[0].address, expected_revision:initial.objects[0].fields.find(f=>f.id==='position').revision, position:[5,3,1]};
  const previewAt = host.messages.length;
  guest.send({kind:'drag_preview',preview});
  const live = await host.wait(m => host.messages.indexOf(m)>=previewAt && m.kind==='object_previews' && m.previews.some(p=>p.preview.position[0]===5));
  assert.ok(JSON.stringify(live).length<600, 'preview contains only target, revision and position');
  assert.equal(host.latest().world_revision, initial.world_revision);
  assert.deepEqual(host.latest(), initial, 'live preview cannot mutate canonical world');
  let clearAt = host.messages.length;
  guest.send({kind:'drag_preview',preview:null});
  await host.wait(m=>host.messages.indexOf(m)>=clearAt && m.kind==='object_previews' && m.previews.length===0);
  guest.send({kind:'drag_preview',preview});
  await delay(200);
  clearAt = host.messages.length;
  await host.wait(m=>host.messages.indexOf(m)>=clearAt && m.kind==='object_previews' && m.previews.length===0, 3000);
  const invalidAt = host.messages.length;
  guest.send({kind:'drag_preview',preview:{...preview,expected_revision:'999'}});
  await delay(150);
  assert.ok(!host.messages.slice(invalidAt).some(m=>m.kind==='object_previews' && m.previews.length));
  console.log('PASS: compact live drag preview, cancellation, expiry and stale revision rejection');

  host.send(edit(initial, 'color', [1, 0.3, 0.1, 1]));
  await guest.wait(revision(1));
  assert.deepEqual(fieldValue(guest.latest(), 'color'), [1, 0.3, 0.1, 1]);
  console.log('PASS: KokoSync host edit reaches guest');

  guest.send(edit(guest.latest(), 'position', [2, 1, -1]));
  await host.wait(revision(2)); await guest.wait(revision(2));
  await guest.wait(m => m.kind === 'result' && m.ok);
  console.log('PASS: guest command is committed by host');

  const old = host.latest();
  host.send(edit(old, 'name', 'First editor'));
  await host.wait(revision(3));
  guest.send(edit(old, 'name', 'Stale editor'));
  await guest.wait(m => m.kind === 'result' && !m.ok);
  assert.equal(fieldValue(host.latest(), 'name'), 'First editor');
  console.log('PASS: stale same-field edit is rejected');

  const duplicate = edit(host.latest(), 'name', 'Only once', 'same-command');
  host.send(duplicate); host.send(duplicate);
  await guest.wait(revision(4)); await delay(300);
  assert.equal(host.latest().world_revision, '4');
  console.log('PASS: repeated command does not execute twice');

  const late = await connect(base+4);
  await late.wait(revision(4));
  assert.deepEqual(late.latest(), host.latest());
  console.log('PASS: renderer reconnect restores current state');

  // Wrong-version and nonexistent-address input cannot change the world.
  const invalid = edit(host.latest(), 'color', [3, 0, 0, 1]);
  host.send(invalid);
  await host.wait(m => m.kind === 'result' && !m.ok);
  await delay(100);
  assert.equal(host.latest().world_revision, '4');
  console.log('PASS: invalid color does not change world revision');

  guest.send({kind:'profile',name:'Guest test'});
  const movementStart = host.messages.length;
  const guestStart = latestPerson(guest).pose.position;
  guest.send(playerInput([1,0], {pitch:0.1}));
  await host.wait(m => host.messages.indexOf(m)>=movementStart && m.kind === 'presence' && m.participants.some(p => p.name === 'Guest test' && p.pose.position[0] > guestStart[0] + 0.2 && p.pose.pitch === 0.1));
  guest.send(playerInput([0,0],{active:false}));
  await delay(150);
  assert.equal(host.latest().world_revision,'4','transient avatars must not revise persistent objects');
  const stopped = latestPerson(guest).pose;
  guest.send(playerInput([9999,0]));
  // Renderer-supplied coordinates are no longer an input route.
  guest.send({kind:'pose',pose:{position:[9999,0,2],yaw:0,pitch:0}});
  await delay(200);
  assert.deepEqual(latestPerson(guest).pose,stopped);
  const timeoutStart = latestPerson(guest).pose.position[0];
  guest.send(playerInput([1,0]));
  await delay(600);
  const timedOut = latestPerson(guest).pose.position[0];
  assert.ok(timedOut > timeoutStart && timedOut-timeoutStart < 1.1);
  await delay(120);
  assert.equal(latestPerson(guest).pose.position[0],timedOut);
  console.log('PASS: Rust simulates movement, rejects coordinates/invalid input and stops stale input');

  const inputClient = await connect(base+4);
  await inputClient.wait(snapshot);
  const jumpStart = host.messages.length;
  inputClient.send(playerInput([1,0],{jump:true}));
  await host.wait(m=>host.messages.indexOf(m)>=jumpStart && m.kind==='presence' && m.participants.some(p=>p.name==='Guest test' && p.pose.position[1]>0.1));
  inputClient.socket.destroy();
  await delay(150);
  const disconnectedX = latestPerson(guest).pose.position[0];
  await delay(850);
  assert.equal(latestPerson(guest).pose.position[0],disconnectedX);
  assert.equal(latestPerson(guest).pose.position[1],0);
  assert.equal(guest.messages.filter(m=>m.kind==='presence').at(-1).local_motion.grounded,true);
  console.log('PASS: renderer disconnect stops walking while Rust completes jump and landing');

  if (!process.argv.includes('--skip-reconnect')) {
  const beforeDisconnect = host.messages.length;
  guestAdapter.kill();
  await guest.wait(m => m.kind === 'status' && !m.editable);
  host.send(edit(host.latest(), 'position', [7, 2, 0]));
  await host.wait(revision(5));
  await host.wait(m => host.messages.indexOf(m) >= beforeDisconnect && m.kind === 'presence' && m.participants.length === 1);
  console.log('PASS: disconnected avatar is removed from the remaining client');
  await delay(800);
  guestAdapter = start('.tools/kokosync-adapter.exe', ['--engine', `127.0.0.1:${base+5}`, '--address', `127.0.0.1:${base+1}`, '--session', session, '--peer', 'guest', '--insecure-development']);
  await guest.wait(revision(5), 50000);
  assert.deepEqual(guest.latest(), host.latest());
  console.log('PASS: KokoSync reconnect requests a fresh snapshot');
  }

  if (process.argv.includes('--godot')) {
    const beforeGodot = Number(host.latest().world_revision);
    const godot = start('.tools/godot/Godot_v4.7.2-stable_win64_console.exe', ['--headless', '--path', 'client-godot', '--script', 'res://smoke.gd', '--', `--port=${base+4}`, `--session=${session}`]);
    let testEvents = '';
    const dragChecks = [];
    const playerChecks = [];
    godot.stdout.on('data', chunk => {
      testEvents += chunk.toString();
      for (;;) {
        const end = testEvents.indexOf('\n');
        if (end < 0) break;
        const line = testEvents.slice(0,end).trim(); testEvents = testEvents.slice(end+1);
        if (line.startsWith('GIZMO_CONFLICT ')) {
          const event = JSON.parse(line.slice('GIZMO_CONFLICT '.length));
          host.send({kind:'edit',edit:{command_id:randomUUID(),target:{...event.target,local_id:3},field_id:'position',expected_revision:event.revision,value:event.value}});
        }
        if (line.startsWith('REMOTE_PREVIEW ')) {
          const preview = JSON.parse(line.slice('REMOTE_PREVIEW '.length));
          host.send({kind:'drag_preview',preview});
          const refresh = setInterval(()=>host.send({kind:'drag_preview',preview}),50);
          setTimeout(()=>{clearInterval(refresh);host.send({kind:'drag_preview',preview:null});},1200);
        }
        if (line.startsWith('LOCAL_PREVIEW ')) {
          const event = JSON.parse(line.slice('LOCAL_PREVIEW '.length));
          const start = host.messages.length;
          const worldRevision = host.latest().world_revision;
          dragChecks.push(host.wait(m=>host.messages.indexOf(m)>=start && m.kind==='object_previews' && m.previews.some(p=>p.preview.target.instance_id===event.target.instance_id && p.preview.position.every((v,i)=>Math.abs(v-event.position[i])<0.02)), 3000).then(()=>assert.equal(host.latest().world_revision,worldRevision)));
        }
        if (line.startsWith('PLAYER_MOVED ')) {
          const position = JSON.parse(line.slice('PLAYER_MOVED '.length));
          playerChecks.push(host.wait(m=>m.kind==='presence' && m.participants.some(p=>p.name==='Guest test' && p.pose.position.every((v,i)=>Math.abs(v-position[i])<0.1))));
        }
      }
    });
    const code = await new Promise(r => godot.once('exit', r));
    assert.equal(code, 0, godot.logs);
    assert.doesNotMatch(godot.logs,/SCRIPT ERROR:|\bERROR:/);
    assert.match(godot.logs, /GODOT SMOKE PASS/);
    await Promise.all(dragChecks);
    await Promise.all(playerChecks);
    assert.equal(playerChecks.length,1);
    assert.equal(dragChecks.length,3,'all three live axis drags must reach the other engine before release');
    await host.wait(revision(beforeGodot + 18));
    const edited = host.latest().objects.find(o => o.address.instance_id === initial.objects[0].address.instance_id);
    assert.equal(edited.fields.find(f=>f.id==='name').value, 'Godot integration');
    const moved = edited.fields.find(f=>f.id==='position').value;
    assert.ok(moved.every((value,index)=>Math.abs(value-[4,2.25,-1.5][index])<0.02));
    assert.ok(host.latest().objects.some(o=>o.visual.kind==='image' && o.visual.width===32 && o.visual.height===16));
    console.log('PASS: Godot forms, three-axis gizmo drag/cancel, FPS, jump and image panels');
    console.log('PASS: Godot input moves the Rust character and reaches the other engine');
    await host.wait(m => m.kind === 'presence' && m.participants.some(p => p.name === 'Guest test' && p.pose.position[1] > 0.2));
    console.log('PASS: jump height reaches the other engine');
  }

  // Host retains client IDs, allowing exact result correlation for structural mutations.
  const previousRevision = Number(host.latest().world_revision);
  const previousCount = host.latest().objects.length;
  const createId = randomUUID();
  host.send({kind:'structure',command:{command_id:createId,action:'create'}});
  const created = await host.wait(m=>m.kind==='result' && m.command_id===createId);
  assert.equal(created.ok,true);
  await guest.wait(revision(previousRevision+1));
  const newObject = guest.latest().objects.find(o=>o.address.instance_id===created.selected.instance_id);
  const duplicateId = randomUUID();
  host.send({kind:'structure',command:{command_id:duplicateId,action:'duplicate',target:newObject.address,expected_revision:newObject.object_revision}});
  const copied = await host.wait(m=>m.kind==='result' && m.command_id===duplicateId);
  assert.equal(copied.ok,true);
  await guest.wait(revision(previousRevision+2));
  assert.equal(guest.latest().objects.length,previousCount+2);
  assert.notEqual(copied.selected.instance_id,newObject.address.instance_id);
  const copy = guest.latest().objects.find(o=>o.address.instance_id===copied.selected.instance_id);
  const deleteId = randomUUID();
  host.send({kind:'structure',command:{command_id:deleteId,action:'delete',target:copy.address,expected_revision:copy.object_revision}});
  await host.wait(m=>m.kind==='result' && m.command_id===deleteId && m.ok);
  await guest.wait(revision(previousRevision+3));
  assert.equal(guest.latest().objects.length,previousCount+1);
  assert.deepEqual(host.latest(),guest.latest());
  console.log('PASS: creation, independent duplication and deletion replicate as object deltas');

  const grabTarget = host.latest().objects.find(o=>o.visual.kind==='cube').address;
  const player = latestPerson(guest).pose.position;
  let aisle = 0;
  for(const object of host.latest().objects.filter(o=>o.visual.kind==='cube' && o.address.instance_id!==grabTarget.instance_id)) {
    const nextRevision = Number(host.latest().world_revision)+1;
    host.send({kind:'edit',edit:{command_id:randomUUID(),target:{...object.address,local_id:3},field_id:'position',expected_revision:object.fields.find(f=>f.id==='position').revision,value:[player[0]+20+aisle++*3,0.75,player[2]]}});
    await guest.wait(revision(nextRevision));
  }
  let currentBox = host.latest().objects.find(o=>o.address.instance_id===grabTarget.instance_id);
  host.send({kind:'edit',edit:{command_id:randomUUID(),target:{...grabTarget,local_id:3},field_id:'position',expected_revision:currentBox.fields.find(f=>f.id==='position').revision,value:[player[0],0.75,player[2]-3]}});
  await guest.wait(revision(Number(host.latest().world_revision)+1));
  let aim = 0;
  guest.send(playerInput([0,0],{yaw:aim}));
  const keepInput = setInterval(()=>guest.send(playerInput([0,0],{yaw:aim})),80);
  try {
    const start = guest.messages.length;
    guest.send({kind:'grab',action:'begin'});
    await guest.wait(m=>guest.messages.indexOf(m)>=start && m.kind==='grab_state' && m.grab);
    const heldRevision = guest.latest().world_revision;
    const previewStart = host.messages.length;
    aim = 0.4;
    await host.wait(m=>host.messages.indexOf(m)>=previewStart && m.kind==='object_previews' && m.previews.some(p=>p.preview.target.instance_id===grabTarget.instance_id && p.preview.position[0]<player[0]-0.5));
    await host.wait(m=>m.kind==='interaction_rays' && m.rays.some(r=>r.ray.holding && r.ray.target?.instance_id===grabTarget.instance_id));
    const transformStart=host.messages.length;
    guest.send({kind:'grab',action:'rotate',delta:[0.3,0.15],vertical:false});
    guest.send({kind:'grab',action:'scale',factor:1.1});
    const transformed = await host.wait(m=>host.messages.indexOf(m)>=transformStart && m.kind==='object_previews' && m.previews.some(p=>p.preview.target.instance_id===grabTarget.instance_id && p.preview.scale?.[0]>1.09 && Math.abs(p.preview.rotation?.[1])>0.1));
    const heldTransform=transformed.previews.find(p=>p.preview.target.instance_id===grabTarget.instance_id).preview;
    assert.equal(host.latest().world_revision,heldRevision);
    const releaseStart = guest.messages.length;
    guest.send({kind:'grab',action:'release'});
    await guest.wait(m=>guest.messages.indexOf(m)>=releaseStart && m.kind==='result' && m.ok);
    await guest.wait(revision(Number(heldRevision)+1));
    assert.deepEqual(guest.latest(),host.latest());
    const placedObject=guest.latest().objects.find(o=>o.address.instance_id===grabTarget.instance_id);
    assert.ok(placedObject.world_rotation.every((value,index)=>Math.abs(value-heldTransform.rotation[index])<1e-12),'Placed rotation preserves the normalized grab rotation');
    assert.deepEqual(placedObject.world_scale,heldTransform.scale);
    const cancelStart = guest.messages.length;
    guest.send({kind:'grab',action:'begin'});
    await guest.wait(m=>guest.messages.indexOf(m)>=cancelStart && m.kind==='grab_state' && m.grab);
    aim = -0.3;
    await delay(200);
    const cancelAt = guest.messages.length;
    guest.send({kind:'grab',action:'cancel'});
    await guest.wait(m=>guest.messages.indexOf(m)>=cancelAt && m.kind==='grab_state' && !m.grab);
    assert.equal(guest.latest().world_revision,String(Number(heldRevision)+1));
    aim=0.4;guest.send(playerInput([0,0],{yaw:aim}));
    const conflictStart=guest.messages.length;
    guest.send({kind:'grab',action:'begin'});
    await guest.wait(m=>guest.messages.indexOf(m)>=conflictStart && m.kind==='grab_state' && m.grab);
    currentBox=host.latest().objects.find(o=>o.address.instance_id===grabTarget.instance_id);
    const changed=currentBox.world_position.map((v,i)=>i===1?v+0.2:v);
    const conflictAt=guest.messages.length;
    host.send({kind:'edit',edit:{command_id:randomUUID(),target:{...grabTarget,local_id:3},field_id:'position',expected_revision:currentBox.fields.find(f=>f.id==='position').revision,value:changed}});
    await guest.wait(m=>guest.messages.indexOf(m)>=conflictAt && m.kind==='grab_state' && !m.grab);
    console.log('PASS: Rust position/rotation/scale previews reach the other engine, atomic release commits and cancel preserves the world');
  } finally {clearInterval(keepInput);guest.send(playerInput([0,0],{active:false}));}

  if (process.argv.includes('--godot')) {
    const interaction = start('.tools/godot/Godot_v4.7.2-stable_win64_console.exe',['--headless','--path','client-godot','--script','res://interaction_smoke.gd','--',`--port=${base+4}`,`--session=${session}`]);
    let lines='';
    interaction.stdout.on('data',chunk=>{
      lines+=chunk.toString();
      for(;;){const end=lines.indexOf('\n');if(end<0)break;const line=lines.slice(0,end).trim();lines=lines.slice(end+1);
        if(line.startsWith('INTERACTION_SETUP ')){
          const event=JSON.parse(line.slice('INTERACTION_SETUP '.length));
          const object=host.latest().objects.find(o=>o.address.instance_id===event.target.instance_id);
          const rev=id=>object.fields.find(f=>f.id===id).revision;
          host.send({kind:'transform',command:{command_id:randomUUID(),target:event.target,expected_position_revision:rev('position'),expected_rotation_revision:rev('rotation'),expected_scale_revision:rev('scale'),position:event.position,rotation:[0,0,0,1],scale:[1,1,1]}});
        }
      }
    });
    const code=await new Promise(r=>interaction.once('exit',r));
    assert.equal(code,0,interaction.logs);assert.doesNotMatch(interaction.logs,/SCRIPT ERROR:|\bERROR:/);assert.match(interaction.logs,/INTERACTION SMOKE PASS/);
    console.log('PASS: real FPS right-click hold, mouse look, wheel, release/cancel, box landing and moving platform');
  }
  guestEngine.kill();
  console.log('All integration checks passed.');
} catch (error) {
  for (const child of children) if (child.logs.trim()) console.error(child.logs.slice(-5000));
  throw error;
} finally {
  for (const socket of sockets) socket.destroy();
  for (const child of children) if (child.exitCode === null) child.kill();
}
