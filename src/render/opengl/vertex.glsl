// input variables: Content to Vertex
in vec4 c2v_dst_rect;
in vec4 c2v_src_rect;
in vec4 c2v_color_0;
in vec4 c2v_color_1;
in vec4 c2v_color_2;
in vec4 c2v_color_3;
in vec4 c2v_extra;

// output variables: Vertex to Phragment
out vec2 v2p_tex_coords;
out vec4 v2p_tint;
out float v2p_omit_texture;
out vec2 v2p_dst_pos_px;
out vec4 v2p_dst_rect_px;
out float v2p_corner_radius_px;

uniform vec2 u_viewport_size_px;

void main() {
  vec2 vertices[] = vec2[](vec2(-1, -1), vec2(-1, +1), vec2(+1, -1), vec2(+1, +1));

  // xarkes: compute destination coords
  vec2 dst_half_size = (c2v_dst_rect.zw - c2v_dst_rect.xy) / 2.;
  vec2 dst_center    = (c2v_dst_rect.zw + c2v_dst_rect.xy) / 2.;
  vec2 dst_position  = vertices[gl_VertexID] * dst_half_size + dst_center;

  // xarkes: compute texture coords
  vec2 src_half_size = (c2v_src_rect.zw - c2v_src_rect.xy) / 2.;
  vec2 src_center    = (c2v_src_rect.zw + c2v_src_rect.xy) / 2.;
  vec2 src_position  = vertices[gl_VertexID] * src_half_size + src_center;

  // xarkes: find color
  vec4 colors[] = vec4[](c2v_color_0, c2v_color_1, c2v_color_2, c2v_color_3);
  vec4 color = colors[0];

  // xarkes: output values
  gl_Position = vec4(2. * dst_position.x / u_viewport_size_px.x - 1.,
                     2. * (1. - dst_position.y / u_viewport_size_px.y) - 1.,
                     0.0, 1.0);
  v2p_tex_coords = src_position;
  v2p_tint = color;
  v2p_omit_texture = c2v_extra.x;
  v2p_dst_pos_px = dst_position;
  v2p_dst_rect_px = c2v_dst_rect;
  v2p_corner_radius_px = c2v_extra.y;
}
