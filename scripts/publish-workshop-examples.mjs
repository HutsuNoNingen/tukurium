// Publish a small, repeatable set of node-graph examples to the signed-in HN's workshop.
// TUKURIUM_TOKEN stays in the caller's environment; no credentials are stored here.
import {randomUUID} from 'node:crypto';
import {mkdirSync, writeFileSync} from 'node:fs';
import {resolve} from 'node:path';

const service = (process.env.TUKURIUM_SERVICE || 'https://45.77.130.146/api').replace(/\/$/, '');
const token = process.env.TUKURIUM_TOKEN;
const preparing = process.argv.includes('--prepare');
const node = (id, kind, value, x, y) => ({id, kind, x, y, ...(value === undefined ? {} : {value})});
const link = (from, out, to, input) => ({from, out, to, input});

const examples = [
  {
    title: 'Node Lab 01 | クリックで色を切り替える',
    description: 'クリックごとに青とオレンジを切り替えます。On Interact、State、Modulo、Select、Write Field の作例です。',
    shape: 'box', color: [0.12, 0.43, 0.9, 1],
    graph: {name: 'ClickColor', nodes: [
      node('event', 'on_interact', undefined, 0, 0), node('state', 'get_state', 'count', 0, 260),
      node('one', 'number', 1, 0, 360), node('sum', 'add', undefined, 220, 270),
      node('two', 'number', 2, 220, 410), node('remainder', 'modulo', undefined, 440, 270),
      node('set', 'set_state', 'count', 660, 0), node('test', 'equal', undefined, 660, 300),
      node('orange', 'color', [1, 0.45, 0.08, 1], 680, 470),
      node('blue', 'color', [0.12, 0.43, 0.9, 1], 680, 580),
      node('choice', 'select', undefined, 900, 320),
      node('paint', 'write_field', undefined, 900, 0),
    ], links: [
      link('event','flow','set','flow'), link('set','next','paint','flow'),
      link('state','value','sum','a'), link('one','value','sum','b'),
      link('sum','value','remainder','a'), link('two','value','remainder','b'),
      link('remainder','value','set','value'), link('remainder','value','test','a'),
      link('one','value','test','b'), link('test','value','choice','test'),
      link('orange','value','choice','yes'), link('blue','value','choice','no'),
      link('choice','value','paint','value'),
    ]}, refs: {paint: 'color'},
  },
  {
    title: 'Node Lab 02 | 回転リング',
    description: '各クライアントで滑らかに回転します。On Drive、Time、公開速度、Rotation Y、Drive Field の作例です。速度はScriptのパラメータから変更できます。',
    shape: 'torus', color: [0.88, 0.58, 0.14, 1],
    graph: {name: 'SpinRing', nodes: [
      node('event','on_drive',undefined,0,0), node('time','time',undefined,0,260),
      node('speed','public_number',45,0,370), node('degrees','multiply',undefined,220,260),
      node('base','read_field',undefined,220,430), node('rotation','rotation_y',undefined,450,270),
      node('turn','drive_field',undefined,690,0),
    ], links: [
      link('event','flow','turn','flow'), link('time','value','degrees','a'),
      link('speed','value','degrees','b'), link('degrees','value','rotation','degrees'),
      link('base','value','rotation','base'), link('rotation','value','turn','value'),
    ]}, refs: {base: 'rotation', turn: 'rotation'},
  },
  {
    title: 'Node Lab 03 | クリック回転ダイヤル',
    description: 'クリックするたびに一定角度だけ回転します。On Interact、Read Field、公開角度、Rotation Y、Write Field の作例です。',
    shape: 'cylinder', color: [0.13, 0.68, 0.67, 1],
    graph: {name: 'ClickDial', nodes: [
      node('event','on_interact',undefined,0,0), node('base','read_field',undefined,0,260),
      node('angle','public_number',45,0,390), node('rotation','rotation_y',undefined,240,270),
      node('turn','write_field',undefined,500,0),
    ], links: [
      link('event','flow','turn','flow'), link('base','value','rotation','base'),
      link('angle','value','rotation','degrees'), link('rotation','value','turn','value'),
    ]}, refs: {base:'rotation', turn:'rotation'},
  },
  {
    title: 'Node Lab 04 | 点滅ビーコン',
    description: '1秒ごとに2色を切り替えるローカル演出です。On Drive、Time、Modulo、Less、Select、Drive Field の作例です。',
    shape: 'sphere', color: [0.1, 0.75, 0.92, 1],
    graph: {name: 'BlinkBeacon', nodes: [
      node('event','on_drive',undefined,0,0), node('time','time',undefined,0,260),
      node('two','number',2,0,360), node('phase','modulo',undefined,230,260),
      node('one','number',1,230,410), node('first','less',undefined,450,260),
      node('cyan','color',[0.1,0.75,0.92,1],450,430),
      node('pink','color',[0.95,0.14,0.58,1],450,540),
      node('choice','select',undefined,690,280), node('paint','drive_field',undefined,900,0),
    ], links: [
      link('event','flow','paint','flow'), link('time','value','phase','a'),
      link('two','value','phase','b'), link('phase','value','first','a'),
      link('one','value','first','b'), link('first','value','choice','test'),
      link('cyan','value','choice','yes'), link('pink','value','choice','no'),
      link('choice','value','paint','value'),
    ]}, refs: {paint: 'color'},
  },
];

if (preparing) {
  const out = resolve('.run/workshop-examples/graphs.json');
  mkdirSync(resolve('.run/workshop-examples'), {recursive: true});
  writeFileSync(out, JSON.stringify(examples.map(e => e.graph), null, 2));
  console.log(`Prepared ${examples.length} graphs: ${out}`);
  process.exit(0);
}
if (!token) throw Error('Set TUKURIUM_TOKEN in the current process.');

async function rpc(body) {
  const response = await fetch(`${service}/v1/rpc`, {
    method: 'POST', headers: {authorization: `Bearer ${token}`, 'content-type': 'application/json'},
    body: JSON.stringify(body),
  });
  const value = await response.json();
  if (!response.ok) throw Error(`${body.action}: HTTP ${response.status}: ${JSON.stringify(value)}`);
  return value;
}

function fieldRef(object, fieldId) {
  const field = object.fields.find(f => f.id === fieldId);
  if (!field) throw Error(`Base item lacks ${fieldId}`);
  return {kind: 'field', target: {instance_id: object.address.instance_id, local_id: field.component_id}, field_id: fieldId};
}
function scriptComponent(example, object) {
  const {graph, refs} = example;
  const fields = [], parameters = {};
  for (const n of graph.nodes) {
    if (n.kind === 'public_number') {
      const id = `p_${n.id}`;
      fields.push({id, label:id, value_type:'number', default:n.value, min:null, max:null});
      parameters[id] = n.value;
    } else if (['read_field','write_field','drive_field'].includes(n.kind)) {
      const id = `ref_${n.id}`;
      fields.push({id, label:id, value_type:'field_ref', default:null, min:null, max:null});
      parameters[id] = fieldRef(object, refs[n.id]);
    }
  }
  fields.sort((a,b) => a.id.localeCompare(b.id, 'en'));
  return {id:randomUUID(), revision:'0', enabled:true,
    source:`@tukurium-graph-v1\n${JSON.stringify(graph)}`,
    schema:{name:graph.name, fields, has_update:graph.nodes.some(n=>n.kind==='on_update'),
      has_drive:graph.nodes.some(n=>n.kind==='on_drive'),
      has_interact:graph.nodes.some(n=>n.kind==='on_interact')},
    parameters, state:{}, clock:{anchor:0, elapsed:0}};
}

const me = await rpc({action:'me'});
const library = await rpc({action:'library'});
const base = library.entries.find(e => e.kind === 'cube' && e.script_count === 0);
if (!base) throw Error('A plain cube in the signed-in HN library is required as a base item.');
const original = (await rpc({action:'version', version_id:base.version_id})).object;
const existing = [];
for (let offset = 0;; offset += 12) {
  const page = await rpc({action:'workshop', query:'Node Lab', offset});
  existing.push(...page.entries);
  if (existing.length >= page.total) break;
}
console.log(`Publishing as ${me.name}; ${existing.length} matching workshop entries already exist.`);
for (const example of examples) {
  if (existing.some(e => e.title === example.title && e.publisher_id === me.id)) {
    console.log(`SKIP ${example.title}`);
    continue;
  }
  const object = structuredClone(original);
  const id = randomUUID();
  object.address.instance_id = id;
  if (object.renderer) object.renderer.instance_id = id;
  object.item_identity = null;
  object.object_revision = '0';
  object.parent = null;
  object.world_position = [0,1,0];
  object.world_rotation = [0,0,0,1];
  object.world_scale = [1,1,1];
  object.visual = {kind:'primitive', shape:example.shape};
  for (const field of object.fields) {
    field.revision = '0';
    if (field.id === 'name') field.value = example.title;
    if (field.id === 'parent') field.value = null;
    if (field.id === 'position') field.value = [0,1,0];
    if (field.id === 'rotation') field.value = [0,0,0,1];
    if (field.id === 'scale') field.value = [1,1,1];
    if (field.id === 'color') field.value = example.color;
  }
  object.scripts = [scriptComponent(example, object)];
  const version = await rpc({action:'save_item', name:example.title, object, request_id:randomUUID()});
  const publication = await rpc({action:'publish_item', version_id:version.id,
    title:example.title, description:example.description, request_id:randomUUID()});
  console.log(`PUBLISHED ${example.title} version=${version.id} publication=${publication.id}`);
}
