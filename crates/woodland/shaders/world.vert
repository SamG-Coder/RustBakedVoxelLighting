#version 460
#extension GL_GOOGLE_include_directive : require
#include "common.glsl"
layout(set=0,binding=1) uniform sampler2D images[64];
layout(location=0) in vec3 position;
layout(location=1) in vec3 normal;
layout(location=2) in vec2 uv;
layout(location=3) in vec4 color;
layout(location=4) in vec2 extra;
layout(location=5) in vec4 position_scale;
layout(location=6) in vec4 rotation_tint;
layout(location=7) in vec4 lod;
layout(location=0) out vec3 world;
layout(location=1) out vec3 world_normal;
layout(location=2) out vec2 texcoord;
layout(location=3) out vec4 colour;
layout(location=4) out vec2 auxiliary;
layout(location=5) out vec2 fade;
layout(location=6) out vec2 yaw;
layout(location=7) flat out int grass_tile;
layout(location=8) flat out int grass_lod;
layout(location=9) out vec2 grass_shape;
vec3 rotate_y(vec3 p,vec2 r){return vec3(p.x*r.y+p.z*r.x,p.y,-p.x*r.x+p.z*r.y);}
void main(){
    Material mat=materials.items[draw.material];
    world=rotate_y(position*position_scale.w,rotation_tint.xy)+position_scale.xyz;
    world_normal=rotate_y(normal,rotation_tint.xy);
    texcoord=uv;colour=color;auxiliary=extra;fade=lod.xy;yaw=rotation_tint.xy;
    int kind=int(mat.settings.x);grass_tile=0;grass_lod=0;grass_shape=vec2(0.,1.);
    if(kind==0 && position.y<1.2){
        vec2 coord=clamp((world.xz+2000.)/4000.,0.,1.);
        vec4 packed=textureLod(images[globals.geography_maps.w],(coord*2048.+.5)/2049.,0.);
        float ground=(packed.r*65280.+packed.g*255.)/65535.*1024.-128.;
        float follow=1.-smoothstep(.04,1.2,position.y);
        world.y+=(ground-.055-position_scale.y)*follow;
    }
    if(kind==2){
        grass_tile=int(normal.z+.5);
        world_normal=vec3(0.,1.,0.);
        if(lod.z>.5 && lod.z<1.5)world.xz=normal.xy+position_scale.xz+(world.xz-normal.xy-position_scale.xz)*1.42;
        grass_lod=int(lod.z+.5);
        float v=clamp(1.-uv.y,0.,1.);
        if(grass_lod==1 && v<1.){
            vec2 coord=clamp((world.xz+2000.)/4000.,0.,1.);
            vec4 packed=textureLod(images[globals.geography_maps.w],(coord*2048.+.5)/2049.,0.);
            float ground=(packed.r*65280.+packed.g*255.)/65535.*1024.-128.;
            world.y+=(ground-.035-(world.y-v*abs(color.a)))*(1.-v)*(1.-v);
        }
        vec2 anchor=normal.xy+position_scale.xz;
        float t=globals.camera_time.w;
        // Port of native-kernels.js windPosition: 0.085 strength, .85 phase speed,
        // independent secondary flutter and v^2 anchoring, transformed by card yaw/width.
        float phase=dot(anchor,vec2(.083,.061))+t*.85;
        float gust=.65+.35*sin(phase*.37);
        float id=dot(anchor,vec2(1.37,2.71));
        float sway=sin(phase)+.26*sin(t*2.3+id*.73+v);
        vec3 movement=vec3(sway*.82,0.,sin(phase*.83+.7)*.57)*.085*gust*v*v*extra.y;
        world+=rotate_y(movement,vec2(sin(extra.x),cos(extra.x)));
        grass_shape=vec2(abs(color.a),sign(color.a));

    }
    if(kind==5){
        float t=globals.camera_time.w,speed=max(color.z,.08);
        float along=dot(world.xz,color.xy);
        float wave=sin(along*1.9-t*speed*1.9+world.x*.7)*.012+sin(along*3.1-t*speed*3.1-world.z*1.1)*.007;
        world.y+=wave*smoothstep(.05,.55,color.w);
    }
    if(kind==3){
        float d=distance(globals.camera_time.xyz,vec3(extra.x,color.a,extra.y));
        fade=vec2(clamp((d-400.)/80.,0.,1.),1.);
        yaw=color.rg;colour=vec4(1.);
    }else if(kind!=4){colour.rgb*=rotation_tint.z;}
    gl_Position=(draw.pass==1 ? globals.light_projection : draw.pass==2 ? globals.far_light_projection : globals.view_projection)*vec4(world,1.);
}
