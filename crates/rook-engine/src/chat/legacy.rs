//! The legacy chat, installed as a server starts the way Roblox's installers
//! do (`Modules/Server/ClientChat/ChatWindowInstaller` and
//! `Modules/Server/ServerChat/ChatServiceInstaller` in its server scripts):
//! what the Chat service saves is kept, the rest comes from Roblox's file,
//! and the runner and the client's scripts are copied out to where they run.
//! Games wait on those copies, so a place's own forks are copied out even
//! with no Roblox file to fill in the rest. See docs/behavior/gui/chat.md.

use rook_dom::{Dom, DomError, InstanceId};
use rook_types::Value;

use crate::core_scripts::CorePatch;

/// Roblox's script folders, copied out of its file for the install.
#[derive(Clone, Copy)]
struct Files {
    holder: InstanceId,
    client: InstanceId,
    server: InstanceId,
    common: InstanceId,
}

impl Files {
    fn load(dom: &mut Dom, patch: &CorePatch) -> Option<Self> {
        let file = patch.server_scripts()?;
        let holder = dom.create_by_name("Folder").ok()?;
        let tops = rook_formats::copy_into_dom(file, dom, holder).top;
        let modules = *tops.first()?;
        let server = dom.find_first_child(modules, "Server", false)?;
        Some(Self {
            holder,
            client: dom.find_first_child(server, "ClientChat", false)?,
            server: dom.find_first_child(server, "ServerChat", false)?,
            common: dom.find_first_child(modules, "Common", false)?,
        })
    }
}

/// Whether Roblox's server would install it: `Chat.LoadDefaultChat`, on a
/// place using the legacy chat (what `ChatVersion` unset in a file reads).
fn wanted(dom: &Dom, chat: InstanceId) -> bool {
    let version = dom
        .find_service("TextChatService")
        .and_then(|t| dom.enum_item_property(t, "ChatVersion"));
    dom.bool_property(chat, "LoadDefaultChat") != Some(false)
        && version == Some("LegacyChatService")
}

/// A new `class` named `name` under `parent`.
fn create(
    dom: &mut Dom,
    class: &str,
    name: &str,
    parent: InstanceId,
) -> Result<InstanceId, DomError> {
    let id = dom.create_by_name(class)?;
    dom.set_name(id, name.to_owned())?;
    dom.set_parent(id, Some(parent))?;
    Ok(id)
}

/// A `class` running the source of the ModuleScript `name` in `from`, which
/// is how Roblox's file keeps every script.
fn load(
    dom: &mut Dom,
    from: InstanceId,
    name: &str,
    class: &str,
    parent: InstanceId,
) -> Result<InstanceId, DomError> {
    let source = dom
        .find_first_child(from, name, false)
        .and_then(|module| dom.string_property(module, "Source"))
        .unwrap_or_default();
    let id = dom.create_by_name(class)?;
    dom.set_name(id, name.to_owned())?;
    dom.set_property_by_name(id, "Source", Value::String(source))?;
    dom.set_parent(id, Some(parent))?;
    Ok(id)
}

/// `parent`'s folder `name`, made unsaved if it has none, with
/// `InsertDefaultModules` on if asked.
fn folder(
    dom: &mut Dom,
    parent: InstanceId,
    name: &str,
    defaults: bool,
) -> Result<InstanceId, DomError> {
    if let Some(found) = dom.find_first_child(parent, name, false) {
        return Ok(found);
    }
    let id = create(dom, "Folder", name, parent)?;
    dom.set_property_by_name(id, "Archivable", Value::Bool(false))?;
    if defaults {
        let insert = create(dom, "BoolValue", "InsertDefaultModules", id)?;
        dom.set_property_by_name(insert, "Value", Value::Bool(true))?;
    }
    Ok(id)
}

fn inserts_defaults(dom: &Dom, folder: InstanceId) -> bool {
    dom.find_first_child(folder, "InsertDefaultModules", false)
        .is_some_and(|i| dom.bool_property(i, "Value") == Some(true))
}

/// Give `into` the ModuleScripts in `from` it doesn't have.
fn fill(dom: &mut Dom, from: InstanceId, into: InstanceId) -> Result<(), DomError> {
    for module in dom.children(from).to_vec() {
        let name = dom.name(module).to_owned();
        if dom.class_name(module) == "ModuleScript"
            && dom.find_first_child(into, &name, false).is_none()
        {
            load(dom, from, &name, "ModuleScript", into)?;
        }
    }
    Ok(())
}

/// Copy `script` under `parent` unless a child there has its name.
fn copy_out(dom: &mut Dom, script: InstanceId, parent: InstanceId) -> Result<(), DomError> {
    let name = dom.name(script);
    if dom.find_first_child(parent, name, false).is_none()
        && let Some(copy) = dom.clone_instance(script)
    {
        dom.set_property_by_name(copy, "Archivable", Value::Bool(false))?;
        dom.set_parent(copy, Some(parent))?;
    }
    Ok(())
}

/// Keep what Roblox's file made out of the place's saves, once it's copied:
/// `Clone` refuses what isn't Archivable.
fn keep_unsaved(dom: &mut Dom, made: InstanceId) -> Result<(), DomError> {
    dom.set_property_by_name(made, "Archivable", Value::Bool(false))
}

/// The client's scripts and modules: `ChatWindowInstaller`.
fn install_window(dom: &mut Dom, chat: InstanceId, files: Option<Files>) -> Result<(), DomError> {
    let mut script = dom.find_first_child(chat, "ChatScript", false);
    let mut bubble = dom.find_first_child(chat, "BubbleChat", false);
    let saved = [script.is_some(), bubble.is_some()];
    if let Some(f) = files {
        if script.is_none() {
            script = Some(load_chat_script(dom, chat, f)?);
        }
        if bubble.is_none()
            && let Some(from) = dom.find_first_child(f.client, "BubbleChat", false)
        {
            bubble = Some(load(dom, from, "BubbleChat", "LocalScript", chat)?);
        }
        fill_client_modules(dom, chat, f)?;
    }
    if let Some(player) = dom.find_service("StarterPlayer") {
        // Roblox waits for this child: it has one, saved or not.
        let starter = match dom.find_first_child_of_class(player, "StarterPlayerScripts") {
            Some(saved) => saved,
            None => create(dom, "StarterPlayerScripts", "StarterPlayerScripts", player)?,
        };
        for script in [script, bubble].into_iter().flatten() {
            copy_out(dom, script, starter)?;
        }
    }
    for (script, saved) in [script, bubble].into_iter().zip(saved) {
        if let Some(script) = script
            && !saved
        {
            keep_unsaved(dom, script)?;
        }
    }
    Ok(())
}

fn load_chat_script(dom: &mut Dom, chat: InstanceId, f: Files) -> Result<InstanceId, DomError> {
    let script = load(dom, f.client, "ChatScript", "LocalScript", chat)?;
    let main = load(dom, f.client, "ChatMain", "ModuleScript", script)?;
    for name in [
        "ChannelsBar",
        "ChatBar",
        "ChatChannel",
        "MessageLogDisplay",
        "ChatWindow",
        "MessageLabelCreator",
        "CommandProcessor",
        "ChannelsTab",
    ] {
        load(dom, f.client, name, "ModuleScript", main)?;
    }
    load(dom, f.common, "ObjectPool", "ModuleScript", main)?;
    load(dom, f.client, "MessageSender", "ModuleScript", main)?;
    load(dom, f.client, "CurveUtil", "ModuleScript", main)?;
    Ok(script)
}

fn fill_client_modules(dom: &mut Dom, chat: InstanceId, f: Files) -> Result<(), DomError> {
    let Some(defaults) = dom.find_first_child(f.client, "DefaultClientChatModules", false) else {
        return Ok(());
    };
    let modules = folder(dom, chat, "ClientChatModules", false)?;
    for name in ["ChatSettings", "ChatConstants", "ChatLocalization"] {
        if dom.find_first_child(modules, name, false).is_none() {
            load(dom, defaults, name, "ModuleScript", modules)?;
        }
    }
    for name in ["MessageCreatorModules", "CommandModules"] {
        let kept = folder(dom, modules, name, true)?;
        if let Some(from) = dom.find_first_child(defaults, name, false)
            && inserts_defaults(dom, kept)
        {
            fill(dom, from, kept)?;
        }
    }
    Ok(())
}

/// The server's script and modules: `ChatServiceInstaller`. Returns Luau
/// that fills the default ChatLocalization, whose entries are a module.
fn install_service(
    dom: &mut Dom,
    chat: InstanceId,
    files: Option<Files>,
) -> Result<Option<String>, DomError> {
    let mut localization = None;
    let mut runner = dom.find_first_child(chat, "ChatServiceRunner", false);
    let saved = runner.is_some();
    if let Some(f) = files {
        if dom
            .find_first_child(chat, "ChatLocalization", false)
            .is_none()
        {
            localization = default_localization(dom, chat, f)?;
        }
        if !saved {
            let new = load(dom, f.server, "ChatServiceRunner", "Script", chat)?;
            for name in ["ChatService", "ChatChannel", "Speaker", "Util"] {
                load(dom, f.server, name, "ModuleScript", new)?;
            }
            runner = Some(new);
        }
        fill_server_modules(dom, chat, f)?;
    }
    if let (Some(runner), Some(service)) = (runner, dom.find_service("ServerScriptService")) {
        copy_out(dom, runner, service)?;
        if !saved {
            keep_unsaved(dom, runner)?;
        }
    }
    Ok(localization)
}

fn default_localization(
    dom: &mut Dom,
    chat: InstanceId,
    f: Files,
) -> Result<Option<String>, DomError> {
    let entries = dom
        .find_first_child(f.server, "DefaultChatLocalization", false)
        .and_then(|module| dom.string_property(module, "Source"));
    let Some(entries) = entries else {
        return Ok(None);
    };
    let table = create(dom, "LocalizationTable", "ChatLocalization", chat)?;
    dom.set_property_by_name(table, "Archivable", Value::Bool(false))?;
    dom.set_property_by_name(table, "SourceLocaleId", Value::String("en-us".into()))?;
    Ok(Some(format!(
        "game:GetService(\"Chat\").ChatLocalization:SetEntries((function()\n{entries}\nend)())"
    )))
}

fn fill_server_modules(dom: &mut Dom, chat: InstanceId, f: Files) -> Result<(), DomError> {
    let modules = folder(dom, chat, "ChatModules", true)?;
    let Some(defaults) = dom.find_first_child(f.server, "DefaultChatModules", false) else {
        return Ok(());
    };
    if inserts_defaults(dom, modules) {
        let utility = folder(dom, modules, "Utility", false)?;
        fill(dom, defaults, modules)?;
        if let Some(from) = dom.find_first_child(defaults, "Utility", false) {
            fill(dom, from, utility)?;
        }
    }
    Ok(())
}

/// Install the chat on a server's DataModel. The runner's copy starts as any
/// new script does; what's returned is Luau left to run.
pub(crate) fn install(dom: &mut Dom, patch: Option<&CorePatch>) -> Option<String> {
    let chat = dom.find_service("Chat")?;
    if !wanted(dom, chat) {
        return None;
    }
    let files = patch.and_then(|p| Files::load(dom, p));
    let installed = (|| {
        install_window(dom, chat, files)?;
        let localization = install_service(dom, chat, files)?;
        if let Some(f) = files {
            dom.destroy(f.holder)?;
        }
        Ok::<_, DomError>(localization)
    })();
    installed.unwrap_or_else(|e| {
        tracing::warn!("installing the chat: {e}");
        None
    })
}
