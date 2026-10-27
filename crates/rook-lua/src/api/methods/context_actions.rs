//! ContextActionService, kept in `luau/context_action.luau`; the engine
//! offers it input through [`crate::Runtime::context_action`].

pub(super) fn luau_source(owner: &str, name: &str) -> Option<&'static str> {
    if owner != "ContextActionService" {
        return None;
    }
    Some(match name {
        "BindAction" => {
            "local c = (...).context_action return function(_, name, fn, touch, ...) c.bind('game', name, fn, touch, nil, ...) end"
        }
        "BindActionAtPriority" => {
            "local c = (...).context_action return function(_, name, fn, touch, priority, ...) c.bind('game', name, fn, touch, priority, ...) end"
        }
        "BindCoreAction" => {
            "local c = (...).context_action return function(_, name, fn, touch, ...) c.bind('core', name, fn, touch, nil, ...) end"
        }
        "BindCoreActionAtPriority" => {
            "local c = (...).context_action return function(_, name, fn, touch, priority, ...) c.bind('core', name, fn, touch, priority, ...) end"
        }
        "UnbindAction" => {
            "local c = (...).context_action return function(_, name) c.unbind('game', name) end"
        }
        "UnbindCoreAction" => {
            "local c = (...).context_action return function(_, name) c.unbind('core', name) end"
        }
        "UnbindAllActions" => {
            "local c = (...).context_action return function() c.unbind_all('game') end"
        }
        "GetAllBoundActionInfo" => {
            "local c = (...).context_action return function() return c.info('game') end"
        }
        "GetAllBoundCoreActionInfo" => {
            "local c = (...).context_action return function() return c.info('core') end"
        }
        "GetBoundActionInfo" => {
            "local c = (...).context_action return function(_, name) return c.info('game')[name] or {} end"
        }
        "GetBoundCoreActionInfo" => {
            "local c = (...).context_action return function(_, name) return c.info('core')[name] or {} end"
        }
        _ => return None,
    })
}
