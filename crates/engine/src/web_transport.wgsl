
// Port of woodland/shaders/transport.comp. Stores transfer, not time-of-day radiance.
struct Probe {bounce_sky:vec4f,side:vec4f,bounce_x:vec4f,bounce_y:vec4f,bounce_z:vec4f}
struct Bake {size:u32,layers:u32,slice:u32,pad:u32}
@group(0) @binding(0) var occupancy:texture_2d<f32>;
@group(0) @binding(1) var heights:texture_2d<f32>;
@group(0) @binding(2) var smp:sampler;
@group(0) @binding(3) var<storage,read_write> probes:array<Probe>;
@group(0) @binding(4) var<uniform> info:Bake;
fn height_at(p:vec2f)->f32 {let uv=clamp((p+2000.)/4000.,vec2f(.5/f32(info.size)),vec2f(1.-.5/f32(info.size)));let h=textureSampleLevel(heights,smp,uv,0.);return (h.r*65280.+h.g*255.)/65535.*1024.-128.;}
fn occupancy_at(p:vec3f)->vec4f {let cell=vec2i(floor((p.xz+2000.)/4000.*f32(info.size)));if(any(cell<vec2i(0))||any(cell>=vec2i(i32(info.size)))){return vec4f(0.);}let h=height_at(p.xz);if(p.y<h+.15){return vec4f(.12,.10,.065,1.);}let y=i32(floor((p.y-h)/64.*f32(info.layers)));if(y>=i32(info.layers)){return vec4f(0.);}return textureLoad(occupancy,vec2i(cell.x,cell.y*i32(info.layers)+max(y,0)),0);}
@compute @workgroup_size(8,8,1) fn bake(@builtin(global_invocation_id) id:vec3u){
 let c=vec3u(id.xy,info.slice);if(c.x>=info.size||c.y>=info.size||c.z>=info.layers){return;}
 let xz=(vec2f(c.xy)+.5)/f32(info.size)*4000.-2000.;let p=vec3f(xz.x,height_at(xz)+(f32(c.z)+.5)*64./f32(info.layers),xz.y);
 var bounce=vec3f(0.);var bx=vec3f(0.);var by=vec3f(0.);var bz=vec3f(0.);var sky=0.;var side=vec4f(0.);
 for(var ray=0;ray<12;ray++){let angle=f32(ray)*2.39996323;let up=.12+.82*(f32(ray)+.5)/12.;let dir=vec3f(cos(angle)*sqrt(1.-up*up),up,sin(angle)*sqrt(1.-up*up));var trans=1.;for(var step=0;step<16;step++){let d=3.+f32(step)*4.+f32(step*step)*.35;let hit=occupancy_at(p+dir*d);trans*=1.-hit.a;if(trans<.025){break;}}sky+=trans/12.;}
 for(var ray=0;ray<16;ray++){let y=1.-2.*(f32(ray)+.5)/16.;let angle=f32(ray)*2.39996323;let dir=vec3f(cos(angle)*sqrt(1.-y*y),y,sin(angle)*sqrt(1.-y*y));var trans=1.;
 for(var step=0;step<10;step++){let q=p+dir*(2.+f32(step)*3.+f32(step*step)*.45);let hit=occupancy_at(q);let weight=trans*hit.a;
 if(weight>.025){let ground=height_at(q.xz);let soil=q.y<ground+.15;var normal=normalize(vec3f(0.,.7,0.)-dir*.6);if(soil){normal=normalize(vec3f(height_at(q.xz-vec2f(4,0))-height_at(q.xz+vec2f(4,0)),8.,height_at(q.xz-vec2f(0,4))-height_at(q.xz+vec2f(0,4))));}let surface=vec3f(q.x,max(q.y,ground+.7),q.z);var openness=1.;for(var s=0;s<6;s++){openness*=1.-occupancy_at(surface+vec3f(0.,3.+f32(s)*5.,0.)).a;}let transfer=hit.rgb*(weight/16.)*mix(.15,1.,openness);bounce+=transfer;bx+=transfer*normal.x*.5;by+=transfer*normal.y*.5;bz+=transfer*normal.z*.5;
 }trans*=1.-hit.a;if(trans<.025){break;}}
 }
 for(var axis=0;axis<4;axis++){let dir=vec3f(select(select(0.,-1.,axis==1),1.,axis==0),.55,select(select(0.,-1.,axis==3),1.,axis==2));var trans=1.;for(var step=0;step<20;step++){let hit=occupancy_at(p+normalize(dir)*(4.+f32(step)*6.));trans*=1.-hit.a;if(trans<.025){break;}}side[axis]=trans;}
 let index=(c.y*info.layers+c.z)*info.size+c.x;probes[index]=Probe(vec4f(bounce,sky),side,vec4f(bx,0.),vec4f(by,0.),vec4f(bz,0.));
}
