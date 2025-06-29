#version 330 core

// input variables: Vertex to Phragment
in vec2 v2p_tex_coords;
in vec4 v2p_tint;
in float v2p_omit_texture;

out vec4 final_color;

uniform sampler2D text;

void main()
{
  vec4 sample = vec4(1.0, 1.0, 1.0, 1.0);
  if (v2p_omit_texture < 1) {
      sample = vec4(1.0, 1.0, 1.0, texture(text, v2p_tex_coords).r);
  }
  final_color = v2p_tint * sample;
}
