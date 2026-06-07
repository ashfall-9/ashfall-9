//! V18 validation report.

use super::artifact_policy::{BeautyArtifactCountersV18, BeautyArtifactPolicyV18};
use super::lighting::NaturalLightingRigV18;
use super::materials::SurfaceTextureRecipeV18;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BeautyV18Failure {
    ArtifactPolicyViolation,
    MissingReadableNaturalLight,
    NeonStillDominates,
    FakeBloomGlare,
    NoMaterialRecipes,
    MaterialRecipeSmears,
    NoPhotorealMaterialDetail,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BeautyValidationInputV18 {
    pub policy: BeautyArtifactPolicyV18,
    pub counters: BeautyArtifactCountersV18,
    pub lighting: NaturalLightingRigV18,
    pub material_recipes: Vec<SurfaceTextureRecipeV18>,
}

impl Default for BeautyValidationInputV18 {
    fn default() -> Self {
        Self {
            policy: BeautyArtifactPolicyV18::strict_beauty(),
            counters: BeautyArtifactCountersV18::default(),
            lighting: NaturalLightingRigV18::default(),
            material_recipes: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BeautyValidationReportV18 {
    pub pass: bool,
    pub failures: Vec<BeautyV18Failure>,
}

impl BeautyValidationInputV18 {
    pub fn validate(&self) -> BeautyValidationReportV18 {
        let mut failures = Vec::new();

        if !self.counters.passes_policy(self.policy) {
            failures.push(BeautyV18Failure::ArtifactPolicyViolation);
        }
        if !self.lighting.readable_without_neon() {
            failures.push(BeautyV18Failure::MissingReadableNaturalLight);
        }
        if !self.lighting.neon_is_accent() {
            failures.push(BeautyV18Failure::NeonStillDominates);
        }
        if !self.lighting.bloom_is_not_fake_glare() {
            failures.push(BeautyV18Failure::FakeBloomGlare);
        }
        if self.material_recipes.is_empty() {
            failures.push(BeautyV18Failure::NoMaterialRecipes);
        }
        if self
            .material_recipes
            .iter()
            .any(|recipe| !recipe.avoids_smear())
        {
            failures.push(BeautyV18Failure::MaterialRecipeSmears);
        }
        if !self
            .material_recipes
            .iter()
            .any(SurfaceTextureRecipeV18::has_photoreal_detail)
        {
            failures.push(BeautyV18Failure::NoPhotorealMaterialDetail);
        }

        BeautyValidationReportV18 {
            pass: failures.is_empty(),
            failures,
        }
    }
}
