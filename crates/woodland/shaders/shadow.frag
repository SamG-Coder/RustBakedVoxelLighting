#version 460
#extension GL_GOOGLE_include_directive : require
#include "common.glsl"
layout(set=0,binding=1) uniform sampler2D images[64];
layout(location=0) in vec3 world;
layout(location=2) in vec2 texcoord;
layout(location=5) in vec2 fade;
layout(location=7) flat in int grass_tile;
layout(location=8) flat in int grass_lod;
void main(){
 Material m=materials.items[draw.material];
 if(int(m.settings.x)==0 || int(m.settings.x)==4)return;
 vec2 uv=texcoord;
 if(int(m.settings.x)==2){
  if(grass_lod<0)discard;
  uv=(vec2(grass_tile%4,grass_tile/4)*1280.+13.+texcoord*1254.)/5120.;
  float amount=fade.x*(1.-smoothstep(48.,64.,distance(world.xz,globals.camera_time.xz)));
  float hash=fract(sin(dot(floor(texcoord*128.),vec2(12.9898,78.233)))*43758.5453);
  if(hash>amount)discard;
 }
 if(m.settings.x<3.5 && texture(images[m.textures.x],uv).a<m.settings.y)discard;
}
