
// Atmosphere, transport interpolation and shadow filtering match the native GLSL models.
struct Probe {bounce_sky:vec4f,side:vec4f,bounce_x:vec4f,bounce_y:vec4f,bounce_z:vec4f}
@group(2) @binding(0) var<storage,read> probes:array<Probe>;
@group(2) @binding(1) var shadow_near:texture_depth_2d;
@group(2) @binding(2) var shadow_far:texture_depth_2d;
@group(2) @binding(3) var shadow_sampler:sampler_comparison;
@group(2) @binding(4) var heights:texture_2d<f32>;
@group(2) @binding(5) var clamp_sampler:sampler;
@group(2) @binding(6) var local_ao:texture_2d<f32>;
@group(2) @binding(7) var bank_map:texture_2d<f32>;
fn weather_hash(p:vec2f)->f32 {let q=bitcast<vec2u>(vec2i(p));var h=q.x*374761393u+q.y*668265263u;h=(h^(h>>13u))*1274126177u;return f32(h^(h>>16u))/4294967296.;}
fn weather_noise(p:vec2f)->f32 {let i=floor(p);var f=fract(p);f=f*f*(3.-2.*f);return mix(mix(weather_hash(i),weather_hash(i+vec2f(1,0)),f.x),mix(weather_hash(i+vec2f(0,1)),weather_hash(i+1.),f.x),f.y);}
fn cloud_density(metres:vec2f)->f32 {let q=metres*.0018+vec2f(g.camera.w*.004,g.camera.w*.0013);let broad=weather_noise(q*.43);let shape=weather_noise(q)*.56+weather_noise(q*2.07+13.)*.28+weather_noise(q*4.13-7.)*.16;return smoothstep(.48-g.weather.y*.10,.72,shape+broad*.12);}
fn cloud_transmission(p:vec3f,light:vec3f)->f32 {if(light.y<.035){return 1.;}let at=p.xz+light.xz*max(0.,850.-p.y)/max(light.y,.035);return exp(-cloud_density(at)*mix(.48,1.15,g.weather.y));}
fn atmosphere(ray:vec3f,origin:vec3f,clouds:bool,celestial:bool)->vec3f {
 let h=max(ray.y,0.);let day=g.sky.z;let phase=(g.sky.w-6.)/24.*6.283185307;let sun=normalize(vec3f(cos(phase)*.6,sin(phase),cos(phase)*.8));
 let night=mix(vec3f(.008,.014,.025),vec3f(.001,.003,.012),sqrt(h));var daytime=mix(vec3f(.46,.62,.75),vec3f(.045,.19,.43),sqrt(h));let dusk=(1.-smoothstep(0.,.28,abs(sun.y)))*pow(max(dot(ray,normalize(vec3f(sun.x,.05,sun.z))),0.),6.);daytime=mix(daytime,vec3f(.72,.27,.09),dusk*.55);var sky=mix(night,daytime,day);let solar=dot(ray,sun);let lunar=dot(ray,-sun);
 if(celestial){sky+=vec3f(1.,.83,.52)*smoothstep(cos(.005),cos(.0035),solar)*day*6.;sky+=vec3f(.65,.73,.9)*smoothstep(cos(.007),cos(.006),lunar)*(1.-day)*(.72+.28*weather_noise(ray.xz*1800.));}
 if(clouds&&ray.y>.015){let at=origin.xz+ray.xz*max(0.,850.-origin.y)/max(ray.y,.015);let density=cloud_density(at);let toward_sun=cloud_density(at+sun.xz*100.);let silver=pow(max(solar,0.),24.)*(1.-density)*.35;let lit=mix(vec3f(.40,.46,.54),vec3f(.92,.94,.96),clamp(.60+(density-toward_sun)*1.3,0.,1.));let cloud=mix(vec3f(.018,.025,.041),lit+silver,day)*mix(1.,.72,g.weather.y);sky=mix(sky,cloud,(1.-exp(-density*3.2))*smoothstep(.015,.06,ray.y));}return sky;
}
fn terrain_height(p:vec2f)->f32 {let uv=clamp((p+2000.)/4000.,vec2f(.5/g.environment.z),vec2f(1.-.5/g.environment.z));let h=textureSampleLevel(heights,clamp_sampler,uv,0.);return (h.r*65280.+h.g*255.)/65535.*1024.-128.;}
fn transport_at(p:vec3f)->Probe {
 let size=u32(g.environment.z);let layers=u32(g.environment.w);let coordinate=clamp(vec3f((p.x+2000.)/4000.*f32(size)-.5,(p.y-terrain_height(p.xz))/64.*f32(layers)-.5,(p.z+2000.)/4000.*f32(size)-.5),vec3f(0.),vec3f(f32(size-1),f32(layers-1),f32(size-1)));let cell=vec3u(floor(coordinate));let f=fract(coordinate);var result=Probe(vec4f(0.),vec4f(0.),vec4f(0.),vec4f(0.),vec4f(0.));
 for(var z=0u;z<2u;z++){for(var y=0u;y<2u;y++){for(var x=0u;x<2u;x++){let v=min(cell+vec3u(x,y,z),vec3u(size-1,layers-1,size-1));let w=select(1.-f.x,f.x,x==1u)*select(1.-f.y,f.y,y==1u)*select(1.-f.z,f.z,z==1u);let a=probes[(v.z*layers+v.y)*size+v.x];result.bounce_sky+=a.bounce_sky*w;result.side+=a.side*w;result.bounce_x+=a.bounce_x*w;result.bounce_y+=a.bounce_y*w;result.bounce_z+=a.bounce_z*w;}}}
 let high=smoothstep(54.,64.,p.y-terrain_height(p.xz));result.bounce_sky=mix(result.bounce_sky,vec4f(0.,0.,0.,1.),high);result.side=mix(result.side,vec4f(1.),high);result.bounce_x*=1.-high;result.bounce_y*=1.-high;result.bounce_z*=1.-high;return result;
}
fn sunlight_bounce(p:Probe,light:vec3f)->vec3f {return max(vec3f(0.),p.bounce_sky.rgb*.25+p.bounce_x.rgb*light.x+p.bounce_y.rgb*light.y+p.bounce_z.rgb*light.z);}
fn sun_transport(p:Probe,light:vec3f)->f32 {let w=max(vec4f(light.x,-light.x,light.z,-light.z),vec4f(0.));let side=dot(p.side,w)/max(dot(w,vec4f(1.)),.001);return mix(side,p.bounce_sky.a,smoothstep(.3,.95,light.y));}
fn shadow_coordinate(p:vec3f,far:bool)->vec3f {var clip=g.light*vec4f(p,1.);if(far){clip=g.far_light*vec4f(p,1.);}let q=clip.xyz/clip.w;return vec3f(q.x,-q.y,q.z);}
fn receiver_slope(q:vec3f)->vec2f {let dx=dpdx(q);let dy=dpdy(q);let det=dx.x*dy.y-dx.y*dy.x;if(abs(det)<1e-10){return vec2f(0.);}return clamp(2.*vec2f(dy.y*dx.z-dx.y*dy.z,dx.x*dy.z-dy.x*dx.z)/det,vec2f(-4.),vec2f(4.));}
fn shadow_tile(q:vec3f,far:bool,bias:f32,slope:vec2f)->f32 {
 if(any(abs(q.xy)>vec2f(1.))||q.z<=0.||q.z>=1.){return 1.;}let uv=q.xy*.5+.5;var result=0.;let pixel=uv*2048.-.5;let f=fract(pixel);let base=(floor(pixel)+.5)/2048.;let wx=vec3f(3.-2.*f.x,5.,1.+2.*f.x);let wy=vec3f(3.-2.*f.y,5.,1.+2.*f.y);let ox=vec3f(-2.+(2.-f.x)/wx.x,(2.+f.x)/5.,2.+f.x/wx.z);let oy=vec3f(-2.+(2.-f.y)/wy.x,(2.+f.y)/5.,2.+f.y/wy.z);
 for(var y=0;y<3;y++){for(var x=0;x<3;x++){let tap=clamp(base+vec2f(ox[x],oy[y])/2048.,vec2f(.5/2048.),vec2f(1.-.5/2048.));let receiver=q.z+dot(slope,tap-uv)-bias;var lit=0.;if(far){lit=textureSampleCompareLevel(shadow_far,shadow_sampler,tap,receiver);}else{lit=textureSampleCompareLevel(shadow_near,shadow_sampler,tap,receiver);}result+=lit*wx[x]*wy[y];}}return result/81.;
}
fn shadows(p:vec3f,bias:f32,transport:Probe)->f32 {let near=shadow_coordinate(p,false);let far=shadow_coordinate(p,true);let ns=receiver_slope(near);let fslope=receiver_slope(far);let coarse=smoothstep(.82,1.,max(abs(far.x),abs(far.y)));let amount=smoothstep(.75,.95,max(abs(near.x),abs(near.y)));var nl=1.;if(amount<1.){nl=shadow_tile(near,false,bias,ns);}var fl=nl;if(amount>0.){fl=shadow_tile(far,true,bias*1.8,fslope);}return mix(mix(nl,fl,amount),sun_transport(transport,normalize(g.sun.xyz)),coarse);}
fn mapped_normal(p:vec3f,uv:vec2f,n:vec3f,detail:vec3f,strength:f32)->vec3f {let dp1=dpdx(p);let dp2=dpdy(p);let duv1=dpdx(uv);let duv2=dpdy(uv);let p1=cross(dp2,n);let p2=cross(n,dp1);let t=p1*duv1.x+p2*duv2.x;let b=p1*duv1.y+p2*duv2.y;let scale=inverseSqrt(max(max(dot(t,t),dot(b,b)),1e-12));return normalize(n*max(detail.z,.35)+(t*detail.x+b*detail.y)*scale*strength);}
fn shade_surface(albedo:vec3f,n:vec3f,view:vec3f,p:vec3f,kind:u32,rough:f32,ao:f32,transport:Probe,shadow:f32)->vec3f {
 let light=normalize(g.sun.xyz);let nl=max(dot(n,light),0.);let nv=max(dot(n,view),.001);let half_vector=normalize(light+view);let nh=max(dot(n,half_vector),0.);let vh=max(dot(view,half_vector),0.);let roughness=clamp(rough,.12,1.);let a=roughness*roughness;let a2=a*a;let denominator=nh*nh*(a2-1.)+1.;let distribution=a2/(3.14159265*denominator*denominator);let geometry=.5/max(nl*sqrt(nv*nv*(1.-a2)+a2)+nv*sqrt(nl*nl*(1.-a2)+a2),.001);let fresnel=vec3f(.035)+(1.-vec3f(.035))*pow(1.-vh,5.);var direct=((1.-fresnel)*albedo/3.14159265+distribution*geometry*fresnel)*nl;
 if(kind==1u||kind==2u||kind==3u){let forward_scatter=pow(max(dot(-light,view),0.),5.);var transmission=(.07+.22*forward_scatter)*(1.-nl*.7);if(kind!=2u){let back_light=max(dot(-n,light),0.);transmission=(.12+.30*forward_scatter)*(.35+.65*back_light);}direct+=albedo*vec3f(.80,1.,.58)*transmission/3.14159265;}
 let geo=clamp((p.xz+2000.)/4000.,vec2f(0.),vec2f(1.));let local=textureSampleLevel(local_ao,clamp_sampler,geo,0.);let cover=textureSampleLevel(canopy,smp,geo,0.).a;let grounded=select(1.-smoothstep(2.,24.,p.y-terrain_height(p.xz)),1.,kind==2u||kind==4u);let sky_visibility=mix(transport.bounce_sky.a,min(transport.bounce_sky.a,local.a*(1.-cover*.48)),grounded);let bent=normalize(local.rgb*2.-1.);let up=clamp(dot(n,mix(vec3f(0.,1.,0.),bent,grounded*.4))*.5+.5,0.,1.);let sky_diffuse=mix(vec3f(.12,.115,.095),vec3f(.25,.30,.37),up)*g.sky.x;var indirect=albedo*(sky_diffuse*mix(.38,1.,sky_visibility)*ao+transport.bounce_sky.rgb*g.sky.x*.65);let cloud=cloud_transmission(p,light);indirect+=albedo*sunlight_bounce(transport,light)*g.sun.w*g.sun_colour.rgb*cloud/3.14159265;let spec=atmosphere(reflect(-view,n),p,false,false)*.035*(1.-roughness*.72)*mix(.2,1.,sky_visibility);return indirect+spec+direct*shadow*cloud*g.sun.w*g.sun_colour.rgb;
}
