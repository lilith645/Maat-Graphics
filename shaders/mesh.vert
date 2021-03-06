#version 450
#extension GL_ARB_separate_shader_objects : enable
#extension GL_ARB_shading_language_420pack : enable

layout (location = 0) in vec3 pos;
layout (location = 1) in vec3 normal;
layout (location = 2) in vec2 uv;
layout (location = 3) in vec3 colour;

layout (location = 0) out vec3 o_normal;
layout (location = 1) out vec3 o_colour;
layout (location = 2) out vec2 o_uv;
layout (location = 3) out vec3 o_view_vec;
layout (location = 4) out vec3 o_light_vec;

layout (set = 0, binding = 0) uniform UBO {
  mat4 projection;
  mat4 view;
  vec4 light_pos;
} ubo;

layout(push_constant) uniform PushConstants {
  mat4 model;
  vec4 attrib0;
  vec4 attrib1; 
  vec4 attrib2;
  vec4 attrib3;
} push_constants;

void main() {
  o_normal = normal;
  o_colour = colour;
  o_uv = uv;
  
  gl_Position = ubo.projection * ubo.view * push_constants.model * vec4(pos.xyz, 1.0);
  
  vec4 pos = ubo.view * vec4(pos, 1.0);
  o_normal = mat3(ubo.view) * normal;
  vec3 l_pos = mat3(ubo.view) * ubo.light_pos.xyz;
  o_light_vec = l_pos - pos.xyz;
  o_view_vec = -pos.xyz;
}
