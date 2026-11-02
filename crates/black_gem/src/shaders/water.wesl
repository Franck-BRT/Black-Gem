// Eau : petites vagues qui déforment la normale (la lumière ondule), et effet de Fresnel
// (l'eau devient plus claire et opaque quand on la regarde en rasant).

import bevy_pbr::render::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::alpha_discard,
};

@if(PREPASS_PIPELINE)
import bevy_pbr::{
    prepass::io::{VertexOutput, FragmentOutput},
    deferred::functions::deferred_output,
};
@else
import bevy_pbr::render::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
};

// x : temps en secondes.
@group(constants::MATERIAL_BIND_GROUP) @binding(101)
var<uniform> wave: vec4<f32>;

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);

    let p = in.world_position.xz;
    let t = wave.x;
    let dx = cos(p.x * 0.9 + t * 1.1) * 0.07 + cos(p.x * 2.3 - p.y * 1.7 + t * 2.3) * 0.035;
    let dz = cos(p.y * 1.1 - t * 0.9) * 0.07 + cos(p.y * 2.9 + p.x * 1.3 + t * 1.9) * 0.035;
    pbr_input.N = normalize(pbr_input.N + vec3<f32>(dx, 0.0, dz));

    let fresnel = pow(1.0 - max(dot(pbr_input.N, pbr_input.V), 0.0), 4.0);
    let base = pbr_input.material.base_color;
    pbr_input.material.base_color = vec4<f32>(
        mix(base.rgb, vec3<f32>(0.55, 0.78, 0.92), fresnel * 0.6),
        mix(base.a, 1.0, fresnel),
    );
    pbr_input.material.base_color = alpha_discard(
        pbr_input.material.flags,
        pbr_input.material.alpha_cutoff,
        pbr_input.material.base_color
    );

    var out: FragmentOutput;
@if(PREPASS_PIPELINE) {
    out = deferred_output(in, pbr_input);
} @else {
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
}
    return out;
}
