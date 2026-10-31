// HDR scene inputs shared by water, local aerial scattering and final display.
@group(3) @binding(0) var scene_colour:texture_2d<f32>;
@group(3) @binding(1) var scene_depth:texture_depth_2d;
@group(3) @binding(2) var scene_sampler:sampler;
@group(3) @binding(3) var volume_map:texture_2d<f32>;
fn reconstruct(uv:vec2f,depth:f32)->vec3f {let p=g.inverse*vec4f(uv.x*2.-1.,1.-uv.y*2.,depth,1.);return p.xyz/p.w;}
fn screen_uv(clip:vec4f)->vec2f {return vec2f(clip.x,-clip.y)/clip.w*.5+.5;}
fn screen_depth_at(uv:vec2f)->f32 {let size=vec2i(textureDimensions(scene_depth));return textureLoad(scene_depth,clamp(vec2i(uv*vec2f(size)),vec2i(0),size-1),0);}
fn flowing_water_normal(coord:vec2f,flow:vec4f,time:f32,dist:f32)->vec3f {
 let phase=fract(time/6.);let other=fract(phase+.5);let blend=1.-abs(phase*2.-1.);let a=coord-flow.xy*(phase*6.*flow.z);let b=coord-flow.xy*(other*6.*flow.z);
 let broad=mix(textureSample(t0,smp,b*vec2f(.16,.24)),textureSample(t0,smp,a*vec2f(.16,.24)),blend);let fine=mix(textureSample(t0,smp,b*vec2f(.59,.71)+vec2f(.37,.19)),textureSample(t0,smp,a*vec2f(.59,.71)+vec2f(.37,.19)),blend);
 let detail=1.-smoothstep(20.,180.,dist);var slope=(broad.rg-.5)*.24+(fine.rg-.5)*.10*detail;slope+=flow.xy*sin(dot(coord,flow.xy)*3.8-time*flow.z*3.8+sin(coord.x*1.7))*.009*detail;return normalize(vec3f(-slope.x,1.,-slope.y));
}
fn screen_reflection(origin:vec3f,direction:vec3f)->vec4f {
 var previous=.15;for(var i=0;i<24;i++){let t=.3+f32(i)*.5+f32(i*i)*.16;let point=origin+direction*t;let clip=g.vp*vec4f(point,1.);if(clip.w<=0.){break;}let q=clip.xyz/clip.w;var uv=screen_uv(clip);if(any(uv<vec2f(.002))||any(uv>vec2f(.998))||q.z>1.){break;}var depth=screen_depth_at(uv);
 if(q.z>depth&&depth<.999999){var lo=previous;var hi=t;for(var j=0;j<5;j++){let mid=(lo+hi)*.5;let c=g.vp*vec4f(origin+direction*mid,1.);let u=screen_uv(c);let d=screen_depth_at(clamp(u,vec2f(.001),vec2f(.999)));if(c.z/c.w>d){hi=mid;}else{lo=mid;}}
 let hit=origin+direction*hi;let c=g.vp*vec4f(hit,1.);uv=screen_uv(c);depth=screen_depth_at(uv);let error=distance(hit,reconstruct(uv,depth));let edge=smoothstep(0.,.08,min(min(uv.x,1.-uv.x),min(uv.y,1.-uv.y)));if(error<2.){return vec4f(textureSampleLevel(scene_colour,scene_sampler,uv,0.).rgb,edge*(1.-smoothstep(.4,2.,error)));}}
 previous=t;}return vec4f(0.);
}
fn water_display(o:Output,n:vec3f,light:vec3f,view:vec3f,shadow:f32)->vec3f {
 let screen=o.clip.xy/vec2f(textureDimensions(scene_depth));let depth=screen_depth_at(screen);let thickness=clamp(distance(reconstruct(screen,depth),o.world),0.,20.);let refract_uv=clamp(screen+n.xz*.006*min(thickness,1.),vec2f(.002),vec2f(.998));let bed=textureSampleLevel(scene_colour,scene_sampler,refract_uv,0.).rgb;let absorption=exp(-vec3f(.26,.12,.075)*thickness);var water=bed*absorption+vec3f(.018,.043,.035)*g.sky.x*(1.-absorption);
 let reflected=reflect(-view,n);var reflection=atmosphere(reflected,o.world,true,true);let hit=screen_reflection(o.world+n*.12,reflected);reflection=mix(reflection,hit.rgb,hit.a);let fresnel=.02+.98*pow(1.-max(dot(n,view),0.),5.);water=mix(water,reflection,fresnel);
 let variance=dot(fwidth(n),fwidth(n));let exponent=clamp(2./(.006+variance*3.),20.,320.);let sun=pow(max(dot(n,normalize(view+light)),0.),exponent)*(exponent/320.)*shadow*g.sun.w;water+=g.sun_colour.rgb*sun*.2*cloud_transmission(o.world,light);
 let ripple=pow(max(0.,sin(dot(o.world.xz,o.colour.xy)*9.-g.camera.w*o.colour.z*9.)),10.);let foam=(1.-smoothstep(.03,.30,thickness))*smoothstep(.35,.85,o.colour.z)*ripple*.12;return mix(water,vec3f(.65)*g.sky.x,foam);
}
// Native local aerosol integration, with depth-aware upsampling in the display pass.
@fragment fn volume_fs(o:SkyOut)->@location(0) vec4f {
 let uv=vec2f(o.uv.x,-o.uv.y)*.5+.5;let end=reconstruct(uv,min(screen_depth_at(uv),.999999));let delta=end-g.camera.xyz;let length_to_surface=length(delta);let limit=min(length_to_surface,128.);let ray=delta/max(length_to_surface,.001);let light=normalize(g.sun.xyz);let anisotropy=.40;let phase=(1.-anisotropy*anisotropy)/(12.5663706*pow(1.+anisotropy*anisotropy-2.*anisotropy*dot(ray,light),1.5));let source=g.sun_colour.rgb*g.sun.w*phase*cloud_transmission(g.camera.xyz,light);
 let jitter=fract(52.9829189*fract(dot(o.clip.xy,vec2f(.06711056,.00583715))));var trans=1.;var scattering=vec3f(0.);
 for(var i=0;i<43;i++){let segment_length=clamp(limit-f32(i)*3.,0.,3.);if(segment_length<=0.){break;}let p=g.camera.xyz+ray*(f32(i)*3.+jitter*segment_length);let above=p.y-terrain_height(p.xz);let near=shadow_coordinate(p,false);let far=shadow_coordinate(p,true);let blend=smoothstep(.75,.95,max(abs(near.x),abs(near.y)));var lit=1.;if(blend<1.){lit=shadow_tile(near,false,.00008,vec2f(0.));}if(blend>0.){lit=mix(lit,shadow_tile(far,true,.000144,vec2f(0.)),blend);}let coarse=smoothstep(.82,1.,max(abs(far.x),abs(far.y)));if(coarse>0.){lit=mix(lit,sun_transport(transport_at(p),light),coarse);}
 let sigma=mix(.00065,.0012,g.weather.y)*exp(-max(above,0.)*.045)*smoothstep(-.4,.5,above);let segment=exp(-sigma*segment_length);scattering+=trans*(1.-segment)*(source*lit+vec3f(.025,.033,.045)*g.sky.x);trans*=segment;}
 return vec4f(scattering,trans);
}
@fragment fn display_fs(o:SkyOut)->@location(0) vec4f {
 let uv=vec2f(o.uv.x,-o.uv.y)*.5+.5;let colour=textureSampleLevel(scene_colour,scene_sampler,uv,0.).rgb;
 let size=vec2f(textureDimensions(volume_map));let base=floor(uv*size-.5);let f=fract(uv*size-.5);let centre=distance(reconstruct(uv,min(screen_depth_at(uv),.999999)),g.camera.xyz);var volume=vec4f(0.);var total=0.;
 for(var y=0;y<2;y++){for(var x=0;x<2;x++){let tap=(base+vec2f(f32(x),f32(y))+.5)/size;let z=distance(reconstruct(tap,min(screen_depth_at(tap),.999999)),g.camera.xyz);let weight=select(1.-f.x,f.x,x==1)*select(1.-f.y,f.y,y==1)*exp(-abs(z-centre)/max(1.,centre*.02));volume+=textureSampleLevel(volume_map,scene_sampler,tap,0.)*weight;total+=weight;}}
 volume/=max(total,1e-6);if(total<1e-6){volume=vec4f(0.,0.,0.,1.);}return vec4f(tone(colour*volume.a+volume.rgb),1.);
}
