// Fragment des objets du monde : l'éclairage standard de Bevy, assombri par le brouillard
// de guerre (texture d'une valeur par case : 1 visible, ~0,45 déjà vu, presque 0 inconnu).

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

@group(constants::MATERIAL_BIND_GROUP) @binding(102)
var fog_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(103)
var fog_sampler: sampler;
// x : demi-côté de la carte ; y : 1 / côté ; z : 1 si le brouillard est actif.
@group(constants::MATERIAL_BIND_GROUP) @binding(104)
var<uniform> fog: vec4<f32>;

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color = alpha_discard(
        pbr_input.material.flags,
        pbr_input.material.alpha_cutoff,
        pbr_input.material.base_color
    );
    let uv = (in.world_position.xz + vec2<f32>(fog.x)) * fog.y;
    let seen = textureSample(fog_texture, fog_sampler, uv).r;
    let light = select(1.0, seen, fog.z > 0.5);

    var out: FragmentOutput;
@if(PREPASS_PIPELINE) {
    out = deferred_output(in, pbr_input);
} @else {
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    out.color = vec4<f32>(out.color.rgb * light + vec3<f32>(0.004, 0.002, 0.01) * (1.0 - light), out.color.a);
}
    return out;
}
