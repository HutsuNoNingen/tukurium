// Decode Draco mesh streams without changing scene hierarchy or animation accessors.
const fs = require('node:fs');
const path = require('node:path');
const [sourcePath, outputPath, decoderPath = 'draco3d'] = process.argv.slice(2);
if (!sourcePath || !outputPath) throw new Error('Usage: node scripts/convert-avatar.cjs input.glb output.glb [draco3d module path]');
const draco = require(decoderPath);

(async () => {
  const source = fs.readFileSync(sourcePath);
  const jsonLength = source.readUInt32LE(12);
  const gltf = JSON.parse(source.subarray(20,20+jsonLength).toString());
  const original = source.subarray(28+jsonLength);
  const chunks = [original]; let length = original.length;
  const append = buffer => {
    const padding = (4 - length % 4) % 4;
    if (padding) { chunks.push(Buffer.alloc(padding)); length += padding; }
    const offset = length; chunks.push(buffer); length += buffer.length; return offset;
  };
  const module = await draco.createDecoderModule();
  for (const mesh of gltf.meshes) for (const primitive of mesh.primitives) {
    const extension = primitive.extensions?.KHR_draco_mesh_compression;
    if (!extension) continue;
    const view = gltf.bufferViews[extension.bufferView];
    const compressed = original.subarray(view.byteOffset || 0,(view.byteOffset || 0)+view.byteLength);
    const buffer = new module.DecoderBuffer(); buffer.Init(compressed,compressed.length);
    const decoder = new module.Decoder(); const decoded = new module.Mesh();
    const result = decoder.DecodeBufferToMesh(buffer,decoded);
    if (!result.ok()) throw new Error(result.error_msg());
    for (const [semantic,id] of Object.entries(extension.attributes)) {
      const attr = decoder.GetAttributeByUniqueId(decoded,id);
      const values = new module.DracoFloat32Array();
      decoder.GetAttributeFloatForAllPoints(decoded,attr,values);
      const output = Buffer.alloc(values.size()*4);
      for (let i=0;i<values.size();i++) output.writeFloatLE(values.GetValue(i),i*4);
      const accessor = gltf.accessors[primitive.attributes[semantic]];
      accessor.bufferView = gltf.bufferViews.length;
      accessor.byteOffset = 0; accessor.componentType = 5126; accessor.count = decoded.num_points();
      gltf.bufferViews.push({buffer:0,byteOffset:append(output),byteLength:output.length,target:34962});
      module.destroy(values);
    }
    const indices = new module.DracoInt32Array();
    const output = Buffer.alloc(decoded.num_faces()*3*4);
    for(let i=0;i<decoded.num_faces();i++) {
      decoder.GetFaceFromMesh(decoded,i,indices);
      for(let j=0;j<3;j++) output.writeUInt32LE(indices.GetValue(j),(i*3+j)*4);
    }
    const accessor = gltf.accessors[primitive.indices];
    accessor.bufferView = gltf.bufferViews.length; accessor.byteOffset = 0; accessor.componentType = 5125; accessor.count=decoded.num_faces()*3;
    gltf.bufferViews.push({buffer:0,byteOffset:append(output),byteLength:output.length,target:34963});
    delete primitive.extensions.KHR_draco_mesh_compression;
    if (!Object.keys(primitive.extensions).length) delete primitive.extensions;
    for(const object of [indices,decoded,decoder,buffer]) module.destroy(object);
  }
  for (const field of ['extensionsUsed','extensionsRequired']) {
    if(gltf[field]) gltf[field]=gltf[field].filter(x=>x!=='KHR_draco_mesh_compression');
  }
  gltf.buffers[0].byteLength=length;
  let json=Buffer.from(JSON.stringify(gltf));
  json=Buffer.concat([json,Buffer.alloc((4-json.length%4)%4,32)]);
  let binary=Buffer.concat(chunks);
  binary=Buffer.concat([binary,Buffer.alloc((4-binary.length%4)%4)]);
  const header=Buffer.alloc(12);header.writeUInt32LE(0x46546c67);header.writeUInt32LE(2,4);header.writeUInt32LE(28+json.length+binary.length,8);
  const j=Buffer.alloc(8);j.writeUInt32LE(json.length);j.writeUInt32LE(0x4e4f534a,4);
  const b=Buffer.alloc(8);b.writeUInt32LE(binary.length);b.writeUInt32LE(0x004e4942,4);
  fs.mkdirSync(path.dirname(outputPath),{recursive:true});
  fs.writeFileSync(outputPath,Buffer.concat([header,j,json,b,binary]));
  console.log(`Decoded ${gltf.meshes.length} meshes; retained ${gltf.animations.length} animation clips`);
})().catch(e=>{console.error(e);process.exitCode=1;});
