//! Fire, Smoke and Sparkles under parts and their Attachments in
//! Workspace, as the renderer's particle emitters
//! (docs/behavior/effects.md). Each is fitted to Studio 0.742.

mod textures;

use rook_render::{Curve, Emitter, EmitterLook};

pub(super) use textures::EffectTextures;

use super::*;

/// Whether `id` is an effect this module draws.
pub(super) fn is_effect(dom: &Dom, id: InstanceId) -> bool {
    matches!(dom.class_name(id), "Fire" | "Smoke" | "Sparkles")
}

/// How many emitters an effect of `class` draws with.
fn emitter_count(class: &str) -> usize {
    match class {
        // Its flames, and the core and trail in SecondaryColor.
        "Fire" => 2,
        _ => 1,
    }
}

/// The effects in Workspace, the emitters they draw with, and the parts
/// and Attachments that carry them.
#[derive(Debug, Default)]
pub(super) struct EffectCache {
    ids: Vec<InstanceId>,
    /// Each effect's first emitter in `Scene::emitters`.
    first: Vec<usize>,
    /// Indices into `ids` of the effects each part or Attachment carries,
    /// and of each effect itself.
    holders: HashMap<InstanceId, Vec<usize>>,
    textures: Option<EffectTextures>,
}

impl EffectCache {
    pub(super) fn new(textures: EffectTextures) -> Self {
        Self {
            textures: Some(textures),
            ..Default::default()
        }
    }

    /// Find every effect in Workspace again, and read them all.
    pub(super) fn find(&mut self, dom: &Dom, emitters: &mut Vec<Emitter>) {
        profiling::scope!("EffectCache::find");
        let Some(ws) = dom.find_service("Workspace") else {
            return;
        };
        let mut ids = dom.descendants(ws);
        ids.retain(|id| is_effect(dom, *id));
        self.ids = ids;
        self.first.clear();
        self.holders.clear();
        emitters.clear();
        for (i, &id) in self.ids.iter().enumerate() {
            self.first.push(emitters.len());
            self.holders.entry(id).or_default().push(i);
            let mut holder = dom.parent(id);
            while let Some(h) = holder {
                self.holders.entry(h).or_default().push(i);
                if dom.class_name(h) != "Attachment" {
                    break;
                }
                holder = dom.parent(h);
            }
            let read = self.read(dom, id);
            emitters.extend(read);
        }
    }

    /// Bring the emitters up to date with a frame's changes: effects whose
    /// properties changed are read again, and those on parts and
    /// Attachments that moved only move. Whether any changed.
    pub(super) fn changed(
        &mut self,
        dom: &Dom,
        changes: &RenderChanges,
        emitters: &mut Vec<Emitter>,
    ) -> bool {
        let holders = &self.holders;
        let placed = changes.placed.iter().any(|id| holders.contains_key(id));
        if changes.effects_placed || placed {
            self.find(dom, emitters);
            return true;
        }
        let mut any = false;
        let stale: HashSet<usize> = changes
            .effects
            .iter()
            .chain(&changes.parts)
            .filter_map(|id| holders.get(id))
            .flatten()
            .copied()
            .collect();
        for &i in &stale {
            let read = self.read(dom, self.ids[i]);
            let first = self.first[i];
            emitters.splice(first..first + read.len(), read);
            any = true;
        }
        let moved: HashSet<usize> = changes
            .moved
            .iter()
            .filter_map(|id| holders.get(id))
            .flatten()
            .copied()
            .filter(|i| !stale.contains(i))
            .collect();
        for i in moved {
            let id = self.ids[i];
            let Some(frame) = holder_frame(dom, id) else {
                continue;
            };
            let first = self.first[i];
            for e in &mut emitters[first..first + emitter_count(dom.class_name(id))] {
                e.position = frame.position;
                e.rotation = frame.rotation;
            }
            any = true;
        }
        any
    }

    /// The emitters effect `id` draws with now: always as many for its
    /// class, disabled where Studio draws nothing (not under a part or an
    /// Attachment).
    fn read(&self, dom: &Dom, id: InstanceId) -> Vec<Emitter> {
        let textures = self.textures.as_ref().expect("made with textures");
        let class = dom.class_name(id);
        let frame = holder_frame(dom, id);
        let enabled = frame.is_some() && boolean(dom, id, "Enabled") != Some(false);
        let frame = frame.unwrap_or(CFrame::IDENTITY);
        let time_scale = number(dom, id, "TimeScale").unwrap_or(1.0).clamp(0.0, 1.0);
        let looks = match class {
            "Fire" => fire(dom, id, textures),
            "Smoke" => vec![smoke(dom, id, textures)],
            _ => vec![sparkles(dom, id, textures)],
        };
        looks
            .into_iter()
            .map(|look| Emitter {
                position: frame.position,
                rotation: frame.rotation,
                enabled,
                time_scale,
                look: Arc::new(look),
            })
            .collect()
    }
}

/// Where an effect emits from and which way: its part's CFrame, or its
/// Attachment's in the world. `None` when it's under neither.
fn holder_frame(dom: &Dom, id: InstanceId) -> Option<CFrame> {
    let parent = dom.parent(id)?;
    let cframe = |id| match dom.property_by_name(id, "CFrame")? {
        Value::CFrame(cf) => Some(cf),
        _ => None,
    };
    if is_part(dom, parent) {
        return cframe(parent);
    }
    if dom.class_name(parent) == "Attachment" {
        let part = dom.parent(parent).filter(|&p| is_part(dom, p))?;
        return Some(cframe(part)? * cframe(parent)?);
    }
    None
}

/// Keypoints from a table of values at even steps over a particle's life.
fn even(values: &[f32]) -> Curve {
    let last = (values.len() - 1).max(1) as f32;
    Curve(
        values
            .iter()
            .enumerate()
            .map(|(i, &v)| (i as f32 / last, v))
            .collect(),
    )
}

/// Fire: flames in `Color` that rise faster the higher `Heat` (its
/// acceleration in studs/s², halved), as wide as about half its `Size`,
/// covering what's behind and glowing; and a smaller, longer-lived core
/// and trail in `SecondaryColor` added on top. Neither is lit.
fn fire(dom: &Dom, id: InstanceId, textures: &EffectTextures) -> Vec<EmitterLook> {
    let size = number(dom, id, "Size").unwrap_or(5.0).clamp(2.0, 30.0);
    let heat = number(dom, id, "Heat").unwrap_or(9.0).clamp(-25.0, 25.0);
    let primary = color(dom, id, "Color").unwrap_or([0.925, 0.545, 0.275]);
    let secondary = color(dom, id, "SecondaryColor").unwrap_or([0.545, 0.314, 0.216]);
    let rise = Vec3::new(0.0, heat * 0.5, 0.0);
    let flames = EmitterLook {
        texture: Some(textures.fire),
        rate: 40.0,
        lifetime: [1.0, 1.4],
        spawn_radius: 0.12 * size,
        velocity: Vec3::ZERO,
        spread: 0.05 * size,
        acceleration: rise,
        drag: 0.0,
        size: Curve(vec![(0.0, 0.4 * size), (1.0, 0.15 * size)]),
        size_spread: [0.8, 1.2],
        spin: [0.0, 60.0],
        color: primary,
        brightness: Curve(vec![(0.0, 20.0), (0.5, 14.0), (0.75, 2.0), (1.0, 0.0)]),
        alpha: Curve(vec![(0.0, 0.0), (0.1, 1.0), (0.7, 0.8), (1.0, 0.0)]),
        light_emission: 0.0,
        lit: false,
    };
    let core = EmitterLook {
        texture: Some(textures.fire),
        rate: 40.0,
        lifetime: [1.3, 1.9],
        spawn_radius: 0.05 * size,
        velocity: Vec3::ZERO,
        spread: 0.05 * size,
        acceleration: rise * 1.1,
        drag: 0.0,
        size: Curve(vec![(0.0, 0.3 * size), (1.0, 0.1 * size)]),
        size_spread: [0.8, 1.2],
        spin: [0.0, 60.0],
        color: secondary,
        brightness: Curve::constant(4.0),
        alpha: Curve(vec![(0.0, 0.0), (0.15, 0.8), (0.6, 0.5), (1.0, 0.0)]),
        light_emission: 1.0,
        lit: false,
    };
    vec![flames, core]
}

/// Smoke: big, lit puffs in `Color` rising at `RiseVelocity` and growing
/// from `Size` by about 4 studs a second, as opaque as `Opacity` at their
/// thickest, a third of the way through their 5 seconds.
fn smoke(dom: &Dom, id: InstanceId, textures: &EffectTextures) -> EmitterLook {
    let size = number(dom, id, "Size").unwrap_or(1.0).clamp(0.1, 100.0);
    let opacity = number(dom, id, "Opacity").unwrap_or(0.5).clamp(0.0, 1.0);
    let rise = number(dom, id, "RiseVelocity")
        .unwrap_or(1.0)
        .clamp(-25.0, 25.0);
    // The install's smoke ramp (`textures/particles/smoke_color.dds`
    // alpha), in sixteenths of the life.
    let ramp = [
        0.02, 0.07, 0.22, 0.42, 0.58, 0.63, 0.57, 0.45, 0.32, 0.22, 0.15, 0.11, 0.07, 0.05, 0.02,
        0.0,
    ];
    let alpha: Vec<f32> = ramp.iter().map(|a| (a * opacity * 3.6).min(1.0)).collect();
    EmitterLook {
        texture: Some(textures.smoke),
        rate: 10.0,
        lifetime: [4.5, 5.5],
        spawn_radius: 0.1 * size,
        velocity: Vec3::new(0.0, rise, 0.0),
        spread: 3.0,
        acceleration: Vec3::ZERO,
        drag: 0.4,
        size: Curve(vec![(0.0, size + 1.0), (1.0, size + 14.0)]),
        size_spread: [0.8, 1.2],
        spin: [0.0, 20.0],
        color: color(dom, id, "Color").unwrap_or([1.0; 3]),
        brightness: Curve::constant(1.0),
        alpha: even(&alpha),
        light_emission: 0.0,
        lit: true,
    }
}

/// Sparkles: stars in `SparkleColor` thrown out a few studs every way
/// (more upward), lighting what's behind them.
fn sparkles(dom: &Dom, id: InstanceId, textures: &EffectTextures) -> EmitterLook {
    EmitterLook {
        texture: Some(textures.sparkles),
        rate: 30.0,
        lifetime: [1.0, 1.6],
        spawn_radius: 0.5,
        velocity: Vec3::new(0.0, 2.0, 0.0),
        spread: 8.0,
        acceleration: Vec3::ZERO,
        drag: 1.0,
        size: Curve::constant(1.6),
        size_spread: [0.4, 1.4],
        spin: [0.0, 90.0],
        color: color(dom, id, "SparkleColor").unwrap_or([0.565, 0.098, 1.0]),
        brightness: Curve::constant(1.8),
        alpha: Curve(vec![
            (0.0, 0.0),
            (0.2, 0.1),
            (0.45, 0.85),
            (0.7, 0.55),
            (1.0, 0.0),
        ]),
        light_emission: 1.0,
        lit: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rook_engine::Options;

    const PLACE: &str = r#"<roblox version="4">
<Item class="Workspace" referent="W"><Properties><string name="Name">Workspace</string></Properties>
<Item class="Part" referent="P"><Properties><string name="Name">Holder</string><bool name="Anchored">true</bool></Properties>
<Item class="Fire" referent="F"><Properties><string name="Name">Fire</string></Properties></Item>
<Item class="Smoke" referent="S"><Properties><string name="Name">Smoke</string></Properties></Item>
</Item></Item>
<Item class="ServerScriptService" referent="SS"><Properties><string name="Name">ServerScriptService</string></Properties>
<Item class="Script" referent="X"><Properties><string name="Name">Script</string><ProtectedString name="Source"><![CDATA[
task.wait(0.5)
local holder = workspace.Holder
holder.Fire.Enabled = false
holder.Smoke.Color = Color3.new(1, 0, 0)
holder.Position = Vector3.new(10, 0, 0)
]]></ProtectedString></Properties></Item></Item>
</roblox>"#;

    /// A script switching an effect off, changing its colour and moving
    /// its part changes the emitters drawn.
    #[test]
    fn scripts_redraw_effects() {
        let mut dm = DataModel::load_place(PLACE.as_bytes(), &Options::default()).unwrap();
        dm.start();
        dm.take_render_changes();
        let content = Arc::new(Resolver::new(rook_content::Config::from_env(false)));
        let mut cache = SceneCache::new(&dm.runtime().world().dom, content, Vec3::ZERO);
        // Fire's two emitters, then Smoke's.
        let e = &cache.scene.emitters;
        assert_eq!(e.len(), 3);
        assert!(e.iter().all(|e| e.enabled));
        dm.run_for(1.0);
        let changes = dm.take_render_changes();
        assert_ne!(
            cache.apply(&dm.runtime().world().dom, &changes),
            SceneChange::None
        );
        let e = &cache.scene.emitters;
        assert!(!e[0].enabled && !e[1].enabled && e[2].enabled);
        assert_eq!(e[2].look.color, [1.0, 0.0, 0.0]);
        assert!(
            e.iter().all(|e| e.position.x == 10.0),
            "{:?}",
            e[0].position
        );
    }
}
