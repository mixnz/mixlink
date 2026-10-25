// NV12 -> RGB, BT.709 limited range (spec 0013). Full-screen triangle from SV_VertexID; the
// viewport places it (aspect fit). uv_max crops the decoder's alignment padding.

cbuffer Crop : register(b0) {
    float2 uv_max;
    float2 pad;
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
        y + 1.5748 * c.y,
        y - 0.1873 * c.x - 0.4681 * c.y,
        y + 1.8556 * c.x);
    return float4(saturate(rgb), 1);
}
