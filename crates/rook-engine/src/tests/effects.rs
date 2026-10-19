use super::*;

/// A server script's `ParticleEmitter:Emit` reaches the client's
/// renderer, even on an emitter made in the same frame; in Run mode the
/// server's own renderer takes it.
#[test]
fn emit_reaches_the_renderer() {
    let options = Options::default();
    let mut server = DataModel::new(&options).unwrap();
    add_script(
        &server,
        "ServerScriptService",
        "Burst",
        "local p = Instance.new('Part') p.Anchored = true p.Parent = workspace \
         local e = Instance.new('ParticleEmitter') e.Parent = p e:Emit(12)",
    );
    server.start();
    let mut session = Session::new(server, options);
    let index = session.join("Player1", 1).unwrap();
    session.run_for(0.2);
    let client = session.client_mut(index).unwrap();
    let calls = client.take_render_changes().effect_calls;
    assert_eq!(calls.len(), 1);
    let (id, call) = calls[0];
    assert_eq!(call, EffectCall::Emit(12));
    let dom = &client.runtime().world().dom;
    assert_eq!(dom.class_name(id), "ParticleEmitter");

    let mut run = DataModel::new(&Options::default()).unwrap();
    add_script(
        &run,
        "ServerScriptService",
        "Burst",
        "local e = Instance.new('ParticleEmitter', workspace) e:Emit() e:Clear()",
    );
    run.start();
    run.run_for(0.1);
    let calls: Vec<_> = run
        .take_render_changes()
        .effect_calls
        .into_iter()
        .map(|c| c.1)
        .collect();
    assert_eq!(calls, [EffectCall::Emit(16), EffectCall::Clear]);
}
