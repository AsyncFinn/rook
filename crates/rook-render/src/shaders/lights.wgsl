// Local lights as Studio's voxel lighting takes them: what each light
// writes into the 4-stud voxels around it (docs/behavior/lights.md). The
// renderer fills `Light`s (`renderer/lighting/local.rs`).

struct Light {
    // xyz: where it is; w: the distance its falloff reaches 0.
    position: vec4<f32>,
    // rgb: Brightness x Color squared; w: falloff per stud.
    color: vec4<f32>,
    // xyz: where it points; w: a spot light's tan(Angle / 2), a surface
    // light's cosine where its cone has faded out.
    direction: vec4<f32>,
    // x: 0 point, 1 spot, 2 surface; y: how far a spot light brightens
    // from its origin; z: 1 when parts block it.
    kind: vec4<f32>,
    // xyz: the box of the part it hangs from, which doesn't block it.
    part_min: vec4<f32>,
    part_max: vec4<f32>,
};

// Studio's light voxels, in studs, centred at 4k + 2.
const VOXEL: f32 = 4.0;

// What one light writes into the voxel centred at `v` (fitted to Studio's
// voxels, docs/behavior/lights.md).
fn light_at_voxel(light: Light, v: vec3<f32>) -> vec3<f32> {
    let kind = light.kind.x;
    let to = v - light.position.xyz;
    let d = length(to);
    var k = clamp((light.position.w - d) * light.color.w, 0.0, 1.0);
    let along = dot(to, light.direction.xyz);
    if kind > 1.5 {
        // Fading out by the cosine towards the cone's edge.
        let edge = light.direction.w;
        k *= clamp((along / max(d, 1e-4) - edge) / (1.0 - edge), 0.0, 1.0);
    } else if kind > 0.5 {
        // A spot light: nothing behind it; fading with the distance off
        // its axis, out to the cone's width there plus a voxel, and
        // brightening from its origin.
        if along <= 0.0 {
            return vec3(0.0);
        }
        let off = length(to - light.direction.xyz * along);
        let width = max(along, 0.0) * light.direction.w + VOXEL;
        k *= clamp(1.0 - off / width, 0.0, 1.0) * clamp((d - 2.0) / light.kind.y, 0.0, 1.0);
    }
    return light.color.rgb * k;
}

