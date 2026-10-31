// Offline migration tool only. The native executable never embeds JavaScript.
import { mkdir, writeFile, copyFile, readdir } from 'node:fs/promises';
import { pathToFileURL } from 'node:url';
import { resolve, join } from 'node:path';
import { execFileSync } from 'node:child_process';
const source = resolve(process.argv[2] || 'D:/BakedVoxelLighting');
const destination = resolve('assets');
const load = path => import(pathToFileURL(join(source, path)).href);
const T = await load('node_modules/three/build/three.webgpu.js');
const { mergeGeometries } = await load('node_modules/three/examples/jsm/utils/BufferGeometryUtils.js');
console.log('Generating the original terrain and placements…');
const { makeWorld, random, WORLD_VERSION } = await load('src/world-data.js');
const { terrainPixels, terrainSample, applyTreeInfluence, riverData, groundInfluence, TERRAIN_GRID, routes } = await load('src/terrain-field.js');
const { growPlant } = await load('src/growth.js');
const { woodlandHabitat, grassHabitat } = await load('src/habitat.js');
const world = makeWorld();
applyTreeInfluence(world.trees);
await mkdir(join(destination, 'world'), { recursive: true });
await mkdir(join(destination, 'textures'), { recursive: true });
const write = (name, bytes) => writeFile(join(destination, 'world', name), bytes);
// Fixed binary mesh layout: magic, vertex/index counts, then pos/normal/UV/RGBA floats and u32 indices.
async function mesh(name, geometry) {
  const a = geometry.attributes, count = a.position.count;
  const indices = geometry.index?.array || Uint32Array.from({length:count}, (_,i)=>i);
  const data = Buffer.alloc(12 + count * 48 + indices.length * 4);
  data.write('BVM1'); data.writeUInt32LE(count,4); data.writeUInt32LE(indices.length,8);
  let offset=12;
  for(let i=0;i<count;i++) {
    const value=[a.position.getX(i),a.position.getY(i),a.position.getZ(i),
      a.normal?.getX(i)??0,a.normal?.getY(i)??1,a.normal?.getZ(i)??0,
      a.uv?.getX(i)??0,1-(a.uv?.getY(i)??0),
      a.color?.getX(i)??1,a.color?.getY(i)??1,a.color?.getZ(i)??1,1];
    for(const f of value){data.writeFloatLE(f,offset);offset+=4;}
  }
  for(const index of indices){data.writeUInt32LE(index,offset);offset+=4;}
  await write(name,data);
}
function foliage(leaves, stride=1) {
  const parts=[], dummy=new T.Object3D();
  for(let j=0;j<leaves.length;j+=stride){
    const t=leaves[j],g=new T.PlaneGeometry(1,1),p=g.attributes.position,n=g.attributes.normal;
    for(let i=0;i<n.count;i++){
      const v=new T.Vector3(p.getX(i)*.7,p.getY(i)*.7,.7).normalize();n.setXYZ(i,v.x,v.y,v.z);
    }
    dummy.position.set(...t.p);dummy.rotation.set(...t.r);dummy.scale.set(...t.s);dummy.updateMatrix();g.applyMatrix4(dummy.matrix);
    if(t.n)for(let i=0;i<n.count;i++){
      const v=new T.Vector3().fromBufferAttribute(n,i).multiplyScalar(.45).addScaledVector(new T.Vector3(...t.n),.55).normalize();n.setXYZ(i,v.x,v.y,v.z);
    }
    const c=new T.Color(t.c);g.setAttribute('color',new T.Float32BufferAttribute(Array.from({length:4},()=>[c.r,c.g,c.b]).flat(),3));parts.push(g);
  }
  const merged=mergeGeometries(parts);parts.forEach(g=>g.dispose());return merged;
}
const variants=[];
for(let i=0;i<15;i++){
  const shrub=i>=12,k=shrub?i-12:i%4,age=shrub?1:Math.floor(i/4);
  console.log(`Exporting ${shrub?'shrub':'tree'} ${i+1}/15`);
  const plant=growPlant(shrub?552+k*83:9801+i*137,k,shrub,age);
  const levels=[];
  for(let lod=0;lod<3;lod++){
    const wood=`plant-${i}-wood-${lod}.mesh`,leaf=`plant-${i}-leaf-${lod}.mesh`;
    await mesh(wood,plant[['high','low','farWood'][lod]]);
    const leaves=foliage(plant.leaves,[1,2,6][lod]);await mesh(leaf,leaves);leaves.dispose();levels.push({wood,leaf});
  }
  variants.push({bark:shrub?'bark-ash':['bark','bark-birch','bark-ash','bark'][k],leaf:shrub?['hazel','bramble','ash'][k]:['oak','birch','ash','pine'][k],levels});
}
const shrubs=[];
for(const c of world.clusters){const rng=random(c.id+4100);
  for(let k=0;k<3;k++)for(let patch=0;patch<6;patch++){
    const anchor=c.trees[Math.floor(rng()*c.trees.length)];if(!anchor)continue;
    const a=rng()*Math.PI*2,r=5+rng()*12,px=anchor.x+Math.cos(a)*r,pz=anchor.z+Math.sin(a)*r;
    for(let j=0;j<3;j++){
      const x=px+(rng()-.5)*6,z=pz+(rng()-.5)*6,s=terrainSample(x,z);
      if(s.traffic>.12||s.slope>.58||s.riverDistance<9||s.root>.6)continue;
      const h=grassHabitat(x,z,s),w=woodlandHabitat(x,z,s);
      const suitable=k===0?.15+w.edge*.6+s.canopy*.22:k===1?.08+w.edge*.75+w.gap*w.stand*.4:.06+s.moisture*.35+w.edge*.3;
      if(rng()>suitable*(1-h.woodland*.65))continue;
      shrubs.push({x,y:s.height-.03,z,scale:(k===1?.48:.7)+rng()*(k===0?.85:.6),rotation:rng()*6.28,variant:12+k});
    }
  }
}
console.log('Packing terrain fields and river surface…');
await write('terrain.f32',Buffer.from(terrainPixels.buffer));
// Same geographic samples as the browser, including canopy/root influence.
const ecology=new Float32Array(TERRAIN_GRID*TERRAIN_GRID*4);
for(let z=0;z<TERRAIN_GRID;z++)for(let x=0;x<TERRAIN_GRID;x++){
  const s=terrainSample(x*2-2000,z*2-2000),i=(z*TERRAIN_GRID+x)*4;
  ecology.set([s.root,s.canopy,s.riverDistance,s.forest],i);
}
await write('ecology.f32',Buffer.from(ecology.buffer));
await write('riverbank.rgba',Buffer.from(groundInfluence.buffer));
const rivers=[];
for(let i=0;i<riverData.chunks.length;i++){
  const c=riverData.chunks[i],g=new T.BufferGeometry();
  g.setAttribute('position',new T.BufferAttribute(c.position,3));g.setAttribute('normal',new T.BufferAttribute(c.normal,3));g.setAttribute('uv',new T.BufferAttribute(c.coord,2));g.setIndex(new T.BufferAttribute(c.index,1));
  const name=`river-${i}.mesh`;await mesh(name,g);await write(name+".flow",Buffer.from(c.flow.buffer));rivers.push(name);
}
for(const name of await readdir(join(source,'public/textures')))if(name.endsWith('.png'))await copyFile(join(source,'public/textures',name),join(destination,'textures',name));
const manifest={format:1,sourceCommit:execFileSync('git',['rev-parse','HEAD'],{cwd:source,encoding:'utf8'}).trim(),worldVersion:WORLD_VERSION,extent:4000,grid:TERRAIN_GRID,seed:world.seed,variants,trees:world.trees,shrubs,deadwood:world.deadwood,rivers,routes};
// Publish manifest last, so interrupted exports are never treated as complete.
await write('manifest.json',JSON.stringify(manifest));
console.log(`Export complete: ${world.trees.length} trees, ${shrubs.length} shrubs, ${rivers.length} river pages.`);
