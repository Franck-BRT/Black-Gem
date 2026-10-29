// Vertex shader principal des objets du monde : identique à celui de Bevy
// (bevy_pbr/render/mesh.wesl), plus la courbure de l'horizon (voir `curvature.rs`).

import bevy_pbr::render::{
    mesh_bindings::mesh,
    mesh_functions,
    skinning,
    morph,
    morph::{morph_position, morph_normal, morph_tangent},
    forward_io::{Vertex, UncompressedVertex, VertexOutput, decompress_vertex},
    view_transformations::position_world_to_clip,
};

struct Curvature {
    // xy : point visé par la caméra (x, z du monde) ; z : force de la courbure ; w : inutilisé.
    params: vec4<f32>,
};

@group(constants::MATERIAL_BIND_GROUP) @binding(100)
var<uniform> curvature: Curvature;

// Le sol « tombe » en s'éloignant du point visé : y -= k * distance².
fn bend_position(p: vec3<f32>) -> vec3<f32> {
    let d = p.xz - curvature.params.xy;
    return vec3<f32>(p.x, p.y - curvature.params.z * dot(d, d), p.z);
}

// Les normales suivent la pente de la courbure, pour que la lumière reste juste.
fn bend_normal(p: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
    let g = 2.0 * curvature.params.z * (p.xz - curvature.params.xy);
    return normalize(vec3<f32>(n.x + g.x * n.y, n.y - g.x * n.x - g.y * n.z, n.z + g.y * n.y));
}

@if(MORPH_TARGETS)
fn morph_vertex(vertex_in: UncompressedVertex, instance_index: u32) -> UncompressedVertex {
    var vertex = vertex_in;
    let first_vertex = mesh[instance_index].first_vertex_index;
    let vertex_index = vertex.index - first_vertex;
    let morph_descriptor_index = mesh[instance_index].morph_descriptor_index;

    let weight_count = morph::layer_count(morph_descriptor_index);
    for (var i: u32 = 0u; i < weight_count; i ++) {
        let weight = morph::weight_at(i, morph_descriptor_index);
        if weight == 0.0 {
            continue;
        }
        vertex.position += weight * morph_position(vertex_index, i, morph_descriptor_index);
        @if(VERTEX_NORMALS)
        vertex.normal += weight * morph_normal(vertex_index, i, morph_descriptor_index);
        @if(VERTEX_TANGENTS)
        vertex.tangent += vec4(weight * morph_tangent(vertex_index, i, morph_descriptor_index), 0.0);
    }
    return vertex;
}

@vertex
fn vertex(vertex_no_morph: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let uncompressed_vertex_no_morph = decompress_vertex(vertex_no_morph, vertex_no_morph.instance_index);
@if(MORPH_TARGETS)
    var vertex = morph_vertex(uncompressed_vertex_no_morph, vertex_no_morph.instance_index);
@else
    var vertex = uncompressed_vertex_no_morph;

    let mesh_world_from_local = mesh_functions::get_world_from_local(vertex_no_morph.instance_index);

@if(SKINNED)
    var world_from_local = skinning::skin_model(
        vertex.joint_indices,
        vertex.joint_weights,
        vertex_no_morph.instance_index
    );
@else
    var world_from_local = mesh_world_from_local;

@if(VERTEX_POSITIONS)
    let flat_world_position = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(vertex.position, 1.0));

@if(VERTEX_NORMALS && SKINNED)
    out.world_normal = bend_normal(flat_world_position.xyz, skinning::skin_normals(world_from_local, vertex.normal));
@if(VERTEX_NORMALS && !SKINNED)
    out.world_normal = bend_normal(flat_world_position.xyz, mesh_functions::mesh_normal_local_to_world(
        vertex.normal,
        vertex_no_morph.instance_index
    ));

@if(VERTEX_POSITIONS)
    out.world_position = vec4<f32>(bend_position(flat_world_position.xyz), flat_world_position.w);
@if(VERTEX_POSITIONS)
    out.position = position_world_to_clip(out.world_position.xyz);

@if(VERTEX_UVS_A)
    out.uv = vertex.uv;
@if(VERTEX_UVS_B)
    out.uv_b = vertex.uv_b;

@if(VERTEX_TANGENTS)
    out.world_tangent = mesh_functions::mesh_tangent_local_to_world(
        world_from_local,
        vertex.tangent,
        vertex_no_morph.instance_index
    );

@if(VERTEX_COLORS)
    out.color = vertex.color;

@if(VERTEX_OUTPUT_INSTANCE_INDEX)
    out.instance_index = vertex_no_morph.instance_index;

@if(VISIBILITY_RANGE_DITHER)
    out.visibility_range_dither = mesh_functions::get_visibility_range_dither_level(
        vertex_no_morph.instance_index, mesh_world_from_local[3]);

    return out;
}
