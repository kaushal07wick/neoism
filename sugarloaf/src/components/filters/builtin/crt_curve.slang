#version 450

// Built-in preset: Ghostty-style CRT curve geometry pass.

layout(push_constant) uniform Push
{
    vec4 SourceSize;
    vec4 OriginalSize;
    vec4 OutputSize;
    uint FrameCount;
} params;

layout(std140, set = 0, binding = 0) uniform UBO
{
    mat4 MVP;
} global;

#pragma stage vertex
layout(location = 0) in vec4 Position;
layout(location = 1) in vec2 TexCoord;
layout(location = 0) out vec2 vTexCoord;

void main()
{
    gl_Position = global.MVP * Position;
    vTexCoord = TexCoord;
}

#pragma stage fragment
layout(location = 0) in vec2 vTexCoord;
layout(location = 0) out vec4 FragColor;
layout(set = 0, binding = 2) uniform sampler2D Source;

vec2 crtWarp(vec2 uv, float amount)
{
    vec2 p = uv * 2.0 - 1.0;
    float r2 = dot(p, p);
    p *= 1.0 + amount * r2;
    return p * 0.5 + 0.5;
}

vec2 overscan(vec2 uv, float zoom)
{
    return (uv - 0.5) * zoom + 0.5;
}

float curvedFalloff(vec2 warped_uv)
{
    vec2 d = abs(warped_uv * 2.0 - 1.0);
    float m = max(d.x, d.y);
    return 1.0 - smoothstep(1.0, 1.2, m);
}

void main()
{
    vec2 warped = crtWarp(vTexCoord, 0.075);
    vec2 sample_uv = clamp(overscan(warped, 0.94), vec2(0.001), vec2(0.999));

    vec3 color = texture(Source, sample_uv).rgb;

    float vignette = 16.0 * sample_uv.x * sample_uv.y * (1.0 - sample_uv.x) * (1.0 - sample_uv.y);
    vignette = 0.97 + 0.03 * pow(clamp(vignette, 0.0, 1.0), 0.22);

    float curve = curvedFalloff(warped);

    color *= vignette;
    color *= mix(0.96, 1.0, curve);

    FragColor = vec4(clamp(color, 0.0, 1.0), 1.0);
}
