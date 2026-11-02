// Ciel : dégradé selon la hauteur du soleil, halo et disque solaire, étoiles scintillantes
// la nuit, et une géante gazeuse à bandes suspendue dans le ciel (clin d'œil à Dyson Sphere
// Program). Tout est calculé ici : aucune texture, compatible WebGL2.

import bevy_pbr::render::{forward_io::VertexOutput, mesh_view_bindings::view};

struct Sky {
    // xyz : direction du soleil ; w : visibilité du soleil (0 la nuit).
    sun: vec4<f32>,
    // rgb : couleur du zénith ; w : intensité de la nuit (étoiles).
    zenith: vec4<f32>,
    // rgb : couleur de l'horizon ; w : temps, en secondes.
    horizon: vec4<f32>,
    // rgb : couleur du soleil ; w : inutilisé.
    sun_color: vec4<f32>,
};

@group(constants::MATERIAL_BIND_GROUP) @binding(0)
var<uniform> sky: Sky;

fn hash(p: vec3<f32>) -> f32 {
    return fract(sin(dot(p, vec3<f32>(12.9898, 78.233, 37.719))) * 43758.5453);
}

// Géante gazeuse : bandes ondulées, éclairée par le soleil, assombrie au bord.
fn gas_giant(dir: vec3<f32>, background: vec3<f32>) -> vec3<f32> {
    let center = normalize(vec3<f32>(-0.6, 0.2, -0.78));
    let radius = 0.2;
    let c = dot(dir, center);
    if (c < cos(radius)) {
        return background;
    }
    let offset = dir - center * c;
    let east = normalize(cross(vec3<f32>(0.2, 1.0, 0.0), center));
    let north = cross(center, east);
    let u = dot(offset, east) / sin(radius);
    let v = dot(offset, north) / sin(radius);
    let r = sqrt(u * u + v * v);
    let z = sqrt(max(1.0 - r * r, 0.0));
    let normal = normalize(east * u + north * v - center * z);
    let lit = max(dot(normal, normalize(sky.sun.xyz)), 0.0);

    let swirl = sin(u * 4.0 + v * 9.0) * 0.6;
    let band = sin(v * 16.0 + swirl);
    let amber = vec3<f32>(0.85, 0.55, 0.3);
    let teal = vec3<f32>(0.25, 0.55, 0.62);
    let cream = vec3<f32>(0.92, 0.86, 0.72);
    var surface = mix(amber, teal, band * 0.5 + 0.5);
    surface = mix(surface, cream, smoothstep(0.7, 1.0, sin(v * 31.0 + swirl * 2.0)) * 0.6);

    let shade = 0.06 + 1.3 * lit;
    var color = surface * shade * (0.55 + 0.45 * z);
    // Fine atmosphère au bord, du côté éclairé.
    color += vec3<f32>(0.4, 0.6, 1.0) * pow(1.0 - z, 3.0) * (0.2 + lit) * 0.6;
    // De jour, la planète se fond un peu dans le bleu du ciel.
    color = mix(color, background, 0.25 * sky.sun.w);
    return mix(background, color, smoothstep(1.0, 0.97, r));
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let dir = normalize(in.world_position.xyz - view.world_position);
    let h = dir.y;
    let night = sky.zenith.w;

    var color = mix(sky.horizon.rgb, sky.zenith.rgb, smoothstep(-0.02, 0.32, h));
    // Sous l'horizon : une brume qui s'assombrit.
    if (h < 0.0) {
        color = mix(sky.horizon.rgb, sky.horizon.rgb * 0.3, smoothstep(0.0, -0.3, h));
    }

    // Étoiles : une cellule sur ~300 s'allume, et scintille.
    if (night > 0.0 && h > 0.0) {
        let scaled = dir * 230.0;
        let cell = floor(scaled);
        let r = hash(cell);
        if (r > 0.9968) {
            let d = length(fract(scaled) - vec3<f32>(0.5));
            let twinkle = 0.55 + 0.45 * sin(sky.horizon.w * (1.5 + r * 6.0) + r * 120.0);
            let tint = mix(vec3<f32>(0.8, 0.88, 1.0), vec3<f32>(1.0, 0.85, 0.7), hash(cell + 7.0));
            color += tint * smoothstep(0.4, 0.0, d) * twinkle * night * 5.0 * smoothstep(0.0, 0.25, h);
        }
    }

    color = gas_giant(dir, color);

    // Soleil : grand halo, halo serré, puis disque très lumineux (il fait briller le bloom).
    let sd = max(dot(dir, normalize(sky.sun.xyz)), 0.0);
    color += sky.sun_color.rgb * (pow(sd, 5.0) * 0.3 + pow(sd, 90.0) * 1.2) * sky.sun.w;
    color += sky.sun_color.rgb * smoothstep(0.99935, 0.99965, sd) * 40.0 * sky.sun.w;

    return vec4<f32>(color, 1.0);
}
