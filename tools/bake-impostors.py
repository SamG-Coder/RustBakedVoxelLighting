"""Offline neutral-albedo captures of the exported source meshes, not replacement art.

CPU rasterization keeps this migration tool reproducible without a browser. The
native renderer relights the resulting alpha-cutout cards with its live lights.
"""
import json
from pathlib import Path
import numpy as np
from PIL import Image

ROOT = Path(__file__).resolve().parents[1] / 'assets'
W, H = 256, 512

def mesh(path):
    raw = path.read_bytes()
    assert raw[:4] == b'BVM1'
    n, m = np.frombuffer(raw, '<u4', 2, 4)
    vertices = np.frombuffer(raw, '<f4', int(n)*12, 12).reshape(-1, 12)
    indices = np.frombuffer(raw, '<u4', int(m), 12+int(n)*48).reshape(-1, 3)
    return vertices, indices

def capture(parts, axis, bounds):
    width, bottom, height = bounds
    output = np.zeros((H, W, 4), np.float32)
    normals = np.full((H,W,4), [0.5,1.,0.5,0.], np.float32)
    depth = np.full((H, W), -np.inf, np.float32)
    for vertices, indices, texture in parts:
        pos = vertices[:, :3]
        x = pos[:, axis] / width * (W-8) + W/2
        y = (1-(pos[:, 1]-bottom)/height) * (H-8) + 4
        z = pos[:, 2 if axis == 0 else 0]
        for tri in indices:
            px, py, pz = x[tri], y[tri], z[tri]
            x0,x1=max(0,int(np.floor(px.min()))),min(W-1,int(np.ceil(px.max())))
            y0,y1=max(0,int(np.floor(py.min()))),min(H-1,int(np.ceil(py.max())))
            if x1<x0 or y1<y0: continue
            denom=(py[1]-py[2])*(px[0]-px[2])+(px[2]-px[1])*(py[0]-py[2])
            if abs(denom)<1e-8: continue
            xx,yy=np.meshgrid(np.arange(x0,x1+1)+.5,np.arange(y0,y1+1)+.5)
            a=((py[1]-py[2])*(xx-px[2])+(px[2]-px[1])*(yy-py[2]))/denom
            b=((py[2]-py[0])*(xx-px[2])+(px[0]-px[2])*(yy-py[2]))/denom
            c=1-a-b
            zz=a*pz[0]+b*pz[1]+c*pz[2]
            old=depth[y0:y1+1,x0:x1+1]
            inside=(a>=0)&(b>=0)&(c>=0)&(zz>old)
            if not inside.any(): continue
            weights=np.stack((a,b,c),axis=-1)
            uv=weights@vertices[tri,6:8]
            tx=((uv[...,0]%1)*texture.shape[1]).astype(int)%texture.shape[1]
            ty=((uv[...,1]%1)*texture.shape[0]).astype(int)%texture.shape[0]
            sample=texture[ty,tx].astype(np.float32)/255
            inside &= sample[...,3]>.4
            colour=weights@vertices[tri,8:11]
            # Vertex colours are linear; texture is sRGB. Keep the capture unlit.
            sample[...,:3]=(sample[...,:3]**2.2 * colour)**(1/2.2)
            sample[...,3]=1
            output[y0:y1+1,x0:x1+1][inside]=sample[inside]
            n = weights @ vertices[tri,3:6]
            n /= np.maximum(np.linalg.norm(n,axis=-1,keepdims=True), 1e-6)
            normals[y0:y1+1,x0:x1+1,:3][inside] = n[inside]*.5+.5
            normals[y0:y1+1,x0:x1+1,3][inside] = 1
            old[inside]=zz[inside]
    return np.clip(output*255,0,255).astype(np.uint8), np.clip(normals*255,0,255).astype(np.uint8)

manifest=json.loads((ROOT/'world/manifest.json').read_text())
records=[]
for i,v in enumerate(manifest['variants']):
    print(f'Baking neutral impostor {i+1}/15',flush=True)
    parts=[]
    for part,tex in [('wood',v['bark']),('leaf',v['leaf'])]:
        vertices,indices=mesh(ROOT/'world'/v['levels'][0][part])
        texture=np.array(Image.open(ROOT/'textures'/f'{tex}.png').convert('RGBA'))
        parts.append((vertices,indices,texture))
    positions=np.concatenate([p[0][:,:3] for p in parts])
    radius=float(np.linalg.norm(positions[:,[0,2]],axis=1).max())
    bottom=float(positions[:,1].min()); height=float(positions[:,1].max())-bottom
    bounds=(radius*2.08,bottom-.04,height+.08)
    views=[capture(parts,axis,bounds) for axis in (0,2)]
    atlas=np.concatenate([view[0] for view in views],axis=1)
    normal_atlas=np.concatenate([view[1] for view in views],axis=1)
    Image.fromarray(normal_atlas).save(ROOT/"textures"/f"impostor-normal-{i}.png")
    Image.fromarray(atlas).save(ROOT/'textures'/f'impostor-{i}.png')
    records.append({'width':bounds[0], 'bottom':bounds[1], 'height':bounds[2]})
(ROOT/'world/impostors.json').write_text(json.dumps(records))

# Pack every prototype into one shared material atlas.
combined = Image.new("RGBA", (2048, 2048))
for i in range(len(records)):
    with Image.open(ROOT / "textures" / f"impostor-{i}.png") as tile:
        combined.paste(tile, ((i % 4) * 512, (i // 4) * 512))
combined.save(ROOT / "textures" / "impostor-atlas.png")

normal_combined = Image.new("RGBA", (2048,2048), (128,255,128,0))
for i in range(len(records)):
    with Image.open(ROOT / "textures" / f"impostor-normal-{i}.png") as tile:
        normal_combined.paste(tile,((i%4)*512,(i//4)*512))
normal_combined.save(ROOT / "textures" / "impostor-normal-atlas.png")
