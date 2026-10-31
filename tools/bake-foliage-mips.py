"""Offline, alpha-weighted linear RGB mipmaps with per-atlas-cell alpha coverage.
Source PNG fingerprint is checked by the Rust loader; stale bakes are rejected.
"""
from pathlib import Path
import json, struct
import numpy as np
from PIL import Image
root=Path(__file__).resolve().parents[1]
manifest=json.loads((root/'assets/world/manifest.json').read_text())
files={v['leaf']: (.35,1) for v in manifest['variants']}
files.update({'grass-lod-atlas':(.34,4), 'impostor-atlas':(.32,4)})
def fingerprint(data):
    h=14695981039346656037
    for b in data: h=((h^b)*1099511628211)&0xffffffffffffffff
    return h
def resize(a,w,h):
    return np.stack([np.asarray(Image.fromarray(a[:,:,c]).resize((w,h),Image.Resampling.BOX)) for c in range(a.shape[2])],axis=2)
for name,(cutoff,grid) in files.items():
    path=root/'assets/textures'/f'{name}.png'
    a=np.asarray(Image.open(path).convert('RGBA'),dtype=np.float32)/255.
    height,width=a.shape[:2]
    rgb=np.where(a[:,:,:3]<=.04045,a[:,:,:3]/12.92,((a[:,:,:3]+.055)/1.055)**2.4)
    # Keep raw coverage between levels, so alpha compensation does not accumulate.
    raw=np.concatenate((rgb*a[:,:,3:4],a[:,:,3:4]),axis=2)
    targets=[]
    for z in range(grid):
      for x in range(grid):
        tile=a[z*height//grid:(z+1)*height//grid,x*width//grid:(x+1)*width//grid,3]
        targets.append(float(np.mean(tile>=cutoff)))
    payload=bytearray(); w,h=width,height; levels=0
    while w>1 or h>1:
      w=max(1,w//2);h=max(1,h//2);levels+=1
      raw=resize(raw,w,h)
      alpha=raw[:,:,3].copy()
      if min(w,h)>=grid*4:
        for z in range(grid):
          for x in range(grid):
            tile=alpha[z*h//grid:(z+1)*h//grid,x*w//grid:(x+1)*w//grid]
            target=targets[z*grid+x]; lo,hi=.5,3.
            for _ in range(12):
              mid=(lo+hi)/2
              if np.mean(tile*mid>=cutoff)<target:lo=mid
              else:hi=mid
            tile*=min(2.,(lo+hi)/2)
      rgb=raw[:,:,:3]/np.maximum(raw[:,:,3:4],1e-6)
      rgb=np.where(rgb<=.0031308,rgb*12.92,1.055*np.maximum(rgb,0.)**(1/2.4)-.055)
      rgba=np.concatenate((rgb,alpha[:,:,None]),axis=2)
      payload+=np.rint(np.clip(rgba,0,1)*255).astype(np.uint8).tobytes()
    header=struct.pack('<4sIIIQ',b'BVM2',width,height,levels,fingerprint(path.read_bytes()))
    path.with_suffix('.mips').write_bytes(header+payload)
    print(name,levels,len(payload),flush=True)
