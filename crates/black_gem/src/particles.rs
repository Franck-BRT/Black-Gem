//! Particules : étincelles, éclats, poussière, flammes. Purement visuelles (calculées sur le
//! processeur, sans shader de calcul : elles fonctionnent aussi en WebGL2).

use bevy::{light::NotShadowCaster, prelude::*};
use gem_sim::GemColor;

use crate::{
    curvature::{CurvedMaterial, curved},
    palette::{gem_glow, gem_rgb},
    world::PlanetEntity,
};

/// Nombre maximal de particules en vie (au-delà, les nouvelles sont ignorées).
const MAX_PARTICLES: usize = 900;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    /// Étincelle d'impact ou de foreuse (couleur de gemme, ou blanc-bleu sans gemme).
    Spark(Option<GemColor>),
    /// Éclats de Corruption à la mort d'un ennemi.
    Corruption,
    /// Mort d'un boss : gros éclats rouges.
    Boss,
    /// Poussière du Grand foreur.
    Dust,
    /// Flammes des réacteurs (capsule, golems).
    Flame,
}

/// Demande d'émission de particules.
#[derive(Message, Clone, Copy, Debug)]
pub struct Burst {
    pub at: Vec3,
    pub style: Style,
    pub count: u32,
}

impl Burst {
    pub fn new(at: Vec3, style: Style, count: u32) -> Self {
        Self { at, style, count }
    }
}

#[derive(Component)]
struct Particle {
    velocity: Vec3,
    life: f32,
    lifetime: f32,
    size: f32,
    gravity: f32,
}

#[derive(Resource)]
struct ParticleKit {
    mesh: Handle<Mesh>,
    sparks: [Handle<CurvedMaterial>; 5],
    plain_spark: Handle<CurvedMaterial>,
    corruption: Handle<CurvedMaterial>,
    boss: Handle<CurvedMaterial>,
    dust: Handle<CurvedMaterial>,
    flame: Handle<CurvedMaterial>,
}

pub struct ParticlesPlugin;

impl Plugin for ParticlesPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Burst>()
            .add_systems(Startup, create_kit)
            .add_systems(PostUpdate, (emit, animate).chain());
    }
}

fn create_kit(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<CurvedMaterial>>,
) {
    let mut glow = |color: Vec3, emissive: LinearRgba| {
        materials.add(curved(StandardMaterial {
            base_color: Color::srgb(color.x, color.y, color.z),
            emissive,
            unlit: true,
            ..default()
        }))
    };
    let kit = ParticleKit {
        mesh: meshes.add(Sphere::new(0.5).mesh().ico(0).unwrap()),
        sparks: GemColor::ALL.map(|c| glow(gem_rgb(c), gem_glow(c, 8.0))),
        plain_spark: glow(Vec3::new(0.8, 0.95, 1.0), LinearRgba::rgb(3.0, 5.0, 7.0)),
        corruption: glow(Vec3::new(0.6, 0.1, 0.5), LinearRgba::rgb(5.0, 0.4, 3.0)),
        boss: glow(Vec3::new(1.0, 0.2, 0.2), LinearRgba::rgb(9.0, 0.8, 0.6)),
        dust: glow(Vec3::new(0.35, 0.25, 0.4), LinearRgba::rgb(0.4, 0.15, 0.6)),
        flame: glow(Vec3::new(1.0, 0.6, 0.2), LinearRgba::rgb(9.0, 4.0, 0.8)),
    };
    commands.insert_resource(kit);
}

/// Générateur pseudo-aléatoire minimal (l'aspect des particules n'a pas besoin d'être
/// déterministe : il n'influence pas la partie).
struct Random(u32);

impl Random {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 % 10_000) as f32 / 10_000.0
    }

    fn signed(&mut self) -> f32 {
        self.next() * 2.0 - 1.0
    }
}

fn emit(
    mut commands: Commands,
    mut bursts: MessageReader<Burst>,
    kit: Res<ParticleKit>,
    particles: Query<(), With<Particle>>,
    mut seed: Local<u32>,
) {
    if *seed == 0 {
        *seed = 0x9E37_79B9;
    }
    let mut random = Random(*seed);
    let mut alive = particles.iter().count();
    for burst in bursts.read() {
        // (vitesse, durée de vie, taille, gravité, dispersion verticale)
        let (speed, lifetime, size, gravity, upward) = match burst.style {
            Style::Spark(_) => (4.0, 0.45, 0.09, 9.0, 0.6),
            Style::Corruption => (3.0, 0.8, 0.16, 6.0, 0.8),
            Style::Boss => (5.0, 1.3, 0.3, 5.0, 1.0),
            Style::Dust => (0.8, 1.6, 0.22, -0.4, 0.3),
            Style::Flame => (1.2, 0.5, 0.18, -2.0, -1.0),
        };
        let material = match burst.style {
            Style::Spark(Some(color)) => kit.sparks[color.index()].clone(),
            Style::Spark(None) => kit.plain_spark.clone(),
            Style::Corruption => kit.corruption.clone(),
            Style::Boss => kit.boss.clone(),
            Style::Dust => kit.dust.clone(),
            Style::Flame => kit.flame.clone(),
        };
        for _ in 0..burst.count {
            if alive >= MAX_PARTICLES {
                break;
            }
            alive += 1;
            let direction = Vec3::new(
                random.signed(),
                random.next() * upward + 0.2,
                random.signed(),
            )
            .normalize_or(Vec3::Y);
            let velocity = direction * speed * (0.5 + random.next());
            let lifetime = lifetime * (0.6 + 0.8 * random.next());
            commands.spawn((
                PlanetEntity,
                Particle {
                    velocity,
                    life: lifetime,
                    lifetime,
                    size: size * (0.6 + 0.8 * random.next()),
                    gravity,
                },
                Mesh3d(kit.mesh.clone()),
                MeshMaterial3d(material.clone()),
                Transform::from_translation(burst.at).with_scale(Vec3::splat(size)),
                NotShadowCaster,
            ));
        }
    }
    *seed = random.0;
}

fn animate(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut particles: Query<(Entity, &mut Particle, &mut Transform)>,
) {
    let dt = time.delta_secs().min(0.1);
    for (entity, mut particle, mut transform) in &mut particles {
        particle.life -= dt;
        if particle.life <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }
        particle.velocity.y -= particle.gravity * dt;
        let velocity = particle.velocity;
        transform.translation += velocity * dt;
        let fraction = particle.life / particle.lifetime;
        transform.scale = Vec3::splat(particle.size * fraction.sqrt());
    }
}
