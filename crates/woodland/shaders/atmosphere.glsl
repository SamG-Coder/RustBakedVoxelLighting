// Shared linear-radiance atmosphere. Cloud coordinates are metres, shared by sky and shadows.
float weatherHash(vec2 p){uvec2 q=uvec2(ivec2(p));uint h=q.x*374761393u+q.y*668265263u;h=(h^(h>>13u))*1274126177u;return float(h^(h>>16u))/4294967296.;}
float weatherNoise(vec2 p){vec2 i=floor(p),f=fract(p);f=f*f*(3.-2.*f);return mix(mix(weatherHash(i),weatherHash(i+vec2(1,0)),f.x),mix(weatherHash(i+vec2(0,1)),weatherHash(i+1.),f.x),f.y);}
float cloudDensity(vec2 metres){
 vec2 q=metres*.0018+vec2(globals.camera_time.w*.004,globals.camera_time.w*.0013);
 float broad=weatherNoise(q*.43);
 float shape=weatherNoise(q)*.56+weatherNoise(q*2.07+13.)*.28+weatherNoise(q*4.13-7.)*.16;
 return smoothstep(.48-globals.weather.y*.10,.72,shape+broad*.12);
}
float cloudTransmission(vec3 p,vec3 light){
 if(light.y<.035)return 1.;
 vec2 at=p.xz+light.xz*max(0.,850.-p.y)/max(light.y,.035);
 return exp(-cloudDensity(at)*mix(.48,1.15,globals.weather.y));
}
vec3 atmosphereRadiance(vec3 ray,vec3 origin,bool clouds,bool celestial){
 float h=max(ray.y,0.),day=globals.sky.z;
 float phase=(globals.sky.w-6.)/24.*6.283185307;
 vec3 sun=normalize(vec3(cos(phase)*.6,sin(phase),cos(phase)*.8));
 vec3 night=mix(vec3(.008,.014,.025),vec3(.001,.003,.012),sqrt(h));
 vec3 daytime=mix(vec3(.46,.62,.75),vec3(.045,.19,.43),sqrt(h));
 float dusk=(1.-smoothstep(.0,.28,abs(sun.y)))*pow(max(dot(ray,normalize(vec3(sun.x,.05,sun.z))),0.),6.);
 daytime=mix(daytime,vec3(.72,.27,.09),dusk*.55);
 vec3 sky=mix(night,daytime,day);
 float solar=dot(ray,sun),lunar=dot(ray,-sun);
 if(celestial){
  sky+=vec3(1.,.83,.52)*smoothstep(cos(.005),cos(.0035),solar)*day*6.;
  sky+=vec3(.65,.73,.9)*smoothstep(cos(.007),cos(.006),lunar)*(1.-day)*(.72+.28*weatherNoise(ray.xz*1800.));
 }
 if(clouds && ray.y>.015){
  vec2 at=origin.xz+ray.xz*max(0.,850.-origin.y)/max(ray.y,.015);
  float density=cloudDensity(at);
  float towardSun=cloudDensity(at+sun.xz*100.);
  float silver=pow(max(solar,0.),24.)*(1.-density)*.35;
  vec3 lit=mix(vec3(.40,.46,.54),vec3(.92,.94,.96),clamp(.60+(density-towardSun)*1.3,0.,1.));
  vec3 cloud=mix(vec3(.018,.025,.041),lit+silver,day)*mix(1.,.72,globals.weather.y);
  sky=mix(sky,cloud,(1.-exp(-density*3.2))*smoothstep(.015,.06,ray.y));
 }
 return sky;
}
vec3 atmosphereRadiance(vec3 ray,vec3 origin,bool clouds){return atmosphereRadiance(ray,origin,clouds,true);}
