//! Roblox's R15 rig for characters' bodies: `avatar/characterR15.rbxm`
//! from its client packages or the user's install, read when the first
//! R15 character spawns and never shipped
//! (docs/design/policies.md#content-and-assets). Its parts, Motor6Ds and
//! attachments are the ones Roblox's own characters start from
//! (docs/behavior/characters/rigs.md).

use std::sync::{Arc, OnceLock};

use rbx_dom_weak::WeakDom;
use rook_lua::{RigSource, RigType};

/// The rig file's `rbxasset://` path.
const R15: &str = "rbxasset://avatar/characterR15.rbxm";

/// Bodies for a server, read through `resolver`.
pub(crate) fn source(resolver: Arc<rook_content::Resolver>) -> RigSource {
    let file: Arc<OnceLock<Option<WeakDom>>> = Arc::default();
    RigSource(Arc::new(move |dom, rig| {
        if rig != RigType::R15 {
            return None;
        }
        let weak = file
            .get_or_init(|| {
                let fetched =
                    rook_content::ContentRef::parse(R15).and_then(|r| resolver.resolve(&r).ok());
                let loaded = fetched.and_then(|f| rook_formats::load(&f.bytes).ok());
                if loaded.is_none() {
                    tracing::warn!("{R15} isn't available: characters are R6");
                }
                loaded.map(|l| l.dom)
            })
            .as_ref()?;
        let holder = dom.create_by_name("Folder").ok()?;
        let model = rook_formats::into_dom(weak, dom, holder).into_iter().next();
        if let Some(model) = model {
            rook_dom::ignore_stale(dom.set_parent(model, None));
        }
        rook_dom::ignore_stale(dom.destroy(holder));
        model
    }))
}
