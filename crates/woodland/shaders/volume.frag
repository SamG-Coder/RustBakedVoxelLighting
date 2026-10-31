#version 460
#extension GL_GOOGLE_include_directive : require
#include "common.glsl"
layout(set=0,binding=1) uniform sampler2D images[64];
layout(set=0,binding=3) uniform sampler2DShadow shadow_map;
layout(set=0,binding=6) uniform sampler2DMS scene_depth;
#include "lighting.glsl"
#include "atmosphere.glsl"
#define VOLUME_SHADOW
#include "shadow-sampling.glsl"
layout(location=0) in vec2 uv;
layout(location=0) out vec4 volume;

void main(){
 ivec2 size=textureSize(scene_depth);
 ivec2 pixel=clamp(ivec2(uv*vec2(size)),ivec2(0),size-1);
 float depth=texelFetch(scene_depth,pixel,0).r;
 vec4 end=globals.inverse_view_projection*vec4(uv*2.-1.,min(depth,.999999),1.);
 vec3 delta=end.xyz/end.w-globals.camera_time.xyz;
 float lengthToSurface=length(delta),distanceLimit=min(lengthToSurface,128.);
 vec3 ray=delta/max(lengthToSurface,.001),light=normalize(globals.sun.xyz);
 const float g=.40;
 float phase=(1.-g*g)/(12.5663706*pow(1.+g*g-2.*g*dot(ray,light),1.5));
 vec3 source=globals.sun_colour.rgb*globals.sun.w*phase*cloudTransmission(globals.camera_time.xyz,light);
 // Fixed spatial strata: no animated sample noise or temporal-history ghosting.
 float jitter=fract(52.9829189*fract(dot(gl_FragCoord.xy,vec2(.06711056,.00583715))));
 float stepSize=3.,transmission=1.;vec3 scattering=vec3(0.);
 for(int i=0;i<43;i++){
   float segmentLength=clamp(distanceLimit-float(i)*stepSize,0.,stepSize);
   if(segmentLength<=0.)break;
   vec3 p=globals.camera_time.xyz+ray*(float(i)*stepSize+jitter*segmentLength);
   float height=terrainHeight(p.xz),above=p.y-height;
   float coarseBlend;
   float lit=cascadedShadow(p,.00008,coarseBlend);
   if(coarseBlend>0.){
     int n=int(globals.environment.z),layers=int(globals.environment.w);
     ivec3 c=clamp(ivec3((p.x+2000.)/4000.*float(n),above/64.*float(layers),(p.z+2000.)/4000.*float(n)),ivec3(0),ivec3(n-1,layers-1,n-1));
     Probe probe=baked.probes[(c.z*layers+c.y)*n+c.x];
     float coarse=mix(sunTransport(probe,light),1.,smoothstep(54.,64.,above));
     lit=mix(lit,coarse,coarseBlend);
   }
   // Thin local aerosols. Keep the distant landscape clear rather than hiding it in fog.
   float sigma=mix(.00065,.0012,globals.weather.y)*exp(-max(above,0.)*.045)*smoothstep(-.4,.5,above);
   float segment=exp(-sigma*segmentLength);
   scattering+=transmission*(1.-segment)*(source*lit+vec3(.025,.033,.045)*globals.sky.x);
   transmission*=segment;
 }
 volume=vec4(scattering,transmission);
}
