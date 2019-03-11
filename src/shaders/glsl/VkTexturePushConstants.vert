#version 450

layout(location = 0) in vec2 position;
layout(location = 1) in vec2 uv;

layout(location = 0) out vec2 uvs;
layout(location = 1) out vec4 new_colour;
layout(location = 2) out float use_texture;

// 128 bytes, float 4 bytes
layout(push_constant) uniform PushConstants {
  vec4 model; // position.x, position.y, scale x, scale y
  vec4 colour;
  vec4 sprite_sheet; // block_x, block_y, num_of_rows, image_scale?
  vec4 projection_details;
} push_constants;

mat4 create_translation_matrix(vec2 pos, vec2 scale) {
  mat4 translate_matrix = mat4(vec4(scale.x, 0.0,   0.0, 0.0), 
                               vec4(0.0,   scale.y, 0.0, 0.0), 
                               vec4(0.0,   0.0,   1.0, 0.0), 
                               vec4(pos,          0.0, 1.0));
  
  return translate_matrix;
}

mat4 create_ortho_projection(float near, float far, float right, float bottom, vec2 offset) {
  float left = offset.x;
  float top = offset.y;
  right += left;
  bottom += top;
  
  mat4 ortho = mat4(vec4(2.0 / (right - left), 0.0, 0.0, 0.0),
                    vec4(0.0, 2.0 / (top - bottom), 0.0, 0.0),
                    vec4(0.0, 0.0, -2.0 / (near / far), 0.0),
                    vec4(-(right + left) / (right - left), -(top+bottom)/(top-bottom), 0.0, 1.0));
  
  return ortho;
}

void main() {
  float num_rows = push_constants.sprite_sheet.z;
  float block_x = push_constants.sprite_sheet.x;
  float block_y = push_constants.sprite_sheet.y;
  
  if (num_rows < 0.0) {
    num_rows *= -1;
  }
  
  vec2 texcoords = uv.xy;
  texcoords += vec2(block_x, block_y);
  texcoords /= num_rows;
  uvs = texcoords;
  
  new_colour = push_constants.colour;
  use_texture = push_constants.sprite_sheet.z;
  
  //mat4 scale_matrix = mat4(vec4(scale, 0.0, 0.0, 0.0), 
  //                         vec4(0.0, scale, 0.0, 0.0), 
  //                         vec4(0.0, 0.0, scale, 0.0), 
  //                         vec4(0.0, 0.0, 0.0, 1.0));
  
  float x_offset = push_constants.projection_details.x;
  float y_offset = push_constants.projection_details.y;
  float right = push_constants.projection_details.z;
  float bottom = push_constants.projection_details.w;
  mat4 projection = create_ortho_projection(1.0, -1.0, right, bottom, vec2(0.0, 0.0));
  mat4 model = create_translation_matrix(push_constants.model.xy, push_constants.model.zw);
  
  gl_Position = projection * model * vec4(position, 0.0, 1.0);
}
