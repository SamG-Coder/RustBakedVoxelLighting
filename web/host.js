// Browser API glue only. World, movement, LOD, grass and rendering execute in Rust/WASM.
import init, { BrowserGame } from './pkg/woodland_browser.js';
// Native browser decompression keeps downloaded packs small without custom server headers.
window.woodlandAssetFetch=async path=>{
 const compressed=path.endsWith('.bin');
 const response=await fetch(compressed?`${path}.gz`:path);
 if(!response.ok)throw new Error(`${path}: HTTP ${response.status}`);
 return compressed?new Response(response.body.pipeThrough(new DecompressionStream('gzip'))):response;
};
const canvas=document.querySelector('canvas'),menu=document.querySelector('#menu'),status=document.querySelector('#status'),hud=document.querySelector('#hud');
let game,mode=0,keys=0,drag=false,frameCount=0,lastStat=0,cpuTime=0;
const errors=[];window.__woodland={errors,ready:false};
function fail(error){console.error(error);errors.push(String(error));document.querySelector('#error').textContent=String(error);menu.classList.remove('hidden');}
window.addEventListener('unhandledrejection',e=>fail(e.reason));
let tour=null,recorder=null,recordingChunks=[];
function size(){if(tour){canvas.width=1600;canvas.height=900;return;}const scale=Math.min(devicePixelRatio,1.5);const w=Math.max(1,Math.round(innerWidth*scale)),h=Math.max(1,Math.round(innerHeight*scale));if(canvas.width!==w||canvas.height!==h){canvas.width=w;canvas.height=h;}}
function enter(value){mode=value;game.mode(value);menu.classList.add('hidden');canvas.requestPointerLock?.()?.catch(()=>{document.querySelector('#help').textContent='Drag to look - WASD - Esc menu';});}
function pause(){if(tour)return;mode=0;keys=0;game?.keys(0);game?.mode(0);menu.classList.remove('hidden');}
document.querySelector('#walk').onclick=()=>enter(1);document.querySelector('#fly').onclick=()=>enter(2);
document.querySelector('#hour').oninput=e=>game?.time_of_day(Number(e.target.value));
const bits={KeyW:1,KeyS:2,KeyA:4,KeyD:8,Space:16,ControlLeft:32,ShiftLeft:64};
addEventListener('keydown',e=>{if(e.code==='Escape'){pause();document.exitPointerLock?.();}if(mode&&bits[e.code]){e.preventDefault();keys|=bits[e.code];game.keys(keys);}});
addEventListener('keyup',e=>{if(bits[e.code]){keys&=~bits[e.code];game?.keys(keys);}});addEventListener('blur',()=>{keys=0;game?.keys(0);drag=false;});
canvas.onpointerdown=()=>{drag=true;};addEventListener('pointerup',()=>drag=false);addEventListener('mousemove',e=>{if(mode&&(document.pointerLockElement===canvas||drag))game.look(e.movementX,e.movementY);});
document.addEventListener('pointerlockchange',()=>{if(!document.pointerLockElement&&mode)pause();});
function frame(ms){try{if(document.hidden){requestAnimationFrame(frame);return;}size();if(tour){const seconds=(ms-tour.start)/1000-30;game.showcase(seconds);document.querySelector('#help').textContent=seconds<0?`Preparing route: ${Math.ceil(-seconds)}s`:`Browser / Rust / WebGPU / ${Math.min(120,seconds).toFixed(1)} / 120s`;
if(seconds>=0&&!recorder){recordingChunks=[];const mime=['video/webm;codecs=vp9','video/webm;codecs=vp8'].find(t=>MediaRecorder.isTypeSupported(t));recorder=new MediaRecorder(canvas.captureStream(60),{mimeType:mime,videoBitsPerSecond:20000000});recorder.ondataavailable=e=>{if(e.data.size)recordingChunks.push(e.data);};recorder.onstop=()=>{const a=document.querySelector('#download');a.href=URL.createObjectURL(new Blob(recordingChunks,{type:mime}));a.classList.remove('hidden');a.textContent='Download recording';menu.classList.remove('hidden');};recorder.start(1000);}
if(seconds>=120&&recorder.state==='recording'){recorder.stop();tour=null;game.showcase(-1000);}}
const begin=performance.now();game.frame(ms/1000,canvas.width,canvas.height);cpuTime+=performance.now()-begin;frameCount++;if(ms-lastStat>1000){const fps=Math.round(frameCount*1000/(ms-lastStat));hud.textContent=`${fps} FPS - ${(cpuTime/frameCount).toFixed(1)} ms CPU - ${game.stats()}`;window.__woodland.stats=hud.textContent;frameCount=0;cpuTime=0;lastStat=ms;}requestAnimationFrame(frame);}catch(e){fail(e);}}
try{if(!navigator.gpu)throw new Error('WebGPU is required. Use a WebGPU-enabled desktop browser on HTTPS or localhost.');const adapter=await navigator.gpu.requestAdapter();console.info("WebGPU adapter",JSON.stringify({vendor:adapter.info.vendor,architecture:adapter.info.architecture,device:adapter.info.device,description:adapter.info.description}));status.title=JSON.stringify({vendor:adapter.info.vendor,architecture:adapter.info.architecture,device:adapter.info.device,description:adapter.info.description});size();await init();game=await BrowserGame.create(canvas,message=>status.textContent=message);window.__woodland.ready=true;window.__woodland.game=game;document.querySelectorAll('button').forEach(b=>b.disabled=false);requestAnimationFrame(frame);}catch(e){fail(e);}

document.querySelector('#showcase').onclick=()=>{tour={start:performance.now()};recorder=null;mode=0;game.mode(0);menu.classList.add('hidden');};
