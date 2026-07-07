vec2 ctv_curve(vec2 uv) {
    vec2 p = uv * 2.0 - 1.0;
    float r2 = dot(p, p);
    p *= 1.0 + r2 * 0.045;
    return p * 0.5 + 0.5;
}

float ctv_mask(vec2 fragCoord) {
    float grille = 0.94 + 0.06 * sin(fragCoord.x * 2.094395102);
    float scan = 0.93 + 0.07 * sin(fragCoord.y * 3.141592654);
    return grille * scan;
}

float ctv_hash(vec2 p) {
    return fract(sin(dot(p, vec2(41.23, 289.19))) * 45758.5453);
}

void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 uv = fragCoord.xy / iResolution.xy;
    vec2 curved = ctv_curve(uv);

    if (curved.x < 0.0 || curved.x > 1.0 || curved.y < 0.0 || curved.y > 1.0) {
        fragColor = vec4(0.006, 0.004, 0.012, 1.0);
        return;
    }

    vec2 px = vec2(1.0) / iResolution.xy;
    float edge = distance(uv, vec2(0.5));
    float vignette = smoothstep(0.86, 0.30, edge);
    float corner = smoothstep(0.03, 0.11, min(min(curved.x, 1.0 - curved.x), min(curved.y, 1.0 - curved.y)));

    float fringe = 1.15 + 0.25 * sin(iTime * 0.9);
    vec4 base = sampleChannel0(curved);
    vec3 color;
    color.r = sampleChannel0(curved + vec2(px.x * fringe, px.y * 0.25)).r;
    color.g = base.g;
    color.b = sampleChannel0(curved - vec2(px.x * fringe, px.y * 0.25)).b;

    float mask = ctv_mask(fragCoord);
    float noise = (ctv_hash(fragCoord.xy + floor(iTime * 60.0)) - 0.5) * 0.016;
    float glow = smoothstep(0.0, 0.9, max(max(color.r, color.g), color.b));

    color *= mask;
    color += color * glow * 0.055;
    color += vec3(0.016, 0.006, 0.035) * vignette;
    color += noise;
    color *= mix(0.70, 1.04, vignette * corner);

    fragColor = vec4(color, base.a);
}