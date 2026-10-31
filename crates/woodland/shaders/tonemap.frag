#version 460
#extension GL_GOOGLE_include_directive : require
#include "common.glsl"
layout(set=0,binding=11) uniform sampler2D hdr_scene;
layout(set=0,binding=12) uniform sampler2D aerial_volume;
layout(set=0,binding=6) uniform sampler2DMS scene_depth;
layout(location=0) in vec2 uv;
layout(location=0) out vec4 color;
// Three.js ACESFilmicToneMapping (MIT), including colour-space matrices.
// See THIRD_PARTY_NOTICES.md. Exposure is supplied by the caller.
vec3 aces(vec3 c){
    const mat3 inputMatrix=mat3(vec3(.59719,.07600,.02840),vec3(.35458,.90834,.13383),vec3(.04823,.01566,.83777));
    const mat3 outputMatrix=mat3(vec3(1.60475,-.10208,-.00327),vec3(-.53108,1.10813,-.07276),vec3(-.07367,-.00605,1.07602));
    c=inputMatrix*(c/.6);
    c=(c*(c+.0245786)-.000090537)/(c*(.983729*c+.4329510)+.238081);
    return clamp(outputMatrix*c,0.,1.);
}

float surfaceDistance(vec2 coord){
    ivec2 size=textureSize(scene_depth);
    float depth=texelFetch(scene_depth,clamp(ivec2(coord*vec2(size)),ivec2(0),size-1),0).r;
    vec4 p=globals.inverse_view_projection*vec4(coord*2.-1.,min(depth,.999999),1.);
    return length(p.xyz/p.w-globals.camera_time.xyz);
}
vec4 upsampleVolume(){
    ivec2 size=textureSize(aerial_volume,0);vec2 p=uv*vec2(size)-.5,f=fract(p);
    ivec2 base=ivec2(floor(p));float distanceHere=surfaceDistance(uv),total=0.;vec4 sum=vec4(0.);
    for(int y=0;y<2;y++)for(int x=0;x<2;x++){
        ivec2 cell=clamp(base+ivec2(x,y),ivec2(0),size-1);
        vec2 coord=(vec2(cell)+.5)/vec2(size);
        float difference=abs(surfaceDistance(coord)-distanceHere);
        float weight=max(1e-5,exp(-difference/max(.5,distanceHere*.025))*(x==0?1.-f.x:f.x)*(y==0?1.-f.y:f.y));
        sum+=texelFetch(aerial_volume,cell,0)*weight;total+=weight;
    }
    return sum/total;
}
void main(){
    vec3 hdr=texture(hdr_scene,clamp(uv,vec2(0.),vec2(1.))).rgb;
    vec4 volume=upsampleVolume();hdr=hdr*volume.a+volume.rgb;
    color=vec4(aces(max(hdr,vec3(0.))*globals.sky.y),1.);
}
