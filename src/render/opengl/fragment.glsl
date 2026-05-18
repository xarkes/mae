// input variables: Vertex to Phragment
in vec2 v2p_tex_coords;
in vec4 v2p_tint;
in float v2p_omit_texture;
in vec2 v2p_dst_pos_px;
in vec4 v2p_dst_rect_px;
in float v2p_corner_radius_px;

out vec4 final_color;

uniform sampler2D text;

float rounded_rect_sdf(vec2 p, vec4 rect, float r) {
  vec2 half_size = (rect.zw - rect.xy) * 0.5;
  vec2 center = (rect.zw + rect.xy) * 0.5;
  vec2 q = abs(p - center) - half_size + vec2(r, r);
  return length(max(q, vec2(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - r;
}

void main()
{
  if (v2p_corner_radius_px > 0.0) {
      float max_r = min(v2p_dst_rect_px.z - v2p_dst_rect_px.x,
                        v2p_dst_rect_px.w - v2p_dst_rect_px.y) * 0.5;
      float r = min(v2p_corner_radius_px, max_r);
      if (rounded_rect_sdf(v2p_dst_pos_px, v2p_dst_rect_px, r) > 0.0) {
          discard;
      }
  }

  vec4 bsample = vec4(1.0, 1.0, 1.0, 1.0);
  if (v2p_omit_texture < 1.) {
      bsample = vec4(1.0, 1.0, 1.0, texture(text, v2p_tex_coords).r);
  }
  final_color = v2p_tint * bsample;
}
