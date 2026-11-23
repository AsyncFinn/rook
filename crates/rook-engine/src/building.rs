//! A DataModel built around a DOM, from a place file or empty: the
//! services, Camera and Terrain every place has, and its runtime and
//! bridges set up.

use crate::*;

impl DataModel {
    pub fn load_place(bytes: &[u8], options: &Options) -> Result<Self, EngineError> {
        let loaded = rook_formats::load(bytes)?;
        let mut dom = Dom::new();
        let root = dom.root();
        let converted = rook_formats::into_dom(loaded.dom, &mut dom, root);
        // Part of loading the place, not changes for anyone to replay.
        let journaling = dom.journal().is_enabled();
        dom.journal_mut().set_enabled(false);
        ensure_terrain(&mut dom);
        if rook_physics::joints::surfaces::joins_on_load(&dom)
            && let Err(e) = rook_physics::joints::surfaces::make_joints_on_load(&mut dom)
        {
            tracing::warn!("joining the place's surfaces failed: {e}");
        }
        dom.journal_mut().set_enabled(journaling);
        let engine = Self::with_dom(dom, options, Role::Server)?;
        if converted.duplicate_unique_ids {
            // Studio's Output, word for word (observed in Studio 0.743 on a
            // place with two instances saved under one UniqueId).
            engine.runtime.world_mut().push_log(
                MessageType::Error,
                "DM contains duplicate Unique ids".into(),
            );
        }
        Ok(engine)
    }

    pub(crate) fn with_dom(
        mut dom: Dom,
        options: &Options,
        role: Role,
    ) -> Result<Self, EngineError> {
        threads::size_threads();
        dom.journal_mut().set_enabled(false);
        // Scripts tell where they run by `game:FindFirstChild("NetworkServer")`
        // or `"NetworkClient"`: a playtest server has only the first and a
        // client only the second (observed in Studio 0.742).
        let network = match role {
            Role::Server => "NetworkServer",
            Role::Client => "NetworkClient",
        };
        for service in DEFAULT_SERVICES.iter().chain([&network]) {
            // A class missing from the database would be a Rook bug, not a
            // problem with the place; skip it rather than fail.
            if let Err(e) = dom.get_service(service) {
                tracing::warn!("no {service}: {e}");
            }
        }
        ensure_camera(&mut dom, role);
        // Roblox's client names its build here; the menu's version label shows it.
        if let Some(rs) = dom.find_service("RunService") {
            rook_dom::ignore_stale(dom.set_property_by_name(
                rs,
                "ClientGitHash",
                Value::String("rook".into()),
            ));
        }
        match role {
            Role::Server => chat::set_up(&mut dom),
            Role::Client => chat::set_up_client(&mut dom),
        }
        dom.journal_mut().set_enabled(true);
        let run_service = dom.get_service("RunService").expect("RunService class");
        // Workspace.SignalBehavior: Default currently means Immediate
        // (creator-docs SignalBehavior); new template places save Deferred.
        // AncestryDeferred is treated as Deferred (Approximation).
        let signal_behavior = match dom
            .find_service("Workspace")
            .and_then(|w| dom.enum_item_property(w, "SignalBehavior"))
        {
            Some("Deferred" | "AncestryDeferred") => rook_lua::SignalBehavior::Deferred,
            _ => rook_lua::SignalBehavior::Immediate,
        };
        let loadstring_enabled = dom
            .find_service("ServerScriptService")
            .and_then(|s| dom.bool_property(s, "LoadStringEnabled"))
            == Some(true);

        let runtime = Runtime::new(
            dom,
            Clock::Fixed {
                step: 1.0 / options.hz,
            },
        )
        .map_err(|e| EngineError::Lua(e.to_string()))?;
        runtime.set_script_timeout(options.script_timeout);
        runtime.world_mut().signal_behavior = signal_behavior;
        let content = options
            .content
            .clone()
            .unwrap_or_else(settings::local_content);
        if role == Role::Server {
            runtime.world_mut().rigs = Some(rigs::source(content.clone()));
        }
        runtime.world_mut().gui.set_content(content);
        runtime.set_endpoints(
            options
                .endpoints
                .clone()
                .unwrap_or_else(settings::endpoints),
        );
        runtime.set_heap_limit(options.memory_limit);
        if !loadstring_enabled {
            // Confirmed: creator-docs ServerScriptService.LoadStringEnabled.
            runtime
                .lua()
                .globals()
                .raw_set("loadstring", mlua::Value::Nil)
                .map_err(|e| EngineError::Lua(e.to_string()))?;
        }
        let cursor = runtime.world().dom.journal().end();
        Ok(Self {
            runtime,
            role,
            auto_trim: true,
            run_service,
            new_scripts: scripts::NewScripts::starting_at(cursor),
            core_scripts: options.core_scripts.clone(),
            render_watch: JournalWatch::starting_at(cursor),
            render_changes: RenderChanges::everything(),
            adorned: Default::default(),
            material_parts: Default::default(),
            input: Default::default(),
            content: Content {
                resolver: options.content.clone(),
                wait_for_sounds: options.wait_for_sounds,
                ..Content::default()
            },
            sounds: Default::default(),
            explosions: Default::default(),
            web: Default::default(),
            user_settings: None,
            freed: Vec::new(),
            camera_scripts: false,
        })
    }
}

/// Workspace always has a current camera: Roblox creates one if the place
/// was saved without (Inference: observed behavior, not documented). A
/// client always makes its own (Camera isn't replicated), at a new
/// Camera's angle but 12.5 studs back from its Focus (measured in Studio;
/// see docs/behavior/characters/camera.md#the-camera-before-spawning).
pub(super) fn ensure_camera(dom: &mut Dom, role: Role) {
    let Ok(ws) = dom.get_service("Workspace") else {
        return;
    };
    if dom
        .ref_property(ws, "CurrentCamera")
        .is_some_and(|c| dom.get(c).is_some())
    {
        return;
    }
    let Ok(camera) = dom.create_by_name("Camera") else {
        return;
    };
    if let Err(e) = dom.set_name(camera, "Camera".into()) {
        tracing::warn!("naming the Camera failed: {e}");
    }
    dom.attach_unchecked(camera, ws);
    if let Some(p) = dom.property_id(ws, "CurrentCamera") {
        dom.load_property(ws, p, Value::Ref(Some(camera.to_ref())));
    }
    if role == Role::Client
        && let (Some(cf), Some(focus)) = (
            dom.cframe_property(camera, "CFrame"),
            dom.cframe_property(camera, "Focus"),
        )
        && let Some(p) = dom.property_id(camera, "CFrame")
    {
        const BACK: f32 = 12.5;
        let cf = rook_types::CFrame {
            position: focus.position - cf.look_vector().0 * BACK,
            ..cf
        };
        dom.load_property(camera, p, Value::CFrame(cf));
    }
}

/// Workspace always has a Terrain, as in Roblox, where `workspace.Terrain`
/// is never nil (Inference: every place Studio saves has one, and a new
/// place starts with one). Clients get the server's by replication.
pub(super) fn ensure_terrain(dom: &mut Dom) {
    let Ok(ws) = dom.get_service("Workspace") else {
        return;
    };
    if dom.find_first_child_of_class(ws, "Terrain").is_some() {
        return;
    }
    let Ok(terrain) = dom.create_by_name("Terrain") else {
        return;
    };
    if let Err(e) = dom.set_name(terrain, "Terrain".into()) {
        tracing::warn!("naming the Terrain failed: {e}");
    }
    dom.attach_unchecked(terrain, ws);
}
