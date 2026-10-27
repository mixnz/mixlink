// NV12 -> RGB, limited range, BT.601 or BT.709 per frame (specs 0013, 0019). Full-screen triangle from
// SV_VertexID; the viewport places it (aspect fit). uv_max crops the decoder's alignment padding.

cbuffer Frame : register(b0) {
    float2 uv_max;
    float2 pad;
    // R = y + coef.x * cr; G = y - coef.y * cb - coef.z * cr; B = y + coef.w * cb.
    float4 coef;
};

Texture2DArray<float> luma : register(t0);
Texture2DArray<float2> chroma : register(t1);
SamplerState linear_clamp : register(s0);

struct VsOut {
    float4 pos : SV_Position;
    float2 uv : TEXCOORD0;
};

VsOut vs_main(uint id : SV_VertexID) {
    float2 t = float2((id << 1) & 2, id & 2);
    VsOut o;
    o.pos = float4(t * float2(2, -2) + float2(-1, 1), 0, 1);
    o.uv = t * uv_max;
    return o;
}

float4 ps_main(VsOut i) : SV_Target {
    float y = (luma.Sample(linear_clamp, float3(i.uv, 0)) - 16.0 / 255.0) * (255.0 / 219.0);
    float2 c = (chroma.Sample(linear_clamp, float3(i.uv, 0)) - 128.0 / 255.0) * (255.0 / 224.0);
    float3 rgb = float3(
        y + coef.x * c.y,
        y - coef.y * c.x - coef.z * c.y,
        y + coef.w * c.x);
    return float4(saturate(rgb), 1);
}
