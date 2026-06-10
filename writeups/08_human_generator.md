# 08 — Realistic Human Generator

## Goal

Eventually generate photorealistic, performant human characters with realistic skin, eyes, hair, anatomy, deformation, facial expression, voice anchors, and AI identity anchors.

## Sanity constraint

Do not start with a full human. Human rendering is the most difficult visual target in the project. Build and certify body-part milestones first.

Recommended staging:

```text
1. skin material patch
2. eye close-up
3. hair lock
4. mouth/teeth/tongue close-up
5. face neutral pose
6. face expression loop
7. head with hair and eyes
8. hands
9. full clothed human
10. AI-driven speaking human
```

## Human asset bundle

```rust
pub struct HumanSpec {
    pub identity_seed: u64,
    pub age_range: AgeRange,
    pub body_params: BodyParams,
    pub face_params: FaceParams,
    pub skin_params: SkinParams,
    pub hair_params: HairParams,
    pub eye_params: EyeParams,
    pub performance_tier: HumanPerformanceTier,
    pub consent_metadata: Option<ConsentMetadata>,
}

pub struct HumanAssetBundle {
    pub character_id: CharacterId,
    pub render_avatar: RenderAvatarDesc,
    pub physics_avatar: PhysicsAvatarDesc,
    pub animation_rig: AnimationRigDesc,
    pub material_set: HumanMaterialSet,
    pub ai_anchor: AiPersonaAnchor,
    pub voice_anchor: VoicePersonaAnchor,
    pub provenance: HumanProvenance,
}
```

## Consent and identity rule

Generated humans must not reproduce a real person’s face, body identity, or voice without explicit rights and provenance metadata.

```text
- store consent/provenance for scans, photos, voice samples, and likeness references
- disallow prompts like "make this actor/person" unless consent is present
- store generated identity as its own fictional identity seed
- keep voice and face identity safety linked
```

## Skin model

Skin should be layered, not a single texture.

```text
Layer 0 — anatomical base:
  body region, thickness, curvature, joint areas

Layer 1 — pigmentation:
  melanin, freckles, moles, scars, redness variation

Layer 2 — vascular/subsurface:
  hemoglobin approximation, blood-flow tint, temperature response

Layer 3 — microgeometry:
  pores, fine lines, wrinkles, goosebumps

Layer 4 — surface state:
  oiliness, sweat, dryness, dirt, makeup, wounds

Layer 5 — dynamic response:
  blush, pressure marks, bruising, wetness, aging variation
```

Runtime skin requirements:

```text
- subsurface approximation
- region-specific roughness/oiliness
- detail normals and procedural microstructure
- expression-driven wrinkle fields
- temporal stability under animation
- LOD fallback for mid/far distance
```

## Eye model

```text
- cornea geometry
- wet tearline
- sclera material with subtle vascular detail
- iris depth/parallax
- pupil dilation
- accurate reflection/highlight behavior
- stable gaze and eyelid interaction
```

Eye acceptance should include close-up stills and animation loops. Dead eyes are often caused by poor gaze, eyelid, wetness, and reflection behavior rather than material alone.

## Hair model

```text
Hero LOD:
  strand or strand-cluster representation, anisotropic lighting, self-shadowing

Near gameplay LOD:
  reduced strand clusters or high-quality cards

Mid LOD:
  generated cards with baked directional opacity/normal maps

Far LOD:
  shell, simplified mesh, or impostor
```

Hair evaluation should check:

```text
- clumping
- scalp transition
- flyaways
- silhouette stability
- self-shadowing
- temporal shimmer
- LOD popping
```

## Muscle and deformation

Do not simulate full anatomy for every character. Use tiered deformation:

```text
Hero:
  high-quality rig, corrective shapes, muscle-like volume preservation

GameplayNear:
  skeleton + corrective shapes + limited procedural deformation

GameplayMid:
  simpler skinning and baked expressions

Crowd:
  aggressive animation/material LOD
```

## Human performance tiers

```text
Hero:
  cinematic close-up, highest detail, expensive but controlled

GameplayNear:
  close interaction, full face/eye/skin quality, optimized hair

GameplayMid:
  stable silhouette and expression, reduced detail

Crowd:
  baked animation/materials, strong LOD

Background:
  impostor or simplified mesh
```

Every tier must define:

```text
- triangle/meshlet budget
- material count
- texture/procedural cache budget
- skinning cost
- hair cost
- draw/dispatch count
- target realism score
```

## Human evaluation suite

Required body-part evals:

```text
- skin patch under varied lighting
- eye close-up with gaze changes
- hair lock with motion
- mouth/teeth/tongue close-up
- face neutral pose
- face expression loop
- head turn with eye tracking
```

Metrics:

```text
- AI realism score
- uncanny-valley artifact tags
- temporal shimmer/flicker
- LOD popping
- material memory
- animation cost
- skin/hair/eye GPU pass cost
```

## First human-related milestone

Do not attempt a face first. Start with a skin material patch on a simple curved surface.

Acceptance:

```text
- plausible macro/mid-distance skin under controlled lighting
- no obvious tiling
- pore scale plausible
- roughness/subsurface behavior plausible
- shader cost within skin-patch budget
```
