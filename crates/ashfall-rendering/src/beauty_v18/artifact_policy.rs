//! V18 Beauty artifact policy.
//!
//! This is the contract that prevents the current visual problems from returning.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BeautyArtifactPolicyV18 {
    pub allow_large_screen_space_ellipses: bool,
    pub allow_camera_facing_haze_blobs: bool,
    pub allow_circular_fake_glare: bool,
    pub allow_floating_puddles: bool,
    pub allow_debug_boxes_in_beauty: bool,
    pub allow_material_smear_fallback: bool,
}

impl BeautyArtifactPolicyV18 {
    pub fn strict_beauty() -> Self {
        Self {
            allow_large_screen_space_ellipses: false,
            allow_camera_facing_haze_blobs: false,
            allow_circular_fake_glare: false,
            allow_floating_puddles: false,
            allow_debug_boxes_in_beauty: false,
            allow_material_smear_fallback: false,
        }
    }

    pub fn debug() -> Self {
        Self {
            allow_large_screen_space_ellipses: true,
            allow_camera_facing_haze_blobs: true,
            allow_circular_fake_glare: true,
            allow_floating_puddles: true,
            allow_debug_boxes_in_beauty: true,
            allow_material_smear_fallback: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BeautyArtifactCountersV18 {
    pub large_screen_space_ellipse_count: u32,
    pub camera_facing_haze_blob_count: u32,
    pub circular_fake_glare_count: u32,
    pub floating_puddle_count: u32,
    pub debug_box_count: u32,
    pub material_smear_fallback_count: u32,
}

impl BeautyArtifactCountersV18 {
    pub fn passes_policy(&self, policy: BeautyArtifactPolicyV18) -> bool {
        (policy.allow_large_screen_space_ellipses || self.large_screen_space_ellipse_count == 0)
            && (policy.allow_camera_facing_haze_blobs || self.camera_facing_haze_blob_count == 0)
            && (policy.allow_circular_fake_glare || self.circular_fake_glare_count == 0)
            && (policy.allow_floating_puddles || self.floating_puddle_count == 0)
            && (policy.allow_debug_boxes_in_beauty || self.debug_box_count == 0)
            && (policy.allow_material_smear_fallback || self.material_smear_fallback_count == 0)
    }
}
