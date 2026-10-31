layout(set=0,binding=0) uniform Globals {mat4 view_projection;mat4 light_projection;mat4 far_light_projection;mat4 inverse_view_projection;vec4 camera_time;vec4 sun;vec4 sky;vec4 environment;vec4 weather;vec4 sun_colour;uvec4 geography_maps;} globals;
struct Material {uvec4 textures;vec4 settings;vec4 tint;uvec4 extra;};
layout(std430,set=0,binding=2) readonly buffer Materials {Material items[];} materials;
layout(push_constant) uniform DrawInfo {uint material;uint pass;uint unused0;uint unused1;} draw;
