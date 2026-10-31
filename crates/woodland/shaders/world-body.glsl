#include "common.glsl"
layout(set=0,binding=1) uniform sampler2D images[64];
#include "lighting.glsl"
layout(set=0,binding=3) uniform sampler2DShadow shadow_map;
layout(location=0) in vec3 world;
layout(location=1) in vec3 world_normal;
layout(location=2) in vec2 texcoord;
layout(location=3) in vec4 colour;
layout(location=4) in vec2 auxiliary;
layout(location=5) in vec2 fade;
layout(location=6) in vec2 yaw;
layout(location=7) flat in int grass_tile;
layout(location=8) flat in int grass_lod;
layout(location=9) in vec2 grass_shape;
layout(location=0) out vec4 output_colour;
const float bayer[16]=float[](0.,8.,2.,10.,12.,4.,14.,6.,3.,11.,1.,9.,15.,7.,13.,5.);
#include "atmosphere.glsl"
#include "surface.glsl"
#include "ray-query.glsl"
#include "reflections.glsl"
#include "shadow-sampling.glsl"
float visibility(out float coarseBlend){
    vec3 receiverNormal=normalize(world_normal);
    vec3 receiver=world+receiverNormal*.035;
    if(int(materials.items[draw.material].settings.x)==2)
        receiver+=vec3(0.,grass_shape.x*texcoord.y*.95+.06,0.);
    float bias=.00012+.00016*(1.-max(dot(receiverNormal,normalize(globals.sun.xyz)),0.));
    return cascadedShadow(receiver,bias,coarseBlend);
}
// Source surface-variation sampling: continuous phase blending with unwrapped derivatives.
vec4 varied(uint map,vec2 uv){
    float phase=(sin(world.x*.19+sin(world.z*.13)*2.1)+sin(world.z*.23-world.x*.07))*2.7+12.;
    float i=floor(phase);
    vec2 a=fract(sin(vec2(i*127.1,i*311.7))*43758.5453)*7.13;
    vec2 b=fract(sin(vec2((i+1.)*127.1,(i+1.)*311.7))*43758.5453)*7.13;
    vec2 dx=dFdx(uv),dy=dFdy(uv);
    return mix(textureGrad(images[map],uv+a,dx,dy),textureGrad(images[map],uv+b,dx,dy),smoothstep(.18,.82,fract(phase)));
}
void main(){
    float threshold=(bayer[(int(gl_FragCoord.y)%4)*4+int(gl_FragCoord.x)%4]+.5)/16.;
    if(fade.y>.5 ? threshold<1.-fade.x : threshold>=fade.x)discard;
    Material mat=materials.items[draw.material];int kind=int(mat.settings.x);
    if(kind==2){
        float d=distance(world.xz,globals.camera_time.xz);
        float nearFade=smoothstep(45.,65.,d), farFade=smoothstep(80.,96.,d);
        if(grass_lod==0 && threshold>=1.-nearFade)discard;
        if(grass_lod==1 && (threshold<1.-nearFade || threshold>=1.-farFade))discard;
        if(grass_lod==2 && (threshold<1.-farFade || threshold>=1.-smoothstep(240.,280.,d)))discard;
    }
    vec2 sampleUV=kind==0?texcoord*vec2(1.,max(mat.settings.w,1.)):texcoord;
    if(kind==2)sampleUV=(vec2(grass_tile%4,grass_tile/4)*1280.+13.+texcoord*1254.)/5120.;
    vec4 surface=texture(images[mat.textures.x],sampleUV)*mat.tint;
    vec3 n=normalize(world_normal);
    float coverage=1.;
    float ao=1.;float roughness=mat.settings.z;
    if(kind==4){
        vec4 land;float rock,merge,bankWet;
        vec2 uv=vec2(world.x,-world.z);
        surface=vec4(terrainAlbedo(mat,world,-1.,dFdx(uv),dFdy(uv),land,rock,merge,bankWet),1.);
        ao=1.-land.a*.16;roughness=mix(roughness,.67,bankWet);
        n=normalize(mix(n,vec3(0.,1.,0.),merge*.85));
        if(distance(world,globals.camera_time.xyz)<160.){
            vec3 detail=mix(varied(mat.extra.z,uv*.72).xyz,varied(mat.extra.w,uv*.36).xyz,rock)*2.-1.;
            float soilWeight=max(smoothstep(.08,.65,land.g),smoothstep(.08,.72,land.r));
            n=mappedNormal(world,uv,n,detail,(1.-smoothstep(40.,160.,distance(world,globals.camera_time.xyz)))*mix(.10,.35,max(soilWeight,rock)));
        }
    }else if(kind==5){
#ifdef WATER_PASS
        n=flowingWaterNormal(mat.textures.x,colour,world.xz,globals.camera_time.w,distance(world,globals.camera_time.xyz));
#endif
        surface=vec4(.025,.095,.105,1.);
    }else{
        coverage=clamp((surface.a-mat.settings.y)/max(fwidth(surface.a),.025)+.5,0.,1.);
        if(coverage<=0.)discard;
        surface.rgb*=colour.rgb;
        if(kind==3){
            vec3 detail=texture(images[mat.extra.x],texcoord).xyz*2.-1.;
            n=normalize(vec3(detail.x*yaw.y+detail.z*yaw.x,detail.y,-detail.x*yaw.x+detail.z*yaw.y));
            ao=.78;
        }else if(kind==2){
            vec3 detail=texture(images[mat.extra.x],sampleUV).xyz*2.-1.;
            detail.x*=grass_shape.y;
            // A tuft represents upright blades, independent of either card face.
            vec3 blade=normalize(vec3(detail.x*.34,.78,detail.y*.34));
            float a=auxiliary.x;
            n=vec3(blade.x*cos(a)+blade.z*sin(a),blade.y,-blade.x*sin(a)+blade.z*cos(a));
            ao=mix(.8,1.,smoothstep(.02,.7,1.-texcoord.y));

        }else if(kind==1||kind==0){
            if(mat.extra.y==1u){
                vec3 detail=texture(images[mat.extra.x],sampleUV).xyz*2.-1.;
                n=mappedNormal(world,sampleUV,gl_FrontFacing?n:-n,detail,kind==0?.70:.36);
            }
            if(kind==1)ao=.90;
        }
    }
    vec3 light=normalize(globals.sun.xyz);vec3 view=normalize(globals.camera_time.xyz-world);
    float ndl=max(dot(n,light),0.);
    Probe transport=transportAt(world+n*.2);
    float coarseBlend;
    float shadow=visibility(coarseBlend);
    shadow=mix(shadow,sunTransport(transport,light),coarseBlend);
#ifdef RTX
    if(globals.weather.z>1.5 && distance(world,globals.camera_time.xyz)<140. && kind!=5){
        // Raster PCF retains soft canopy coverage; ray queries add off-screen/contact occlusion.
        shadow=min(shadow,mix(.12,1.,tracedShadow(world+n*.06,light)));
    }
#endif
    vec2 geography=clamp((world.xz+2000.)/4000.,0.,1.);
    float moisture=textureLod(images[uint(globals.environment.y+.5)],geography,0.).b;
    float wet=clamp(globals.weather.x+smoothstep(.72,.98,moisture)*.35,0.,1.);
    if(kind==0||kind==4){surface.rgb*=mix(1.,.72,wet);roughness=mix(roughness,kind==4?max(.67,roughness-.12):.62,wet);}
    // Filter normal-map highlights before they collapse to subpixel sparkles.
    roughness=clamp(sqrt(roughness*roughness+min(dot(dFdx(n),dFdx(n))+dot(dFdy(n),dFdy(n)),.25)),.12,1.);
    vec3 radiance=shadeSurface(surface.rgb,n,view,world,kind,roughness,ao,transport,shadow);
    output_colour=vec4(max(radiance,vec3(0.)),coverage);
#ifdef WATER_PASS
    output_colour=vec4(waterDisplay(n,light,view,shadow),1.);
#endif
}
