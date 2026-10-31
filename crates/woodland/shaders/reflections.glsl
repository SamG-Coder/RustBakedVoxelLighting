#ifdef WATER_PASS
layout(set=0,binding=5) uniform sampler2D scene_color;
layout(set=0,binding=6) uniform sampler2DMS scene_depth;
vec3 flowingWaterNormal(uint map,vec4 flow,vec2 coord,float time,float distanceToEye){
 float phase=fract(time/6.),other=fract(phase+.5),blend=1.-abs(phase*2.-1.);
 vec2 a=coord-flow.xy*(phase*6.*flow.z),b=coord-flow.xy*(other*6.*flow.z);
 vec4 broad=mix(texture(images[map],b*vec2(.16,.24)),texture(images[map],a*vec2(.16,.24)),blend);
 vec4 fine=mix(texture(images[map],b*vec2(.59,.71)+vec2(.37,.19)),texture(images[map],a*vec2(.59,.71)+vec2(.37,.19)),blend);
 float detail=1.-smoothstep(20.,180.,distanceToEye);
 vec2 slope=(broad.rg-.5)*.24+(fine.rg-.5)*.10*detail;
 slope+=flow.xy*sin(dot(coord,flow.xy)*3.8-time*flow.z*3.8+sin(coord.x*1.7))*.009*detail;
 return normalize(vec3(-slope.x,1.,-slope.y));
}
vec3 reconstruct(vec2 uv,float depth){vec4 p=globals.inverse_view_projection*vec4(uv*2.-1.,depth,1.);return p.xyz/p.w;}
vec4 screenReflection(vec3 origin,vec3 direction){
 float previous=.15;
 for(int i=0;i<24;i++){
  float t=.3+float(i)*.5+float(i*i)*.16;
  vec3 point=origin+direction*t;
  vec4 clip=globals.view_projection*vec4(point,1.);
  if(clip.w<=0.)break;
  vec3 q=clip.xyz/clip.w;vec2 uv=q.xy*.5+.5;
  if(any(lessThan(uv,vec2(.002)))||any(greaterThan(uv,vec2(.998)))||q.z>1.)break;
  float depth=texelFetch(scene_depth,ivec2(uv*vec2(textureSize(scene_depth))),0).r;
  if(q.z>depth && depth<.999999){
   float lo=previous,hi=t;
   for(int j=0;j<5;j++){
    float mid=(lo+hi)*.5;
    vec4 c=globals.view_projection*vec4(origin+direction*mid,1.);
    vec2 u=c.xy/c.w*.5+.5;
    float d=texelFetch(scene_depth,ivec2(clamp(u,vec2(.001),vec2(.999))*vec2(textureSize(scene_depth))),0).r;
    if(c.z/c.w>d)hi=mid;else lo=mid;
   }
   vec3 hit=origin+direction*hi;
   vec4 c=globals.view_projection*vec4(hit,1.);uv=c.xy/c.w*.5+.5;
   depth=texelFetch(scene_depth,ivec2(uv*vec2(textureSize(scene_depth))),0).r;
   float error=distance(hit,reconstruct(uv,depth));
   float edge=smoothstep(0.,.08,min(min(uv.x,1.-uv.x),min(uv.y,1.-uv.y)));
   if(error<2.)return vec4(texture(scene_color,uv).rgb,edge*(1.-smoothstep(.4,2.,error)));
  }
  previous=t;
 }
 return vec4(0.);
}
vec3 waterDisplay(vec3 normal,vec3 light,vec3 view,float shadow){
 vec2 screen=gl_FragCoord.xy/vec2(textureSize(scene_depth));
 float depth=texelFetch(scene_depth,ivec2(gl_FragCoord.xy),0).r;
 float thickness=clamp(distance(reconstruct(screen,depth),world),0.,20.);
 vec2 refractUV=clamp(screen+normal.xz*.006*min(thickness,1.),vec2(.002),vec2(.998));
 vec3 bed=texture(scene_color,refractUV).rgb;
 vec3 absorption=exp(-vec3(.26,.12,.075)*thickness);
 vec3 water=bed*absorption+vec3(.018,.043,.035)*globals.sky.x*(1.-absorption);
 vec3 reflected=reflect(-view,normal);
 vec3 reflectedDisplay=atmosphereRadiance(reflected,world,true);
 vec4 hit=screenReflection(world+normal*.12,reflected);
#ifdef RTX
 if(globals.weather.z>.5 && hit.a<.95){
  vec4 traced=tracedReflection(world+normal*.12,reflected);
  reflectedDisplay=mix(reflectedDisplay,traced.rgb,traced.a);
 }
#endif
 reflectedDisplay=mix(reflectedDisplay,hit.rgb,hit.a);
 float fresnel=.02+.98*pow(1.-max(dot(normal,view),0.),5.);
 water=mix(water,reflectedDisplay,fresnel);
 float variance=dot(fwidth(normal),fwidth(normal));
 float exponent=clamp(2./(.006+variance*3.),20.,320.);
 float sun=pow(max(dot(normal,normalize(view+light)),0.),exponent)*(exponent/320.)*shadow*globals.sun.w;
 water+=globals.sun_colour.rgb*sun*.2*cloudTransmission(world,light);
 // Fine broken edge ripples only in shallow, faster water; no constant white shoreline.
 float ripple=pow(max(0.,sin(dot(world.xz,colour.xy)*9.-globals.camera_time.w*colour.z*9.)),10.);
 float foam=(1.-smoothstep(.03,.30,thickness))*smoothstep(.35,.85,colour.z)*ripple*.12;
 water=mix(water,vec3(.65)*globals.sky.x,foam);
 return water;
}
#endif
