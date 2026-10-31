#version 460
#extension GL_GOOGLE_include_directive : require
#include "common.glsl"
layout(constant_id=0) const int MATERIAL_KIND=-1;
layout(set=0,binding=1) uniform sampler2D images[64];
layout(location=0) in vec3 world;
layout(location=2) in vec2 texcoord;
layout(location=5) in vec2 fade;
layout(location=7) flat in int grass_tile;
layout(location=8) flat in int grass_lod;
layout(location=10) flat in vec2 grass_shadow;
// Integer hashing avoids sin-based threshold instability and stays attached to
// the card UVs rather than the camera or the wind-deformed world position.
float shadowNoise(vec2 uv){
 uvec2 p=uvec2(clamp(uv,0.,1.)*64.);
 uint h=p.x*1664525u+p.y*1013904223u;
 h^=h>>16;h*=2246822519u;h^=h>>13;
 return float(h&65535u)/65536.;
}
void main(){
 Material m=materials.items[draw.material];
 int kind=MATERIAL_KIND>=0?MATERIAL_KIND:int(m.settings.x);
 if(kind==0 || kind==4)return;
 vec2 uv=texcoord;
 if(kind==2){
  if(grass_lod<0)discard;
  uv=(vec2(grass_tile%4,grass_tile/4)*1280.+13.+texcoord*1254.)/5120.;
  if(grass_shadow.x>=grass_shadow.y)discard;
 }
 if(kind==1){
  float threshold=shadowNoise(texcoord);
  if(fade.y>.5 ? threshold<1.-fade.x : threshold>=fade.x)discard;
  // Stable material-space porosity; PCF integrates partial tissue transmission.
  // Keep this independent from LOD fade so wood/terrain remain fully opaque.
  if(shadowNoise(fract(texcoord*vec2(1.73,2.17)+vec2(.31,.19)))>.70)discard;
 }
 if(kind<4){
  float alpha=texture(images[m.textures.x],uv).a;
  if(kind==1){
   float lastMip=floor(log2(float(max(textureSize(images[m.textures.x],0).x,textureSize(images[m.textures.x],0).y))));
   float mip=textureQueryLod(images[m.textures.x],uv).x;
   float tail=smoothstep(lastMip-2.,lastMip,mip);
   float coverage=mix(step(m.settings.y,alpha),alpha,tail);
   if(shadowNoise(fract(texcoord*vec2(2.31,1.41)+vec2(.13,.47)))>=coverage)discard;
  }else if(alpha<m.settings.y)discard;
 }
}
