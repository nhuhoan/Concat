struct Params { progress: f32, dim: f32, softness: f32 }

// A caption that lights up word by word as it is spoken. `reveal_order` gives
// each pixel its word's place in the line, 0..1, baked from the title's own
// layout; `progress` is how far the playhead has swept across those words,
// keyed from when each is said. A word already reached plays at full strength;
// one not yet reached is held back to `dim`, fading up over `softness` as the
// playhead crosses it.
//
// Off a title there is no reveal map, so `reveal_order` reads 0 everywhere and
// every pixel counts as already reached: this is then a no-op, which is what
// the reveal contract intends and what keeps its pinned probes an identity.
fn effect(uv: vec2<f32>) -> vec4<f32> {
    let c = sample(uv);
    let order = reveal_order(uv);
    let edge = max(params.softness, 0.001);
    let lit = smoothstep(order - edge, order, params.progress);
    let strength = mix(params.dim, 1.0, lit);
    return vec4<f32>(c.rgb, c.a * strength);
}
