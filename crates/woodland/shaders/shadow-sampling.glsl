// Two stable orthographic cascades packed horizontally. Clamp each tap inside
// its tile so the PCF kernel never fetches the neighbouring cascade.
vec3 shadowCoordinate(vec3 p,bool farCascade){
 vec4 clip=(farCascade?globals.far_light_projection:globals.light_projection)*vec4(p,1.);
 return clip.xyz/clip.w;
}
float shadowTile(vec3 q,bool farCascade,float bias){
 if(any(greaterThan(abs(q.xy),vec2(1.)))||q.z<=0.||q.z>=1.)return 1.;
 vec2 uv=q.xy*.5+.5;float result=0.;
#ifdef VOLUME_SHADOW
 // Low-frequency scattering needs a broad stable filter, not the surface's
 // nine-tap contact detail at every march sample. Hardware PCF filters each tap.
 for(int i=0;i<2;i++){
  vec2 tap=clamp(uv+vec2(i==0?-.65:.65,i==0?.65:-.65)/2048.,vec2(.5/2048.),vec2(1.-.5/2048.));
  tap.x=(tap.x+(farCascade?1.:0.))*.5;
  result+=texture(shadow_map,vec3(tap,q.z-bias));
 }
 return result*.5;
#else
 for(int y=-1;y<=1;y++)for(int x=-1;x<=1;x++){
  vec2 tap=clamp(uv+vec2(x,y)*1.35/2048.,vec2(.5/2048.),vec2(1.-.5/2048.));
  tap.x=(tap.x+(farCascade?1.:0.))*.5;
  float w=float((x==0?2:1)*(y==0?2:1));
  result+=texture(shadow_map,vec3(tap,q.z-bias))*w;
 }
 return result/16.;
#endif
}
float cascadedShadow(vec3 p,float bias,out float coarseBlend){
 vec3 nearQ=shadowCoordinate(p,false),farQ=shadowCoordinate(p,true);
 float nearEdge=max(abs(nearQ.x),abs(nearQ.y));
 float farEdge=max(abs(farQ.x),abs(farQ.y));
 coarseBlend=smoothstep(.82,1.,farEdge);
 float farAmount=smoothstep(.75,.95,nearEdge);
 float nearLight=farAmount<1.?shadowTile(nearQ,false,bias):1.;
 float farLight=farAmount>0.?shadowTile(farQ,true,bias*1.8):nearLight;
 return mix(nearLight,farLight,farAmount);
}
