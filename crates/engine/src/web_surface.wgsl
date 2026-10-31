
struct Globals {vp:mat4x4<f32>,light:mat4x4<f32>,far_light:mat4x4<f32>,inverse:mat4x4<f32>,camera:vec4f,sun:vec4f,sky:vec4f,environment:vec4f,weather:vec4f,sun_colour:vec4f,geography:vec4u}
struct Material {textures:vec4u,settings:vec4f,tint:vec4f,extra:vec4u}
@group(0) @binding(0) var<uniform> g:Globals;
@group(1) @binding(0) var<uniform> m:Material;
@group(1) @binding(1) var smp:sampler;
@group(1) @binding(2) var t0:texture_2d<f32>;
@group(1) @binding(3) var t1:texture_2d<f32>;
@group(1) @binding(4) var t2:texture_2d<f32>;
@group(1) @binding(5) var t3:texture_2d<f32>;
@group(1) @binding(6) var e0:texture_2d<f32>;
@group(1) @binding(7) var e1:texture_2d<f32>;
@group(1) @binding(8) var cover:texture_2d<f32>;
@group(1) @binding(9) var canopy:texture_2d<f32>;
struct Input {@location(0) p:vec3f,@location(1) n:vec3f,@location(2) uv:vec2f,@location(3) colour:vec4f,@location(4) extra:vec2f,@location(5) pos:vec4f,@location(6) yaw:vec4f,@location(7) lod:vec4f}
struct Output {@builtin(position) clip:vec4f,@location(0) world:vec3f,@location(1) normal:vec3f,@location(2) uv:vec2f,@location(3) colour:vec4f,@location(4) fade:vec2f,@location(5) @interpolate(flat) tile:u32,@location(6) @interpolate(flat) lod:u32,@location(7) yaw:vec2f,@location(8) extra:vec2f}
fn rotate(p:vec3f,r:vec2f)->vec3f{return vec3f(p.x*r.y+p.z*r.x,p.y,-p.x*r.x+p.z*r.y);}
fn vertex(i:Input)->Output {
 var o:Output;o.world=rotate(i.p*i.pos.w,i.yaw.xy)+i.pos.xyz;o.normal=rotate(i.n,i.yaw.xy);o.uv=i.uv;o.colour=i.colour;o.fade=i.lod.xy;o.tile=0u;o.lod=u32(i.lod.z+0.5);o.yaw=i.yaw.xy;o.extra=i.extra;
 let kind=u32(m.settings.x);if(kind==2u){o.tile=u32(i.n.z+0.5);o.normal=vec3f(0,1,0);let v=clamp(1.-i.uv.y,0.,1.);let anchor=i.n.xy+i.pos.xz;
 let inset=array<f32,16>(.023,.012,.008,.016,.138,.009,.017,.045,0.,.078,.014,.055,.016,.006,.006,.011);o.world.y-=abs(i.colour.a)*inset[min(o.tile,15u)];
 let t=g.camera.w;let phase=dot(anchor,vec2f(.083,.061))+t*.85;let gust=.65+.35*sin(phase*.37);let id=dot(anchor,vec2f(1.37,2.71));let sway=sin(phase)+.26*sin(t*2.3+id*.73+v);let movement=vec3f(sway*.82,0.,sin(phase*.83+.7)*.57)*.085*gust*v*v*i.extra.y;o.world+=rotate(movement,vec2f(sin(i.extra.x),cos(i.extra.x)));
 }if(kind==3u){let d=distance(g.camera.xyz,vec3f(i.extra.x,i.colour.a,i.extra.y));o.fade=vec2f(clamp((d-400.)/80.,0.,1.),1.);o.yaw=i.colour.rg;o.colour=vec4f(1.);}else if(kind!=4u){o.colour=vec4f(o.colour.rgb*i.yaw.z,o.colour.a);}
 if(kind==5u){let a=dot(o.world.xz,i.colour.xy);let speed=max(i.colour.z,.08);o.world.y+=(sin(a*1.9-g.camera.w*speed*1.9+o.world.x*.7)*.012+sin(a*3.1-g.camera.w*speed*3.1-o.world.z*1.1)*.007)*smoothstep(.05,.55,i.colour.w);}
 o.clip=g.vp*vec4f(o.world,1.);return o;
}
fn tone(c:vec3f)->vec3f {let x=max(c*g.sky.y,vec3f(0.));return clamp((x*(2.51*x+.03))/(x*(2.43*x+.59)+.14),vec3f(0.),vec3f(1.));}
@fragment fn fs(o:Output,@builtin(front_facing) front:bool)->@location(0) vec4f {
 let bayer=array<f32,16>(0.,8.,2.,10.,12.,4.,14.,6.,3.,11.,1.,9.,15.,7.,13.,5.);let threshold=(bayer[(u32(o.clip.y)%4u)*4u+u32(o.clip.x)%4u]+.5)/16.;let kind=u32(m.settings.x);
 var sample_uv=o.uv;if(kind==0u){sample_uv.y*=max(m.settings.w,1.);}if(kind==2u){sample_uv=(vec2f(f32(o.tile%4u),f32(o.tile/4u))*1280.+13.+o.uv*1254.)/5120.;}
 // Explicit gradients keep sampling valid after per-pixel LOD coverage decisions.
 let dx=dpdx(sample_uv);let dy=dpdy(sample_uv);var surface=textureSampleGrad(t0,smp,sample_uv,dx,dy)*m.tint;
 let normal_detail=textureSampleGrad(e0,smp,sample_uv,dx,dy).xyz*2.-1.;
 let geo=clamp((o.world.xz+2000.)/4000.,vec2f(.00001),vec2f(.99999));let canopy_value=textureSampleLevel(canopy,smp,geo,0.).a;
 if(select((threshold >= o.fade.x),(threshold < 1.-o.fade.x),(o.fade.y > .5))){discard;}
 let d=distance(o.world.xz,g.camera.xz);if(kind==2u){let near=smoothstep(45.,65.,d);let far=smoothstep(80.,96.,d);if((o.lod==0u&&threshold>=1.-near)||(o.lod==1u&&(threshold<1.-near||threshold>=1.-far))||(o.lod==2u&&(threshold<1.-far||threshold>=1.-smoothstep(240.,280.,d)))){discard;}}
 var coverage=1.;var n=normalize(o.normal);if(kind==4u){
 let dimensions=vec2f(textureDimensions(e0));let land=textureSampleLevel(e0,clamp_sampler,(geo*(dimensions-1.)+.5)/dimensions,0.);let habitat=textureSampleLevel(e1,clamp_sampler,(geo*511.+.5)/512.,0.);let uv=vec2f(o.world.x,-o.world.z);
 let meadow=terrain_sample(t0,uv*.25,o.world).rgb*habitat.rgb*2.;let soil=terrain_sample(t1,uv*.72,o.world).rgb;let rock=terrain_sample(t2,uv*.36,o.world).rgb;let litter=terrain_sample(t3,uv*.25,o.world).rgb;
 let worn=max(smoothstep(.08,.72,land.r),smoothstep(.08,.65,land.g)*.92);let stone=smoothstep(.32,.72,land.b)*(1.-worn);let forest=habitat.a*(1.-smoothstep(.08,.45,land.r));var c=mix(mix(mix(meadow,soil,worn),rock,stone),litter,forest);
 let distant=textureSampleLevel(cover,clamp_sampler,(geo*511.+.5)/512.,0.);let merge=smoothstep(100.,260.,d)*distant.a*(1.-worn)*(1.-stone);c=mix(c,distant.rgb*clamp(.9+dot(meadow,vec3f(.2126,.7152,.0722))*.35,.9,1.1),merge);n=normalize(mix(n,vec3f(0.,1.,0.),merge*.85));
 let bank=textureSampleLevel(bank_map,clamp_sampler,(geo*(dimensions-1.)+.5)/dimensions,0.).ba;let exposure=bank.x*(1.-smoothstep(.3,.85,land.r));let patches=weather_noise(o.world.xz*.19+weather_noise(o.world.xz*.031)*4.);let gravel=clamp(bank.y*.68+smoothstep(.35,.70,patches)*.42,0.,1.);c=mix(c,mix(soil*vec3f(.69,.65,.58),rock*.80,gravel),exposure);surface=vec4f(c*(.92+.08*weather_noise(o.world.xz*.035)),1.);

 }else if(kind!=5u){coverage=clamp((surface.a-m.settings.y)/max(fwidth(surface.a),.025)+.5,0.,1.);if(kind==1u){let dimensions=vec2f(textureDimensions(t0));let last_mip=floor(log2(max(dimensions.x,dimensions.y)));let mip=log2(max(max(length(dx*dimensions),length(dy*dimensions)),1.));coverage=mix(coverage,surface.a,smoothstep(last_mip-2.,last_mip,mip));}if(coverage<=0.){discard;}surface=vec4f(surface.rgb*o.colour.rgb,1.);if(kind==2u){let blade=normalize(vec3f(normal_detail.x*.34,.78,normal_detail.y*.34));n=rotate(blade,vec2f(sin(o.extra.x),cos(o.extra.x)));}else if(kind==3u){n=normalize(rotate(normal_detail,o.yaw));}else{n=select(-n,n,front);}}
 let light=normalize(g.sun.xyz);let view=normalize(g.camera.xyz-o.world);
 var ao=1.;if(kind==2u){ao=mix(.8,1.,smoothstep(.02,.7,1.-o.uv.y));}else if(kind==1u){ao=.90;}else if(kind==3u){ao=.78;}
 if((kind==0u||kind==1u)&&m.extra.y==1u){n=mapped_normal(o.world,sample_uv,n,normal_detail,select(.36,.70,kind==0u));}
 let moisture=textureSampleLevel(heights,clamp_sampler,geo,0.).b;let wet=clamp(g.weather.x+smoothstep(.72,.98,moisture)*.35,0.,1.);var rough=m.settings.z;if(kind==0u||kind==4u){surface=vec4f(surface.rgb*mix(1.,.72,wet),surface.a);rough=mix(rough,select(.62,max(.67,rough-.12),kind==4u),wet);}
 rough=clamp(sqrt(rough*rough+min(dot(dpdx(n),dpdx(n))+dot(dpdy(n),dpdy(n)),.25)),.12,1.);
 let transport=transport_at(o.world+n*.2);let shadow=shadows(o.world+n*.035,.00012+.00016*(1.-max(dot(n,light),0.)),transport);
 var radiance=shade_surface(surface.rgb,n,view,o.world,kind,rough,ao,transport,shadow);
 if(kind==5u){n=flowing_water_normal(o.world.xz,o.colour,g.camera.w,distance(o.world,g.camera.xyz));return vec4f(water_display(o,n,light,view,shadow),1.);}

 return vec4f(max(radiance,vec3f(0.)),coverage);
}
struct SkyOut {@builtin(position) clip:vec4f,@location(0) uv:vec2f}
@vertex fn sky_vs(@builtin(vertex_index) id:u32)->SkyOut {let p=array<vec2f,3>(vec2f(-1.,-1.),vec2f(3.,-1.),vec2f(-1.,3.));var o:SkyOut;o.clip=vec4f(p[id],.99999,1.);o.uv=p[id];return o;}
@fragment fn sky_fs(o:SkyOut)->@location(0) vec4f {let w=g.inverse*vec4f(o.uv,1.,1.);return vec4f(atmosphere(normalize(w.xyz/w.w-g.camera.xyz),g.camera.xyz,true,true),1.);}


@vertex fn vs(i:Input)->Output {return vertex(i);}
@vertex fn shadow_vs(i:Input)->Output {var o=vertex(i);o.clip=g.light*vec4f(o.world,1.);return o;}
@vertex fn far_shadow_vs(i:Input)->Output {var o=vertex(i);o.clip=g.far_light*vec4f(o.world,1.);return o;}
@fragment fn shadow_fs(o:Output) {
 let kind=u32(m.settings.x);var uv=o.uv;if(kind==2u){uv=(vec2f(f32(o.tile%4u),f32(o.tile/4u))*1280.+13.+o.uv*1254.)/5120.;}
 let alpha=textureSample(t0,smp,uv).a;if(kind==1u||kind==2u||kind==3u){if(alpha<m.settings.y){discard;}if(kind==1u){let hash=fract(sin(dot(floor(uv*1024.),vec2f(12.9898,78.233)))*43758.5453);if(hash>.70){discard;}}}
}

fn terrain_sample(map:texture_2d<f32>,uv:vec2f,p:vec3f)->vec4f {let phase=(sin(p.x*.19+sin(p.z*.13)*2.1)+sin(p.z*.23-p.x*.07))*2.7+12.;let i=floor(phase);let a=fract(sin(vec2f(i*127.1,i*311.7))*43758.5453)*7.13;let b=fract(sin(vec2f((i+1.)*127.1,(i+1.)*311.7))*43758.5453)*7.13;return mix(textureSampleGrad(map,smp,uv+a,dpdx(uv),dpdy(uv)),textureSampleGrad(map,smp,uv+b,dpdx(uv),dpdy(uv)),smoothstep(.18,.82,fract(phase)));}
