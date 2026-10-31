"""Periodic, unlit river slope/detail texture; mirrors the source river-noise bake."""
from pathlib import Path
import numpy as np
from PIL import Image
n=512
u,v=np.meshgrid(np.arange(n)/n,np.arange(n)/n)
def noise(x,y,grid):
 x=x*grid;y=y*grid;ix=np.floor(x);iy=np.floor(y);fx=x-ix;fy=y-iy
 fx=fx*fx*(3-2*fx);fy=fy*fy*(3-2*fy)
 def h(a,b):
  t=np.sin(np.mod(a,grid)*127.1+np.mod(b,grid)*311.7)*43758.5453
  return t-np.floor(t)
 return (h(ix,iy)*(1-fx)+h(ix+1,iy)*fx)*(1-fy)+(h(ix,iy+1)*(1-fx)+h(ix+1,iy+1)*fx)*fy
def height(x,y):return noise(x,y,8)*.52+noise(x+.23,y+.17,16)*.28+noise(x,y,32)*.14+noise(x,y,64)*.06
e=1/256
result=np.stack([.5+(height(u+e,v)-height(u-e,v))*3,.5+(height(u,v+e)-height(u,v-e))*3,noise(u,v,32),noise(u,v,8)],axis=-1)
Image.fromarray(np.uint8(np.clip(result,0,1)*255)).save(Path('assets/textures/river-normal.png'))
