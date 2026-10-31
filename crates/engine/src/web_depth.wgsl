@group(0) @binding(0) var depth:texture_depth_multisampled_2d;
@vertex fn vs(@builtin(vertex_index) id:u32)->@builtin(position) vec4f {let p=array<vec2f,3>(vec2f(-1.,-1.),vec2f(3.,-1.),vec2f(-1.,3.));return vec4f(p[id],0.,1.);}
@fragment fn fs(@builtin(position) p:vec4f)->@builtin(frag_depth) f32 {var d=1.;for(var i=0;i<4;i++){d=min(d,textureLoad(depth,vec2i(p.xy),i));}return d;}
