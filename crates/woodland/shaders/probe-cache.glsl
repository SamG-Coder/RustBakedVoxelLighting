// 208 bytes per probe. Camera-centred toroidal storage retains world-space history.
struct CachedProbe {ivec4 cell;vec4 c0;vec4 cx;vec4 cy;vec4 cz;vec4 moments[6];vec4 state;vec4 light;};
const ivec3 GI_GRID=ivec3(24,12,24);
const float GI_STEP=8.;
#ifdef PROBE_COMPUTE
layout(std430,set=0,binding=13) buffer ProbeCache {CachedProbe records[];} gi;
#else
layout(std430,set=0,binding=13) readonly buffer ProbeCache {CachedProbe records[];} gi;
#endif
int probeAddress(ivec3 cell){ivec3 q=((cell%GI_GRID)+GI_GRID)%GI_GRID;return (q.z*GI_GRID.y+q.y)*GI_GRID.x+q.x;}
bool currentProbe(CachedProbe p,ivec3 cell){
 return p.cell.w==1 && all(equal(p.cell.xyz,cell)) && dot(p.light.xyz,globals.sun.xyz)>.985 && abs(p.light.w-globals.sun.w)<.25;
}
#ifndef PROBE_COMPUTE
vec3 probeDiffuse(vec3 p,vec3 n,out float confidence){
 confidence=0.;
 if(globals.weather.w<.5)return vec3(0.);
 vec3 relative=abs(p-globals.camera_time.xyz);
 float boundary=(1.-smoothstep(64.,84.,max(relative.x,relative.z)))*(1.-smoothstep(28.,40.,relative.y));
 if(boundary<=0.)return vec3(0.);
 vec3 coord=(p+n*.3)/GI_STEP-.5;ivec3 base=ivec3(floor(coord));vec3 fraction=fract(coord);
 vec3 result=vec3(0.);float total=0.,maturity=0.;
 for(int z=0;z<2;z++)for(int y=0;y<2;y++)for(int x=0;x<2;x++){
   ivec3 cell=base+ivec3(x,y,z);CachedProbe probe=gi.records[probeAddress(cell)];
   if(!currentProbe(probe,cell))continue;
   vec3 toPoint=p-(vec3(cell)+.5)*GI_STEP;float d=length(toPoint);
   vec3 direction=toPoint/max(d,.001),absolute=abs(direction);
   int axis=absolute.x>absolute.y && absolute.x>absolute.z?(direction.x>0.?0:1):absolute.y>absolute.z?(direction.y>0.?2:3):(direction.z>0.?4:5);
   vec2 moments=probe.moments[axis].xy;
   float variance=max(moments.y-moments.x*moments.x,.04);
   float delta=max(0.,d-moments.x);
   float visibility=pow(variance/(variance+delta*delta),3.);
   float facing=pow(clamp(dot(-direction,n)*.5+.5,0.,1.),2.)+.02;
   float weight=(x==0?1.-fraction.x:fraction.x)*(y==0?1.-fraction.y:fraction.y)*(z==0?1.-fraction.z:fraction.z)*visibility*facing;
   result+=max(vec3(0.),probe.c0.rgb+probe.cx.rgb*n.x+probe.cy.rgb*n.y+probe.cz.rgb*n.z)*weight;
   maturity+=clamp((globals.camera_time.w-probe.state.w)/.8,0.,1.)*weight;
   total+=weight;
 }
 if(total<.0001)return vec3(0.);
 confidence=boundary*clamp(total*10.,0.,1.)*maturity/total;
 return result/total;
}
#endif
