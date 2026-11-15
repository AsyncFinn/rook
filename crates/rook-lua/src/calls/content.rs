//! What scripts wait on from outside: ContentProvider requests, assets, and
//! web requests.

use crate::{AssetModel, Runtime, WebAnswer, WebRequest};

impl Runtime {
    /// ContentProvider references to resolve, by request id, asked for
    /// since the last call, with what each must load as.
    pub fn take_content_requests(
        &self,
    ) -> Vec<(u64, rook_content::ContentRef, rook_content::Want)> {
        std::mem::take(&mut self.world_mut().content.outbox)
    }

    /// ContentProvider requests answered without resolving anything
    /// (`(request, succeeded)`), to deliver with the next results.
    pub fn take_content_answers(&self) -> Vec<(u64, bool)> {
        std::mem::take(&mut self.world_mut().content.answered)
    }

    /// Deliver finished content requests (`(request, succeeded)`): their
    /// PreloadAsync callbacks run, and finished PreloadAsync calls return.
    pub fn deliver_content(&self, results: &[(u64, bool)]) {
        if results.is_empty() {
            return;
        }
        self.guarded(|| {
            crate::api::methods::content::preload::deliver(&self.lua, results);
            crate::runtime::threads::drain_deferred(&self.lua);
        });
    }

    /// Assets `require(assetId)` and `InsertService:LoadAsset` wait for
    /// (`(request, asset id)`), asked for since the last call.
    pub fn take_asset_requests(&self) -> Vec<(u64, i64)> {
        std::mem::take(&mut self.world_mut().assets.outbox)
    }

    /// Deliver a fetched asset: the waiting call gets `model`, a Model
    /// holding its instances, and the threads waiting for it resume.
    pub fn deliver_asset(&self, request: u64, model: AssetModel) {
        self.guarded(|| {
            crate::api::methods::content::assets::deliver(&self.lua, request, model);
            crate::runtime::threads::drain_deferred(&self.lua);
        });
    }

    /// Where scripts' web requests go (`rook_platform::endpoints`); set
    /// before scripts run.
    pub fn set_endpoints(&self, endpoints: std::sync::Arc<rook_platform::endpoints::Endpoints>) {
        self.world_mut().web.endpoints = endpoints;
    }

    /// Web requests members wait on, by request id, asked for
    /// since the last call.
    pub fn take_web_requests(&self) -> Vec<(u64, WebRequest)> {
        std::mem::take(&mut self.world_mut().web.outbox)
    }

    /// Deliver a web request's answer: the member waiting on it returns.
    pub fn deliver_web(&self, request: u64, answer: WebAnswer) {
        self.guarded(|| {
            crate::api::methods::web::requests::deliver(&self.lua, request, answer);
            crate::runtime::threads::drain_deferred(&self.lua);
        });
    }
}
