#version 450

layout(location = 0) in vec2 position;
layout(location = 1) in vec2 uv;

void main() {
  gl_Position = uniforms.projection * uniforms.model * vec4(position, 0.0, 1.0);
}
