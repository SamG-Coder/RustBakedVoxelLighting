import cv2,subprocess,numpy as np,json
from pathlib import Path
root=Path(__file__).resolve().parents[1]
def frame(file,t):
 b=subprocess.check_output(['ffmpeg','-v','error','-ss',str(t),'-i',str(root/'evidence'/file),'-frames:v','1','-vf','scale=800:450','-f','rawvideo','-pix_fmt','gray','-'])
 return np.frombuffer(b,np.uint8).reshape(450,800)
sift=cv2.SIFT_create(nfeatures=1800); matcher=cv2.BFMatcher()
results=[]
for t in [12,55,95]:
 a,da=sift.detectAndCompute(frame('browser-live.webm',t),None)
 scores=[]
 for offset in np.arange(29.0,30.21,.05):
  b,db=sift.detectAndCompute(frame('native-live.mp4',t+offset),None)
  matches=matcher.knnMatch(da,db,k=2)
  pairs=[(a[m.queryIdx].pt,b[m.trainIdx].pt) for m,n in matches if m.distance<.65*n.distance]
  dist=np.linalg.norm(np.array([x for x,y in pairs])-np.array([y for x,y in pairs]),axis=1)
  score=float(np.median(dist));scores.append((score,float(offset),len(pairs)))
 best=min(scores);results.append({'browser_seconds':t,'best':best});print(results[-1],flush=True)
(root/'evidence/alignment.json').write_text(json.dumps(results,indent=2))
