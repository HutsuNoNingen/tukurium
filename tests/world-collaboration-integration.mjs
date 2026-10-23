import {spawn} from 'node:child_process';
import {createConnection} from 'node:net';
import {mkdirSync} from 'node:fs';
import {resolve} from 'node:path';
import {randomUUID} from 'node:crypto';
import {setTimeout as delay} from 'node:timers/promises';
import assert from 'node:assert/strict';

// Exercise managed identities: the legacy anonymous session never had the owner restriction.
const root = resolve(import.meta.dirname, '..');
const base = 35000 + Math.floor(Math.random() * 3000);
const work = resolve(root, '.run', `world-collaboration-${randomUUID()}`);
const service = `http://127.0.0.1:${base}`;
const processes = [], sockets = [], peers = [];
mkdirSync(work, {recursive:true});

function start(file, args) {
  const child = spawn(resolve(root, file), args, {cwd:root, windowsHide:true, stdio:['ignore','pipe','pipe']});
  child.logs = '';
  child.stdout.on('data', chunk => child.logs += chunk);
  child.stderr.on('data', chunk => child.logs += chunk);
  processes.push(child);
  return child;
}
async function until(test, message, timeout = 20000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    const value = await test();
    if (value) return value;
    await delay(30);
  }
  throw Error(message);
}
async function connect(port) {
  let socket;
  await until(async () => {
    socket = createConnection({host:'127.0.0.1', port});
    return new Promise(done => {
      socket.once('connect', () => done(true));
      socket.once('error', () => {socket.destroy(); done(false);});
    });
  }, 'Engine did not listen');
  sockets.push(socket);
  let buffer = Buffer.alloc(0), session = 'lobby', epoch = '';
  const messages = [];
  socket.on('data', chunk => {
    buffer = Buffer.concat([buffer, chunk]);
    while (buffer.length >= 4 && buffer.length >= buffer.readUInt32BE() + 4) {
      const size = buffer.readUInt32BE(), envelope = JSON.parse(buffer.subarray(4, size + 4));
      buffer = buffer.subarray(size + 4);
      if (envelope.body.kind === 'service_state') session = envelope.session_id;
      if (envelope.session_id === session) epoch = envelope.authority_epoch;
      messages.push(envelope);
    }
  });
  const client = {
    messages,
    body(kind) {return messages.filter(m => m.session_id === session && m.body.kind === kind).at(-1)?.body;},
    send(body) {
      const bytes = Buffer.from(JSON.stringify({protocol_version:3, session_id:session, authority_epoch:epoch, sequence:'1', body}));
      const header = Buffer.alloc(4); header.writeUInt32BE(bytes.length);
      socket.write(Buffer.concat([header, bytes]));
    },
    async call(action, values = {}) {
      const request_id = randomUUID();
      this.send({kind:'service', action, request_id, ...values});
      const result = await until(() => messages.find(m => m.body.kind === 'service_result' && m.body.request_id === request_id)?.body, `Missing ${action}`);
      assert.equal(result.ok, true, JSON.stringify(result));
      return result.data;
    },
    async command(kind, values, ok = true) {
      const command_id = randomUUID(), key = kind === 'edit' ? 'edit' : 'command';
      const revision = this.body('snapshot')?.snapshot.world_revision;
      this.send({kind, [key]:{command_id, ...values}});
      const result = await until(() => messages.find(m => m.body.kind === 'result' && m.body.command_id === command_id)?.body, `Missing ${kind} result`);
      assert.equal(result.ok, ok, JSON.stringify(result));
      if (ok) await until(() => this.body('snapshot')?.snapshot.world_revision !== revision, 'Committed snapshot missing');
      return result;
    }
  };
  await until(() => client.body('service_state'), 'Missing service state');
  return client;
}
async function peer(index) {
  const port = base + 10 + index * 2, folder = resolve(work, `peer-${index}`);
  start(process.env.KOKOVERSE_TEST_ENGINE || 'target/release/kokoverse-engine.exe', [
    '--server', service, '--port', String(port), '--transport-port', String(port + 1),
    '--profile', resolve(folder, 'profile.dat'), '--data-dir', folder, '--asset-dir', resolve(folder, 'assets'),
    '--adapter', resolve(root, '.run/build/kokosync-adapter.exe')
  ]);
  const client = await connect(port); peers.push(client); return client;
}
const object = (client, address) => client.body('snapshot')?.snapshot.objects.find(o => o.address.instance_id === address.instance_id);
async function synchronized() {
  await until(() => peers.every(p => p.body('snapshot')?.editable) &&
    peers.every(p => p.body('snapshot').snapshot.world_revision === peers[0].body('snapshot').snapshot.world_revision), 'Worlds did not converge');
}
async function edit(client, address, field_id, value, ok = true, expected) {
  const field = object(client, address).fields.find(f => f.id === field_id);
  const result = await client.command('edit', {target:{...address, local_id:field.component_id}, field_id, value, expected_revision:expected ?? field.revision}, ok);
  if (ok) await synchronized();
  return result;
}
const input = (active, yaw = 0) => ({kind:'player_input', input:{movement:[0,0], yaw, pitch:0, jump:false, face_view:true, active}});
async function grabOther(client, observer, address) {
  // Remove occlusion so this tests permissions, not accidental picking of another collider.
  for (const other of client.body('snapshot').snapshot.objects.filter(o => o.collider && o.address.instance_id !== address.instance_id)) {
    await edit(client, other.address, 'position', [30, .75, 30]);
  }
  const player = await until(() => client.body('presence')?.participants.find(p => p.id === 'local'), 'Missing local player');
  await edit(client, address, 'position', [player.pose.position[0], .75, player.pose.position[2] - 3]);
  let yaw = 0;
  client.send(input(true, yaw));
  const keepInput = setInterval(() => client.send(input(true, yaw)), 80);
  try {
    await until(() => client.body('interaction_rays')?.rays.some(r => r.owner === 'local' && r.ray.target?.instance_id === address.instance_id), 'Ray did not hit the other user’s object');
    const beginAt = client.messages.length;
    client.send({kind:'grab', action:'begin'});
    await until(() => client.messages.slice(beginAt).some(m => m.body.kind === 'grab_state' && m.body.grab?.preview.target.instance_id === address.instance_id), 'Could not grab another user’s object');
    const revision = client.body('snapshot').snapshot.world_revision, previewAt = observer.messages.length;
    yaw = .3;
    client.send({kind:'grab', action:'rotate', delta:[.3,.1], vertical:false});
    client.send({kind:'grab', action:'scale', factor:1.1});
    const preview = await until(() => observer.messages.slice(previewAt).flatMap(m => m.body.kind === 'object_previews' ? m.body.previews : []).find(p => p.preview.target.instance_id === address.instance_id && p.preview.scale?.[0] > 1.09)?.preview, 'Remote live grab preview missing');
    assert.equal(client.body('snapshot').snapshot.world_revision, revision, 'Preview must not commit the world');
    const releaseAt = client.messages.length;
    client.send({kind:'grab', action:'release'});
    const result = await until(() => client.messages.slice(releaseAt).find(m => m.body.kind === 'result')?.body, 'Grab release did not complete');
    assert.equal(result.ok, true, JSON.stringify(result));
    await until(() => client.body('snapshot').snapshot.world_revision !== revision, 'Released transform was not committed');
    await synchronized();
    const committed = object(observer, address);
    assert.ok(committed.world_scale.every((v, i) => Math.abs(v - preview.scale[i]) < 1e-8));
    assert.notEqual(committed.world_position[0], player.pose.position[0], 'Grab movement was not committed');
  } finally {clearInterval(keepInput); client.send(input(false));}
}

try {
  start('.tools/kokosync-relay.exe', ['--http', `127.0.0.1:${base + 1}`, '--quic', `127.0.0.1:${base + 2}`]);
  start('target/release/kokoverse-asset-server.exe', ['--listen', `127.0.0.1:${base}`, '--data-dir', resolve(work, 'server'), '--relay', `127.0.0.1:${base + 2}`]);
  await until(async () => {try {return (await fetch(service + '/health')).ok;} catch {return false;}}, 'Management service did not start');
  const host = await peer(0), guest = await peer(1);
  const owner = await host.call('login', {name:`Creator ${randomUUID().slice(0,12)}`});
  const editor = await guest.call('login', {name:`Editor ${randomUUID().slice(0,12)}`});
  assert.notEqual(owner.id, editor.id);
  const session = await host.call('create_session', {name:'World collaboration regression'});
  await until(async () => (await host.call('sessions')).some(s => s.id === session.session.id), 'Session not ready');
  await guest.call('join_session', {session_id:session.session.id}); await synchronized();
  const original = host.body('snapshot').snapshot.objects[0].address;
  assert.equal(object(guest, original).item_identity.owner_id, owner.id);
  await edit(guest, original, 'name', 'Edited by another user');
  const oldPositionRevision = object(guest, original).fields.find(f => f.id === 'position').revision;
  await edit(host, original, 'position', [0,.75,0]);
  await edit(guest, original, 'position', [9,.75,0], false, oldPositionRevision);
  await guest.command('script', {action:'attach', target:original, source:'return {name="SharedScript",on_interact=function(ctx) end}'}); await synchronized();
  assert.equal(object(host, original).item_identity.creator_id, owner.id);
  assert.equal(object(host, original).item_identity.owner_id, owner.id);
  assert.ok(object(host, original).item_identity.editors.some(e => e.user_id === editor.id));
  console.log('PASS: another identity edits and adds Lua without taking ownership; stale writes still fail');

  await grabOther(guest, host, original);
  const created = await guest.command('structure', {action:'create'}); await synchronized();
  assert.equal(object(host, created.selected).item_identity.owner_id, editor.id);
  await grabOther(host, guest, created.selected);
  console.log('PASS: host and guest grab, move, rotate and scale each other’s objects with live previews and committed synchronization');

  const panel = await host.command('structure', {action:'create_inspector', target:original, position:[0,1,2], rotation:[0,0,0,1]}); await synchronized();
  await edit(guest, panel.selected, 'inspector_target', created.selected);
  const copy = await guest.command('structure', {action:'duplicate', target:original, expected_revision:object(guest, original).object_revision}); await synchronized();
  await host.command('structure', {action:'delete', target:copy.selected, expected_revision:object(host, copy.selected).object_revision}); await synchronized();
  assert.equal(object(guest, copy.selected), undefined);
  console.log('PASS: shared inspector, duplication and deletion remain collaborative');
  console.log(`TEST_ROOT=${work}`);
} catch (error) {
  for (const process of processes) if (process.logs) console.error(process.logs.slice(-2000));
  throw error;
} finally {
  for (const client of peers) {try {await client.call('leave_session');} catch {}}
  for (const socket of sockets) socket.destroy();
  for (const process of processes.toReversed()) if (process.exitCode === null) process.kill();
}
