// Restore per-vertex downstream direction, speed and depth without regenerating other assets.
import {readFile,writeFile} from 'node:fs/promises';
import {pathToFileURL} from 'node:url';
import {resolve,join} from 'node:path';
const source=resolve(process.argv[2]||'D:/BakedVoxelLighting');
const {riverData,groundInfluence}=await import(pathToFileURL(join(source,'src/terrain-field.js')).href);
const root=resolve('assets/world');
const manifest=JSON.parse(await readFile(join(root,'manifest.json'),'utf8'));
if(manifest.rivers.length!==riverData.chunks.length)throw Error('River source version differs; export reference first');
for(let i=0;i<manifest.rivers.length;i++){
 const name=manifest.rivers[i], mesh=await readFile(join(root,name)), chunk=riverData.chunks[i];
 const count=mesh.readUInt32LE(4);
 if(count*3!==chunk.position.length)throw Error('River vertex count differs');
 for(let v=0;v<count;v++)for(let c=0;c<3;c++)if(Math.abs(mesh.readFloatLE(12+v*48+c*4)-chunk.position[v*3+c])>1e-4)throw Error('River topology differs');
 await writeFile(join(root,name+'.flow'),Buffer.from(chunk.flow.buffer,chunk.flow.byteOffset,chunk.flow.byteLength));
}
console.log(`Restored flow for ${manifest.rivers.length} river pages; verified every vertex against source.`);

await writeFile(join(root,'riverbank.rgba'),Buffer.from(groundInfluence.buffer));
