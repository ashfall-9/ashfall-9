use std::fmt;

pub type EntityId = u64;
pub type MaterialId = u64;
pub type ModuleId = u64;
pub type FrameId = u64;
pub type AssetId = u128;
pub type WorldEventId = u128;
pub type HumanId = u64;
pub type AgentPersonaId = u64;
pub type VoicePersonaId = u64;
pub type FactionId = u64;
pub type LocationId = u64;
pub type LanguageId = u32;
pub type TagSet = Vec<String>;
pub type EvidenceRefs = Vec<String>;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0);

    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub fn distance(self, other: Self) -> f32 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        let dz = self.z - other.z;
        (dx * dx + dy * dy + dz * dz).sqrt()
    }
}

impl Default for Vec3 {
    fn default() -> Self {
        Self::ZERO
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quat {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Quat {
    pub const IDENTITY: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 0.0,
        w: 1.0,
    };
}

impl Default for Quat {
    fn default() -> Self {
        Self::IDENTITY
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    pub const fn new(min: Vec3, max: Vec3) -> Self {
        Self { min, max }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SimTime {
    pub seconds: f64,
    pub tick: u64,
}

impl SimTime {
    pub const fn new(seconds: f64, tick: u64) -> Self {
        Self { seconds, tick }
    }
}

impl Default for SimTime {
    fn default() -> Self {
        Self::new(0.0, 0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub translation_meters: Vec3,
    pub rotation_radians: Quat,
    pub scale: Vec3,
}

impl Transform {
    pub fn at(translation_meters: Vec3) -> Self {
        Self {
            translation_meters,
            ..Self::default()
        }
    }
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            translation_meters: Vec3::ZERO,
            rotation_radians: Quat::IDENTITY,
            scale: Vec3::new(1.0, 1.0, 1.0),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum QualityTier {
    Disabled,
    BackgroundApproximation,
    #[default]
    NormalRuntime,
    HeroHighFidelityRuntime,
    ReferenceOfflineValidation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MeshAssetHandle(pub AssetId);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TextureHandle(pub AssetId);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TextureAssetId(pub AssetId);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PhotoAssetId(pub AssetId);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AudioClipHandle(pub AssetId);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AudioBufferHandle(pub AssetId);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MaterialGraphHandle(pub AssetId);

#[derive(Clone, Debug, PartialEq)]
pub struct MaterialDescriptor {
    pub id: MaterialId,
    pub name: String,
    pub visual: VisualMaterial,
    pub physical: PhysicalMaterial,
    pub acoustic: AcousticMaterial,
    pub thermal: ThermalMaterial,
    pub electrical: ElectricalMaterial,
    pub procedural_source: Option<MaterialGeneratorRef>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VisualMaterial {
    pub base_color_linear: [f32; 4],
    pub roughness: f32,
    pub metallic: f32,
    pub transmission: f32,
    pub subsurface: f32,
    pub emission_linear: [f32; 3],
    pub anisotropy: f32,
    pub clearcoat: f32,
    pub normal_displacement_strength: f32,
    pub layer_count: u8,
    pub transparency: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PhysicalMaterial {
    pub density_kg_per_m3: f32,
    pub young_modulus: f32,
    pub poisson_ratio: f32,
    pub yield_stress: f32,
    pub fracture_toughness: f32,
    pub hardness: f32,
    pub viscosity: f32,
    pub surface_tension: f32,
    pub restitution: f32,
    pub friction_static: f32,
    pub friction_dynamic: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AcousticMaterial {
    pub impact_brightness: f32,
    pub resonance: f32,
    pub absorption: f32,
    pub wetness_muffle: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ThermalMaterial {
    pub heat_capacity: f32,
    pub conductivity: f32,
    pub ignition_temperature: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ElectricalMaterial {
    pub conductivity: f32,
    pub dielectric_strength: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MaterialGeneratorRef(pub AssetId);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MaterialState {
    pub temperature: f32,
    pub moisture: f32,
    pub dirt: f32,
    pub dust: f32,
    pub soot: f32,
    pub burn_level: f32,
    pub corrosion: f32,
    pub plastic_strain: f32,
    pub crack_density: f32,
    pub biological_contamination: f32,
    pub oil_contamination: f32,
    pub electrical_charge: f32,
    pub pressure: f32,
    pub exposed_interior: f32,
}

impl MaterialState {
    pub fn apply_delta(&mut self, delta: MaterialStateDelta) {
        self.temperature += delta.temperature;
        self.moisture = (self.moisture + delta.moisture).clamp(0.0, 1.0);
        self.dirt = (self.dirt + delta.dirt).clamp(0.0, 1.0);
        self.dust = (self.dust + delta.dust).clamp(0.0, 1.0);
        self.soot = (self.soot + delta.soot).clamp(0.0, 1.0);
        self.burn_level = (self.burn_level + delta.burn_level).clamp(0.0, 1.0);
        self.corrosion = (self.corrosion + delta.corrosion).clamp(0.0, 1.0);
        self.plastic_strain = (self.plastic_strain + delta.plastic_strain).max(0.0);
        self.crack_density = (self.crack_density + delta.crack_density).clamp(0.0, 1.0);
        self.biological_contamination =
            (self.biological_contamination + delta.biological_contamination).clamp(0.0, 1.0);
        self.oil_contamination = (self.oil_contamination + delta.oil_contamination).clamp(0.0, 1.0);
        self.electrical_charge += delta.electrical_charge;
        self.pressure += delta.pressure;
        self.exposed_interior = (self.exposed_interior + delta.exposed_interior).clamp(0.0, 1.0);
    }
}

impl Default for MaterialState {
    fn default() -> Self {
        Self {
            temperature: 293.15,
            moisture: 0.0,
            dirt: 0.0,
            dust: 0.0,
            soot: 0.0,
            burn_level: 0.0,
            corrosion: 0.0,
            plastic_strain: 0.0,
            crack_density: 0.0,
            biological_contamination: 0.0,
            oil_contamination: 0.0,
            electrical_charge: 0.0,
            pressure: 101_325.0,
            exposed_interior: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MaterialStateDelta {
    pub entity: EntityId,
    pub temperature: f32,
    pub moisture: f32,
    pub dirt: f32,
    pub dust: f32,
    pub soot: f32,
    pub burn_level: f32,
    pub corrosion: f32,
    pub plastic_strain: f32,
    pub crack_density: f32,
    pub biological_contamination: f32,
    pub oil_contamination: f32,
    pub electrical_charge: f32,
    pub pressure: f32,
    pub exposed_interior: f32,
}

impl MaterialStateDelta {
    pub fn zero(entity: EntityId) -> Self {
        Self {
            entity,
            temperature: 0.0,
            moisture: 0.0,
            dirt: 0.0,
            dust: 0.0,
            soot: 0.0,
            burn_level: 0.0,
            corrosion: 0.0,
            plastic_strain: 0.0,
            crack_density: 0.0,
            biological_contamination: 0.0,
            oil_contamination: 0.0,
            electrical_charge: 0.0,
            pressure: 0.0,
            exposed_interior: 0.0,
        }
    }

    pub fn crack(entity: EntityId, amount: f32) -> Self {
        Self {
            crack_density: amount,
            exposed_interior: amount * 0.5,
            dust: amount * 0.25,
            ..Self::zero(entity)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ForceCommand {
    pub entity: EntityId,
    pub vector_newtons: Vec3,
    pub impulse_newton_seconds: f32,
    pub source: Option<EntityId>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DamageCommand {
    pub entity: EntityId,
    pub amount: f32,
    pub source: Option<EntityId>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Velocity {
    pub linear_meters_per_second: Vec3,
    pub angular_radians_per_second: Vec3,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmotionState {
    pub fear: f32,
    pub anger: f32,
    pub urgency: f32,
    pub trust: f32,
}

impl EmotionState {
    pub fn alarmed() -> Self {
        Self {
            fear: 0.8,
            anger: 0.25,
            urgency: 0.9,
            trust: 0.1,
        }
    }
}

impl Default for EmotionState {
    fn default() -> Self {
        Self {
            fear: 0.0,
            anger: 0.0,
            urgency: 0.0,
            trust: 0.5,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PerformanceCounters {
    pub cpu_milliseconds: f32,
    pub gpu_milliseconds: f32,
    pub memory_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaVersion {
    pub name: &'static str,
    pub version: u32,
}

impl fmt::Display for SchemaVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@v{}", self.name, self.version)
    }
}
