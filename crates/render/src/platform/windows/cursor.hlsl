// Host cursor quad (spec 0020): a 4-vertex strip from SV_VertexID over `rect` (NDC x0, y0, x1, y1),
// sampling the shape with point filtering (sharp edges). Blending is set by the pass: premultiplied
// alpha for the image, "invert below" for the XOR mask.

cbuffer Quad : register(b0) {
    float4 rect;
};

Texture2D<float4> shape : register(t0);
SamplerState point_clamp : register(s0);

struct VsOut {
    float4 pos : SV_Position;
    float2 uv : TEXCOORD0;
};

VsOut vs_main(uint id : SV_VertexID) {
    float2 t = float2(id & 1, id >> 1);
    VsOut o;
    o.pos = float4(lerp(rect.xy, rect.zw, t), 0, 1);
    o.uv = t;
    return o;
}

float4 ps_main(VsOut i) : SV_Target {
    return shape.Sample(point_clamp, i.uv);
}
