// Publish a ready-to-use camera object, including its WorldText and node logic.
// Set TUKURIUM_TOKEN to publish as an existing signed-in HN, or TUKURIUM_HN
// to obtain a process-local token using the app's HN-only sign-in flow.
import {randomUUID} from 'node:crypto';
import {readFileSync} from 'node:fs';

const service = (process.env.TUKURIUM_SERVICE || 'https://45.77.130.146/api').replace(/\/$/, '');
const title = 'カウントダウンカメラ | ノード';
const description = 'クリックするとカメラ上に3→2→1を表示して撮影します。Camera、WorldText、編集できるノードグラフを含む完成品です。';
const source = readFileSync(new URL('../crates/scripting/templates/countdown_camera.graph', import.meta.url), 'utf8').trimEnd();
if (!source.startsWith('@tukurium-graph-v1\n')) throw Error('Invalid camera graph source');
const graph = JSON.parse(source.slice('@tukurium-graph-v1\n'.length));

function cameraObject() {
  const id = randomUUID();
  const address = {instance_id:id, local_id:1};
  const field = (component_id, id, label, value_type, value) =>
    ({component_id, id, label, value_type, value, revision:'0'});
  const reference = {kind:'field', target:{instance_id:id, local_id:11}, field_id:'world_text'};
  const duration = graph.nodes.find(node => node.id === 'duration' && node.kind === 'public_number')?.value;
  if (typeof duration !== 'number') throw Error('Countdown duration is missing');
  const script = {
    id:randomUUID(), revision:'0', enabled:true, source,
    schema:{name:graph.name, fields:[
      {id:'p_duration', label:'p_duration', value_type:'number', default:duration, min:null, max:null},
      {id:'ref_write', label:'ref_write', value_type:'field_ref', default:null, min:null, max:null},
    ], has_update:true, has_drive:false, has_interact:true},
    parameters:{p_duration:duration, ref_write:reference}, state:{}, clock:{anchor:0, elapsed:0},
  };
  const collider = {half_extents:[0.4,0.25,0.45], solid:true};
  return {
    parent:null, item_identity:null, address,
    renderer:{instance_id:id, local_id:2}, object_revision:'0',
    world_position:[0,1,0], world_rotation:[0,0,0,1], world_scale:[1,1,1],
    collider, visual:{kind:'primitive', shape:'camera'}, playback:null,
    scripts:[script], camera:true,
    world_text:{text:'', color:[1,0.88,0.55,1]},
    fields:[
      field(1,'name','Name','string',title),
      field(1,'parent','Parent','address',null),
      field(3,'position','Position','vec3',[0,1,0]),
      field(2,'color','Color','color',[1,1,1,1]),
      field(3,'rotation','Rotation','quat',[0,0,0,1]),
      field(3,'scale','Scale','vec3',[1,1,1]),
      field(4,'collider','Collider','collider',collider),
      field(5,'playback','Playback','playback',null),
      field(11,'world_text','Text','string',''),
      field(11,'world_text_color','Text color','color',[1,0.88,0.55,1]),
      field(13,'camera_enabled','Enabled','bool',true),
    ],
  };
}

const object = cameraObject();
if (process.argv.includes('--prepare')) {
  console.log(JSON.stringify({title, description, object}, null, 2));
  process.exit(0);
}

let token = process.env.TUKURIUM_TOKEN;
if (!token) {
  const hn = process.env.TUKURIUM_HN;
  if (!hn) throw Error('Set TUKURIUM_TOKEN or TUKURIUM_HN before publishing.');
  const response = await fetch(`${service}/v1/users`, {method:'POST', headers:{'content-type':'application/json'}, body:JSON.stringify({name:hn})});
  const account = await response.json();
  if (!response.ok) throw Error(`HN sign-in: HTTP ${response.status}: ${JSON.stringify(account)}`);
  token = account.token;
}

async function rpc(body) {
  const response = await fetch(`${service}/v1/rpc`, {method:'POST', headers:{authorization:`Bearer ${token}`, 'content-type':'application/json'}, body:JSON.stringify(body)});
  const value = await response.json();
  if (!response.ok) throw Error(`${body.action}: HTTP ${response.status}: ${JSON.stringify(value)}`);
  return value;
}

const me = await rpc({action:'me'});
let existing;
for (let offset = 0;; offset += 12) {
  const page = await rpc({action:'workshop', query:title, offset});
  existing = page.entries.find(entry => entry.title === title && entry.publisher_id === me.id);
  if (existing) break;
  if (offset + page.entries.length >= page.total) break;
}
if (existing) {
  console.log(`EXISTS ${title} publication=${existing.id} publisher=${me.name}`);
} else {
  const version = await rpc({action:'save_item', name:title, object, request_id:randomUUID()});
  const publication = await rpc({action:'publish_item', version_id:version.id, title, description, request_id:randomUUID()});
  console.log(`PUBLISHED ${title} version=${version.id} publication=${publication.id} publisher=${me.name}`);
  existing = publication;
}
const detail = await rpc({action:'workshop_detail', publication_id:existing.id});
const saved = (await rpc({action:'version', version_id:detail.version_id})).object;
const cameraScript = saved.scripts?.find(script => script.schema?.name === graph.name);
if (detail.title !== title || saved.visual?.shape !== 'camera' || saved.camera !== true ||
    saved.world_text?.text !== '' || cameraScript?.source !== source ||
    cameraScript.parameters?.ref_write?.target?.instance_id !== saved.address.instance_id ||
    cameraScript.parameters?.ref_write?.field_id !== 'world_text') {
  throw Error(`Published item does not match the current camera definition: ${detail.id}`);
}
console.log(`VERIFIED publication=${detail.id} version=${detail.version_id} camera=enabled world_text=bound`);
