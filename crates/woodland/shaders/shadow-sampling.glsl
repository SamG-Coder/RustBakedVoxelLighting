// Two continuously anchored orthographic cascades packed horizontally. Clamp each tap inside
// its tile so the PCF kernel never fetches the neighbouring cascade.
vec3 shadowCoordinate(vec3 p,bool farCascade){
 vec4 clip=(farCascade?globals.far_light_projection:globals.light_projection)*vec4(p,1.);
 return clip.xyz/clip.w;
}
vec2 receiverSlope(vec3 q){
#ifdef VOLUME_SHADOW
 return vec2(0.); // The volume has no receiving surface plane.
#else
 vec3 dx=dFdx(q),dy=dFdy(q);
 float det=dx.x*dy.y-dx.y*dy.x;
 if(abs(det)<1e-10)return vec2(0.);
 // q.xy is NDC; depth changes twice as fast per unit texture UV.
 return clamp(2.*vec2(dy.y*dx.z-dx.y*dy.z,dx.x*dy.z-dy.x*dx.z)/det,vec2(-4.),vec2(4.));
#endif
}
float shadowTile(vec3 q,bool farCascade,float bias,vec2 slope){
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
 // A translated 5x5 tent [1,2,3,2,1] has six texels of support per
 // axis. Pair them into three linear-PCF samples: still nine lookups, but
 // continuous weights as the sun moves between shadow texels.
 vec2 pixel=uv*2048.-.5;
 vec2 f=fract(pixel),base=(floor(pixel)+.5)/2048.;
 vec3 wx=vec3(3.-2.*f.x,5.,1.+2.*f.x);
 vec3 wy=vec3(3.-2.*f.y,5.,1.+2.*f.y);
 vec3 ox=vec3(-2.+(2.-f.x)/wx.x,(2.+f.x)/5.,2.+f.x/wx.z);
 vec3 oy=vec3(-2.+(2.-f.y)/wy.x,(2.+f.y)/5.,2.+f.y/wy.z);
 for(int y=0;y<3;y++)for(int x=0;x<3;x++){
  vec2 tap=clamp(base+vec2(ox[x],oy[y])/2048.,vec2(.5/2048.),vec2(1.-.5/2048.));
  float receiverDepth=q.z+dot(slope,tap-uv);
  tap.x=(tap.x+(farCascade?1.:0.))*.5;
  result+=texture(shadow_map,vec3(tap,receiverDepth-bias))*wx[x]*wy[y];
 }
 return result/81.;
#endif
}
float cascadedShadow(vec3 p,float bias,out float coarseBlend){
 vec3 nearQ=shadowCoordinate(p,false),farQ=shadowCoordinate(p,true);
 vec2 nearSlope=receiverSlope(nearQ),farSlope=receiverSlope(farQ);
 float nearEdge=max(abs(nearQ.x),abs(nearQ.y));
 float farEdge=max(abs(farQ.x),abs(farQ.y));
 coarseBlend=smoothstep(.82,1.,farEdge);
 float farAmount=smoothstep(.75,.95,nearEdge);
 float nearLight=farAmount<1.?shadowTile(nearQ,false,bias,nearSlope):1.;
 float farLight=farAmount>0.?shadowTile(farQ,true,bias*1.8,farSlope):nearLight;
 return mix(nearLight,farLight,farAmount);
}
