#version 460
#extension GL_GOOGLE_include_directive : require
#include "common.glsl"
#include "atmosphere.glsl"
layout(location=0) in vec2 uv;
layout(location=0) out vec4 color;
void main(){
 vec4 p=globals.inverse_view_projection*vec4(uv*2.-1.,.5,1.);
 vec3 ray=normalize(p.xyz/p.w-globals.camera_time.xyz);
 vec3 sky=atmosphereRadiance(ray,globals.camera_time.xyz,true);
 vec2 stars=vec2(atan(ray.z,ray.x),asin(clamp(ray.y,-1.,1.)))*330.;
 float star=step(.996,weatherHash(floor(stars)))*pow(max(0.,1.-length(fract(stars)-.5)*2.),6.);
 float clear=ray.y>.015?exp(-cloudDensity(globals.camera_time.xz+ray.xz*850./max(ray.y,.015))*3.2):0.;
 sky+=vec3(.5,.62,.85)*star*(1.-globals.sky.z)*smoothstep(0.,.25,ray.y)*clear;
 color=vec4(sky,1.);
}
