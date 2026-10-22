// A self-contained material-bearing box with padding to exercise multiple transfer chunks.
export function modelFixture(binarySize = 150000) {
  const positions = new Float32Array([-1,-1,-1, 1,-1,-1, 1,1,-1, -1,1,-1, -1,-1,1, 1,-1,1, 1,1,1, -1,1,1]);
  const indices = new Uint16Array([0,2,1,0,3,2,4,5,6,4,6,7,0,1,5,0,5,4,3,7,6,3,6,2,1,2,6,1,6,5,0,4,7,0,7,3]);
  const bin = Buffer.alloc(binarySize);
  Buffer.from(positions.buffer).copy(bin);
  Buffer.from(indices.buffer).copy(bin,96);
  const gltf = {asset:{version:'2.0'},scene:0,scenes:[{nodes:[0]}],nodes:[{mesh:0}],buffers:[{byteLength:bin.length}],bufferViews:[{buffer:0,byteOffset:0,byteLength:96,target:34962},{buffer:0,byteOffset:96,byteLength:72,target:34963}],accessors:[{bufferView:0,componentType:5126,count:8,type:'VEC3',min:[-1,-1,-1],max:[1,1,1]},{bufferView:1,componentType:5123,count:36,type:'SCALAR'}],materials:[{pbrMetallicRoughness:{baseColorFactor:[0.1,0.8,0.5,1],metallicFactor:0,roughnessFactor:0.8},doubleSided:true}],meshes:[{primitives:[{attributes:{POSITION:0},indices:1,material:0}]}]};
  let json=Buffer.from(JSON.stringify(gltf));
  json=Buffer.concat([json,Buffer.alloc((4-json.length%4)%4,32)]);
  const header=Buffer.alloc(20);header.write('glTF');header.writeUInt32LE(2,4);header.writeUInt32LE(28+json.length+bin.length,8);header.writeUInt32LE(json.length,12);header.write('JSON',16);
  const binaryHeader=Buffer.alloc(8);binaryHeader.writeUInt32LE(bin.length);binaryHeader.writeUInt32LE(0x004e4942,4);
  return Buffer.concat([header,json,binaryHeader,bin]);
}
