from PIL import Image,ImageDraw,ImageFont
from pathlib import Path
import subprocess
root=Path(__file__).resolve().parents[1]; out=root/'evidence';out.mkdir(exist_ok=True)
font='C:/Windows/Fonts/segoeui.ttf';bold='C:/Windows/Fonts/seguisb.ttf'
def text(d,xy,s,size=24,color='#e9eee4',weight=False):d.text(xy,s,font=ImageFont.truetype(bold if weight else font,size),fill=color)
im=Image.new('RGB',(1920,1080),'#0d1815');d=ImageDraw.Draw(im)
d.rectangle((56,48,88,52),fill='#d7e6a4');text(d,(104,32),'RUST BAKED VOXEL LIGHTING',24,'#c5d49b',True)
text(d,(56,77),'One world. Two runtimes.',62,weight=True)
text(d,(58,178),'RUST IN THE BROWSER',28,weight=True);text(d,(1018,178),'NATIVE RUST',28,weight=True)
text(d,(58,216),'WebAssembly / WebGPU',20,'#abbcb2');text(d,(1018,216),'Vulkan / RTX reflections + probe lighting',20,'#abbcb2')
d.line((960,262,960,802),fill='#d7e6a4',width=2)
text(d,(56,836),'WOODLAND  /  RIVER  /  CANOPY  /  EVENING LIGHT',22,'#d7e6a4',True)
text(d,(56,886),'Same world data. Same camera route. Same sun clock.',32,weight=True)
text(d,(56,937),'Live captures - no colour matching. Browser fallbacks are still being refined.',22,'#abbcb2')
d.line((56,990,1864,990),fill='#304139',width=1)
text(d,(56,1010),'samg-coder.github.io/RustBakedVoxelLighting',22,'#c5d49b')
text(d,(1580,1010),'SamG  /  Open source',22,'#abbcb2')
im.save(out/'comparison-layout.png')
master=Image.new('RGB',(3200,1020),'#0d1815');m=ImageDraw.Draw(master)
text(m,(28,9),'BROWSER  /  RUST + WEBGPU',28,weight=True);text(m,(1628,9),'NATIVE  /  RUST + VULKAN',28,weight=True)
text(m,(28,975),'RustBakedVoxelLighting  -  same world / camera / sun clock  -  recorded live',24,'#c5d49b')
master.save(out/'comparison-master-layout.png')
cmd=['ffmpeg','-hide_banner','-y','-i',str(out/'browser-live.webm'),'-ss','29.933333','-i',str(out/'native-live.mp4'),'-framerate','60','-loop','1','-i',str(out/'comparison-layout.png'),'-filter_complex','[0:v]fps=60,scale=960:540:flags=lanczos,setpts=PTS-STARTPTS[b];[1:v]fps=60,scale=960:540:flags=lanczos,setpts=PTS-STARTPTS[n];[2:v][b]overlay=0:262[tmp];[tmp][n]overlay=960:262,drawbox=x=959:y=262:w=2:h=540:color=0xd7e6a4:t=fill[v]','-map','[v]','-t','119.8','-an','-c:v','h264_nvenc','-preset','p6','-rc','vbr','-cq','20','-b:v','10M','-maxrate','16M','-bufsize','32M','-pix_fmt','yuv420p','-movflags','+faststart',str(out/'Rust-Browser-vs-Native-1080p.mp4')]
with open(out/'comparison-encode.log','w') as log:subprocess.run(cmd,stdout=log,stderr=log,check=True)
cmd=['ffmpeg','-hide_banner','-y','-i',str(out/'browser-live.webm'),'-ss','29.933333','-i',str(out/'native-live.mp4'),'-framerate','60','-loop','1','-i',str(out/'comparison-master-layout.png'),'-filter_complex','[0:v]fps=60,setpts=PTS-STARTPTS[b];[1:v]fps=60,setpts=PTS-STARTPTS[n];[2:v][b]overlay=0:60[tmp];[tmp][n]overlay=1600:60[v]','-map','[v]','-t','119.8','-an','-c:v','h264_nvenc','-preset','p6','-cq','19','-pix_fmt','yuv420p','-movflags','+faststart',str(out/'Rust-Browser-vs-Native-Master.mp4')]
with open(out/'comparison-master-encode.log','w') as log:subprocess.run(cmd,stdout=log,stderr=log,check=True)
print('Both comparison exports completed.')
