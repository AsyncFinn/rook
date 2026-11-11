//! MemStorageService, kept in `luau/mem_storage.luau`.

pub(super) fn luau_source(owner: &str, name: &str) -> Option<&'static str> {
    if owner != "MemStorageService" {
        return None;
    }
    Some(match name {
        "GetItem" => {
            "local m = (...).mem_storage return function(_, key, default) return m.get(key, default) end"
        }
        "HasItem" => "local m = (...).mem_storage return function(_, key) return m.has(key) end",
        "SetItem" => {
            "local m = (...).mem_storage return function(_, key, value) m.set(key, value) end"
        }
        "RemoveItem" => {
            "local m = (...).mem_storage return function(_, key) return m.remove(key) end"
        }
        "Fire" => {
            "local m = (...).mem_storage return function(_, key, value) m.fire(key, value) end"
        }
        "Bind" => {
            "local m = (...).mem_storage return function(_, key, fn) return m.bind(key, fn) end"
        }
        "BindAndFire" => {
            "local m = (...).mem_storage return function(_, key, fn) return m.bind_and_fire(key, fn) end"
        }
        "Call" => {
            "local m = (...).mem_storage return function(_, key, input) return m.call(key, input) end"
        }
        _ => return None,
    })
}
