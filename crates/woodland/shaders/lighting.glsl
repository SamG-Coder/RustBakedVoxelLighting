// Static voxel transport is relit by the current sun/moon, so no daytime radiance is cached.
struct Probe {vec4 bounce_sky;vec4 side;vec4 bounce_x;vec4 bounce_y;vec4 bounce_z;};
layout(std430,set=0,binding=4) readonly buffer Transport {Probe probes[];} baked;
float terrainHeight(vec2 p){
 vec2 uv=clamp((p+2000.)/4000.,vec2(.5/globals.environment.z),vec2(1.-.5/globals.environment.z));
 vec4 h=textureLod(images[uint(globals.environment.y+.5)],uv,0.);
 return (h.r*65280.+h.g*255.)/65535.*1024.-128.;
}
Probe transportAt(vec3 p){
 int size=int(globals.environment.z),layers=int(globals.environment.w);
 vec3 coordinate=vec3((p.x+2000.)/4000.*float(size)-.5,(p.y-terrainHeight(p.xz))/64.*float(layers)-.5,(p.z+2000.)/4000.*float(size)-.5);
 coordinate=clamp(coordinate,vec3(0.),vec3(size-1,layers-1,size-1));
 ivec3 cell=ivec3(floor(coordinate));vec3 f=fract(coordinate);
 Probe result;result.bounce_sky=vec4(0.);result.side=vec4(0.);
 result.bounce_x=vec4(0.);result.bounce_y=vec4(0.);result.bounce_z=vec4(0.);
 for(int z=0;z<2;z++)for(int y=0;y<2;y++)for(int x=0;x<2;x++){
  ivec3 v=min(cell+ivec3(x,y,z),ivec3(size-1,layers-1,size-1));
  float w=(x==0?1.-f.x:f.x)*(y==0?1.-f.y:f.y)*(z==0?1.-f.z:f.z);
  Probe a=baked.probes[(v.z*layers+v.y)*size+v.x];
  result.bounce_sky+=a.bounce_sky*w;result.side+=a.side*w;
  result.bounce_x+=a.bounce_x*w;result.bounce_y+=a.bounce_y*w;result.bounce_z+=a.bounce_z*w;
 }
 // Above the canopy, smoothly return to open sky rather than clamp to a dark top slice.
 float high=smoothstep(54.,64.,p.y-terrainHeight(p.xz));
 result.bounce_sky=mix(result.bounce_sky,vec4(0.,0.,0.,1.),high);
 result.side=mix(result.side,vec4(1.),high);
 result.bounce_x*=1.-high;result.bounce_y*=1.-high;result.bounce_z*=1.-high;
 return result;
}
vec3 sunlightBounce(Probe p,vec3 light){
 // First-order cosine transfer from surrounding surfaces, baked once and relit live.
 return max(vec3(0.),p.bounce_sky.rgb*.25+p.bounce_x.rgb*light.x+p.bounce_y.rgb*light.y+p.bounce_z.rgb*light.z);
}
float sunTransport(Probe p,vec3 light){
 vec4 w=max(vec4(light.x,-light.x,light.z,-light.z),0.);
 float side=dot(p.side,w)/max(dot(w,vec4(1.)),.001);
 return mix(side,p.bounce_sky.a,smoothstep(.3,.95,light.y));
}
