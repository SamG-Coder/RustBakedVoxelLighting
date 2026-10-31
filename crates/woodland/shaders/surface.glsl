#ifdef RTX
#define MATERIAL_IMAGE(i) images[nonuniformEXT(i)]
#else
#define MATERIAL_IMAGE(i) images[i]
#endif
// Material evaluation shared by raster surfaces and ray-hit shading.
const float PI=3.14159265359;
vec4 terrainSample(uint map,vec2 uv,vec3 p,float level,vec2 dx,vec2 dy){
 float phase=(sin(p.x*.19+sin(p.z*.13)*2.1)+sin(p.z*.23-p.x*.07))*2.7+12.;
 float i=floor(phase);
 vec2 a=fract(sin(vec2(i*127.1,i*311.7))*43758.5453)*7.13;
 vec2 b=fract(sin(vec2((i+1.)*127.1,(i+1.)*311.7))*43758.5453)*7.13;
 if(level<0.)return mix(textureGrad(MATERIAL_IMAGE(map),uv+a,dx,dy),textureGrad(MATERIAL_IMAGE(map),uv+b,dx,dy),smoothstep(.18,.82,fract(phase)));
 return mix(textureLod(MATERIAL_IMAGE(map),uv+a,level),textureLod(MATERIAL_IMAGE(map),uv+b,level),smoothstep(.18,.82,fract(phase)));
}
vec3 terrainAlbedo(Material mat,vec3 p,float level,vec2 dx,vec2 dy,out vec4 land,out float rock,out float merge,out float bankWet){
 vec2 geo=clamp((p.xz+2000.)/4000.,0.,1.);
 vec2 dimensions=vec2(textureSize(MATERIAL_IMAGE(mat.extra.x),0));
 land=textureLod(MATERIAL_IMAGE(mat.extra.x),(geo*(dimensions-1.)+.5)/dimensions,0.);
 vec4 habitat=textureLod(MATERIAL_IMAGE(mat.extra.y),(geo*511.+.5)/512.,0.);
 vec2 uv=vec2(p.x,-p.z);
 vec3 meadow=terrainSample(mat.textures.x,uv*.25,p,level,dx*.25,dy*.25).rgb*habitat.rgb*2.;
 vec3 soil=terrainSample(mat.textures.y,uv*.72,p,level<0.?level:level+1.5,dx*.72,dy*.72).rgb;
 vec3 stone=terrainSample(mat.textures.z,uv*.36,p,level<0.?level:level+.5,dx*.36,dy*.36).rgb;
 vec3 litter=terrainSample(mat.textures.w,uv*.25,p,level,dx*.25,dy*.25).rgb;
 float road=smoothstep(.08,.72,land.r);
 float worn=max(road,smoothstep(.08,.65,land.g)*.92);
 rock=smoothstep(.32,.72,land.b)*(1.-worn);
 float woodland=habitat.a*(1.-smoothstep(.08,.45,land.r));
 vec3 c=mix(mix(mix(meadow,soil,worn),stone,rock),litter,woodland);
 vec4 cover=textureLod(MATERIAL_IMAGE(uint(mat.settings.w+.5)),(geo*511.+.5)/512.,0.);
 merge=smoothstep(100.,260.,distance(p.xz,globals.camera_time.xz))*cover.a*(1.-worn)*(1.-rock);
 c=mix(c,cover.rgb*clamp(.9+dot(meadow,vec3(.2126,.7152,.0722))*.35,.9,1.1),merge);
 float macro=.92+.08*weatherNoise(p.xz*.035);
 vec2 bank=textureLod(images[globals.geography_maps.x],(geo*(dimensions-1.)+.5)/dimensions,0.).ba;
 float exposure=bank.x*(1.-smoothstep(.3,.85,land.r));
 // Gravel bars and sediment pockets break up the otherwise uniform shore band.
 float patches=weatherNoise(p.xz*.19+vec2(weatherNoise(p.xz*.031)*4.));
 float gravel=clamp(bank.y*.68+smoothstep(.35,.70,patches)*.42,0.,1.);
 bankWet=exposure*(1.-gravel*.55);
 vec3 sediment=mix(soil*vec3(.69,.65,.58),stone*.80,gravel);
 c=mix(c,sediment,exposure);
 return c*macro;
}
#ifndef PROBE_COMPUTE
vec3 mappedNormal(vec3 p,vec2 uv,vec3 n,vec3 detail,float strength){
 vec3 dp1=dFdx(p),dp2=dFdy(p);vec2 duv1=dFdx(uv),duv2=dFdy(uv);
 vec3 perpendicular1=cross(dp2,n),perpendicular2=cross(n,dp1);
 vec3 t=perpendicular1*duv1.x+perpendicular2*duv2.x;
 vec3 b=perpendicular1*duv1.y+perpendicular2*duv2.y;
 float scale=inversesqrt(max(max(dot(t,t),dot(b,b)),1e-12));
 return normalize(n*max(detail.z,.35)+(t*detail.x+b*detail.y)*scale*strength);
}
#endif
#if defined(RTX) && !defined(PROBE_COMPUTE)
#include "probe-cache.glsl"
#endif
vec3 shadeSurface(vec3 albedo,vec3 n,vec3 view,vec3 p,int kind,float roughness,float ao,Probe transport,float shadow){
 vec3 light=normalize(globals.sun.xyz);
 float nl=max(dot(n,light),0.),nv=max(dot(n,view),.001);
 vec3 halfVector=normalize(light+view);float nh=max(dot(n,halfVector),0.),vh=max(dot(view,halfVector),0.);
 bool foliage=kind==1||kind==2||kind==3;
 roughness=clamp(roughness,.12,1.);
 float a=roughness*roughness,a2=a*a;
 float denominator=nh*nh*(a2-1.)+1.;
 float distribution=a2/(PI*denominator*denominator);
 float geometry=.5/max(nl*sqrt(nv*nv*(1.-a2)+a2)+nv*sqrt(nl*nl*(1.-a2)+a2),.001);
 vec3 fresnel=vec3(.035)+(1.-vec3(.035))*pow(1.-vh,5.);
 vec3 direct=((1.-fresnel)*albedo/PI+distribution*geometry*fresnel)*nl;
 if(foliage){
  // Thin plant tissue, not a translucent rectangular card. No face-sign dependence for grass.
  float forwardScatter=pow(max(dot(-light,view),0.),5.);
  float transmission=(.07+.22*forwardScatter)*(1.-nl*.7);
  if(kind==1||kind==3){
   float backLight=max(dot(-n,light),0.);
   transmission=(.12+.30*forwardScatter)*(.35+.65*backLight);
  }
  direct+=albedo*vec3(.80,1.,.58)*transmission/PI;
 }
 vec2 geo=clamp((p.xz+2000.)/4000.,vec2(0.),vec2(1.));
 vec4 local=textureLod(images[globals.geography_maps.z],geo,0.);
 float canopy=textureLod(images[globals.geography_maps.y],geo,0.).a;
 float grounded=(kind==2||kind==4)?1.:1.-smoothstep(2.,24.,p.y-terrainHeight(p.xz));
 float skyVisibility=mix(transport.bounce_sky.a,min(transport.bounce_sky.a,local.a*(1.-canopy*.48)),grounded);
 vec3 bent=normalize(local.rgb*2.-1.);
 float up=clamp(dot(n,mix(vec3(0.,1.,0.),bent,grounded*.4))*.5+.5,0.,1.);
 vec3 skyDiffuse=mix(vec3(.12,.115,.095),vec3(.25,.30,.37),up)*globals.sky.x;
 vec3 indirect=albedo*(skyDiffuse*mix(.38,1.,skyVisibility)*ao+transport.bounce_sky.rgb*globals.sky.x*.65);
 indirect+=albedo*sunlightBounce(transport,light)*globals.sun.w*globals.sun_colour.rgb*cloudTransmission(p,light)/PI;
#if defined(RTX) && !defined(PROBE_COMPUTE)
 float giWeight;vec3 giLight=probeDiffuse(p,n,giWeight);
 indirect=mix(indirect,albedo*giLight*mix(1.,ao,.4),giWeight);
#endif
 // The directional BRDF already evaluates the sun/moon. Sampling the sharp
 // celestial disk again here incorrectly turns rough litter into a mirror.
 vec3 skySpec=atmosphereRadiance(reflect(-view,n),p,false,false)*.035*(1.-roughness*.72)*mix(.2,1.,skyVisibility);
 return indirect+skySpec+direct*shadow*cloudTransmission(p,light)*globals.sun.w*globals.sun_colour.rgb;
}
