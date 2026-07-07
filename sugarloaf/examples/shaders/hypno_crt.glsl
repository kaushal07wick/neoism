vec2 hypno_curve(vec2 uv) {
    uv = uv * 2.0 - 1.0;
    uv *= 1.0 + dot(uv, uv) * 0.035;
    return uv * 0.5 + 0.5;
}

float hypno_noise(vec2 p) {
    return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453123);
}

void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 uv = fragCoord.xy / iResolution.xy;
    vec2 curved = hypno_curve(uv);

    if (curved.x < 0.0 || curved.x > 1.0 || curved.y < 0.0 || curved.y > 1.0) {
        fragColor = vec4(0.015, 0.006, 0.03, 1.0);
        return;
    }

    vec2 px = vec2(1.0) / iResolution.xy;
    float aberration = 1.25 + 0.5 * sin(iTime * 0.7);
    vec4 base = sampleChannel0(curved);
    vec3 color;
    color.r = sampleChannel0(curved + vec2(px.x * aberration, 0.0)).r;
    color.g = base.g;
    color.b = sampleChannel0(curved - vec2(px.x * aberration, 0.0)).b;

    float scanline = 0.94 + 0.06 * sin(fragCoord.y * 3.14159265);
    float grille = 0.96 + 0.04 * sin(fragCoord.x * 2.0943951);
    float vignette = smoothstep(0.86, 0.25, distance(uv, vec2(0.5)));
    float shimmer = hypno_noise(fragCoord.xy + iTime * 60.0) * 0.018;

    color *= scanline * grille;
    color += vec3(0.02, 0.0, 0.035) * vignette;
    color += shimmer;
    color *= mix(0.72, 1.04, vignette);

    fragColor = vec4(color, base.a);
}