"""Bake the unchanged source grass cards into a padded atlas and distant colonies.
No illumination is baked: the Vulkan material lights all LODs consistently.
Run after export-reference.mjs when source textures/catalog change.
"""
from pathlib import Path
import json, re, random, ast
import numpy as np
from PIL import Image
root=Path(__file__).resolve().parents[1]
source=(root/'crates/woodland/src/plant_catalog.rs').read_text()
names=re.findall(r'"([^"]+)"',source)
mixes=ast.literal_eval('['+source.split('pub const MIXES')[1].split('= [',1)[1].rsplit('];',1)[0]+']')
size=1254; pad=13; cell=size+pad*2
atlas=Image.new('RGBA',(cell*4,cell*4))
normal_atlas=Image.new('RGBA',atlas.size,(128,128,255,100))
textures=[Image.open(root/'assets/textures'/f'{n}.png').convert('RGBA') for n in names]
means=[]
for tex in textures:
 a=np.asarray(tex,dtype=np.float32)/255
 rgb=np.where(a[:,:,:3]<=.04045,a[:,:,:3]/12.92,((a[:,:,:3]+.055)/1.055)**2.4)
 weight=a[:,:,3:4]*(a[:,:,3:4]>.3)
 means.append((rgb*weight).sum((0,1))/weight.sum())
(root/'assets/world/grass-means.json').write_text(json.dumps([v.tolist() for v in means]))
for index in range(16):
 if index<12: tile=textures[index]
 else:
  rng=random.Random(719+index)
  tile=Image.new('RGBA',(size,size))
  # Layer actual species silhouettes into a 4.2 m wide, 2.4 m high colony.
  for j in range(56):
   species=rng.choices(range(12),weights=mixes[index-12])[0]
   width=rng.uniform(.65,1.05); height=rng.uniform(.55,1.1)
   if species in (4,9): height*=.4
   if species in (2,6,8,10): height*=1.35
   w=int(width/4.2*size); h=int(height/2.4*size)
   tuft=textures[species].resize((w,h),Image.Resampling.LANCZOS)
   cluster=(.20,.45,.72,.86)[j%4]
   x=int(max(.03,min(.97,rng.gauss(cluster,.13)))*size)-w//2
   y=int(size*rng.uniform(.85,1.))-h
   if rng.random()<.5: tuft=tuft.transpose(Image.Transpose.FLIP_LEFT_RIGHT)
   tile.alpha_composite(tuft,(x,y))
 # Source normalKernel with strength=3 and foliage transmission in alpha.
 a=np.asarray(tile.convert('RGBA'),dtype=np.float32)/255.
 value=(a[:,:,:3]*np.array([.299,.587,.114])).sum(2)*a[:,:,3]
 edge=np.pad(value,1,mode='edge')
 dx=(edge[1:-1,2:]-edge[1:-1,:-2])*3
 dy=(edge[2:,1:-1]-edge[:-2,1:-1])*3
 n=np.stack((-dx,-dy,np.ones_like(dx)),axis=2)
 n/=np.linalg.norm(n,axis=2,keepdims=True)
 alpha=np.pad(a[:,:,3],5,mode='edge')
 density=(alpha[:-10,:-10]+alpha[10:,10:]+alpha[:-10,10:]+alpha[10:,:-10])*.25
 pigment=(a[:,:,:3]*np.array([.2126,.7152,.0722])).sum(2)
 response=np.clip(.28+(1-density)*.44+pigment*.24,.2,.92)
 normal=np.concatenate((n*.5+.5,response[:,:,None]),axis=2)
 normal_atlas.paste(Image.fromarray(np.rint(normal*255).astype(np.uint8)),(index%4*cell+pad,index//4*cell+pad))
 # Transparent border prevents neighbouring species entering lower mips.
 atlas.paste(tile,(index%4*cell+pad,index//4*cell+pad))
atlas.save(root/'assets/textures/grass-lod-atlas.png')
normal_atlas.save(root/'assets/textures/grass-lod-normal.png')
print('Baked 12 full-resolution species + 4 colonies:',atlas.size)

