#ifdef RTX
layout(set=0,binding=7) uniform accelerationStructureEXT scene_acceleration;
layout(std430,set=0,binding=8) readonly buffer RayMetadata {uvec4 meshes[];} ray_data;
layout(std430,set=0,binding=9) readonly buffer RayVertices {float data[];} ray_vertices;
layout(std430,set=0,binding=10) readonly buffer RayIndices {uint data[];} ray_indices;
vec2 rayUV(uint index){uint b=index*14;return vec2(ray_vertices.data[b+6],ray_vertices.data[b+7]);}
vec3 rayNormal(uint index){uint b=index*14;return vec3(ray_vertices.data[b+3],ray_vertices.data[b+4],ray_vertices.data[b+5]);}
vec4 raySurface(rayQueryEXT query,bool committed,out vec3 normal,out uint material){
 uint mesh=(committed?rayQueryGetIntersectionInstanceCustomIndexEXT(query,true):rayQueryGetIntersectionInstanceCustomIndexEXT(query,false));
 uint primitive=(committed?rayQueryGetIntersectionPrimitiveIndexEXT(query,true):rayQueryGetIntersectionPrimitiveIndexEXT(query,false));
 uvec4 info=ray_data.meshes[mesh];material=info.z;
 uvec3 indices=uvec3(ray_indices.data[info.x+primitive*3],ray_indices.data[info.x+primitive*3+1],ray_indices.data[info.x+primitive*3+2])+info.y;
 vec2 b=(committed?rayQueryGetIntersectionBarycentricsEXT(query,true):rayQueryGetIntersectionBarycentricsEXT(query,false));vec3 w=vec3(1.-b.x-b.y,b.x,b.y);
 vec2 uv=rayUV(indices.x)*w.x+rayUV(indices.y)*w.y+rayUV(indices.z)*w.z;
 vec3 n=rayNormal(indices.x)*w.x+rayNormal(indices.y)*w.y+rayNormal(indices.z)*w.z;
 normal=normalize(mat3((committed?rayQueryGetIntersectionObjectToWorldEXT(query,true):rayQueryGetIntersectionObjectToWorldEXT(query,false)))*n);
 if(int(materials.items[material].settings.x)==0)uv.y*=max(materials.items[material].settings.w,1.);
 return textureLod(images[nonuniformEXT(materials.items[material].textures.x)],uv,1.);
}
float tracedShadow(vec3 p,vec3 direction){
 rayQueryEXT query;
 float transmission=1.;
 rayQueryInitializeEXT(query,scene_acceleration,gl_RayFlagsTerminateOnFirstHitEXT,255,p,.06,direction,220.);
 while(rayQueryProceedEXT(query)){
  if(rayQueryGetIntersectionTypeEXT(query,false)==gl_RayQueryCandidateIntersectionTriangleEXT){
   vec3 n;uint mat;vec4 surface=raySurface(query,false,n,mat);
   if(surface.a>=materials.items[mat].settings.y){
    if(int(materials.items[mat].settings.x)==1){
     // Thin leaves attenuate sunlight; trunks and terrain still stop it.
     transmission*=.30;
     if(transmission<.025){rayQueryTerminateEXT(query);return transmission;}
    }else rayQueryConfirmIntersectionEXT(query);
   }
  }
 }
 return rayQueryGetIntersectionTypeEXT(query,true)==gl_RayQueryCommittedIntersectionNoneEXT?transmission:0.;
}
#ifndef PROBE_COMPUTE
vec4 tracedReflection(vec3 p,vec3 direction){
 rayQueryEXT query;
 rayQueryInitializeEXT(query,scene_acceleration,0,255,p,.08,direction,350.);
 while(rayQueryProceedEXT(query)){
  if(rayQueryGetIntersectionTypeEXT(query,false)==gl_RayQueryCandidateIntersectionTriangleEXT){
   vec3 n;uint mat;vec4 surface=raySurface(query,false,n,mat);
   if(surface.a>=materials.items[mat].settings.y)rayQueryConfirmIntersectionEXT(query);
  }
 }
 if(rayQueryGetIntersectionTypeEXT(query,true)==gl_RayQueryCommittedIntersectionNoneEXT)return vec4(0.);
 vec3 n;uint material;vec4 surface=raySurface(query,true,n,material);
 vec3 hit=p+direction*rayQueryGetIntersectionTEXT(query,true);
 Probe transport=transportAt(hit+n*.2);
 vec3 light=normalize(globals.sun.xyz);
 float ndl=max(dot(n,light),0.);
 Material mat=materials.items[material];int kind=int(mat.settings.x);
 float roughness=mat.settings.z;
 surface.rgb*=mat.tint.rgb;
 if(kind==4){vec4 land;float rock,merge,bankWet;surface.rgb=terrainAlbedo(mat,hit,2.,vec2(0.),vec2(0.),land,rock,merge,bankWet);roughness=mix(roughness,.67,bankWet);}
 float moisture=textureLod(images[uint(globals.environment.y+.5)],clamp((hit.xz+2000.)/4000.,0.,1.),0.).b;
 float wet=clamp(globals.weather.x+smoothstep(.72,.98,moisture)*.35,0.,1.);
 if(kind==0||kind==4){surface.rgb*=mix(1.,.72,wet);roughness=mix(roughness,kind==4?max(.67,roughness-.12):.62,wet);}
 vec3 radiance=shadeSurface(surface.rgb,n,-direction,hit,kind,roughness,1.,transport,sunTransport(transport,light));
 return vec4(radiance,1.);
}
#endif
#endif
