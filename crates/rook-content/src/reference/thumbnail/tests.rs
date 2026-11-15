//! Each form as Studio 0.747 loaded it or didn't
//! (`docs/behavior/content/thumbnails.md`).

use super::*;
use crate::ContentRef;

#[expect(
    clippy::unnecessary_wraps,
    reason = "compared with what ContentRef::parse returns"
)]
fn thumb(kind: ThumbnailKind, target: Target, size: u32) -> Option<ContentRef> {
    Some(ContentRef::Thumbnail(Thumbnail {
        kind,
        target,
        width: size,
        height: size,
        circular: false,
    }))
}

fn asset(size: u32) -> Option<ContentRef> {
    thumb(ThumbnailKind::Asset, Target::Id(5), size)
}

fn user(kind: ThumbnailKind, size: u32) -> Option<ContentRef> {
    thumb(kind, Target::Id(156), size)
}

#[test]
fn rbxthumb_forms() {
    for url in [
        "rbxthumb://type=Asset&id=5&w=150&h=150",
        "rbxthumb://Type=Asset&ID=5&W=150&H=150",
        "rbxthumb://id=5&type=Asset&h=150&w=150",
        "rbxthumb://type=Asset&id= 5&w= 150&h=150",
        "rbxthumb://type=Asset&id=05&w=0150&h=+150",
        "rbxthumb://type=Asset&id=5&w=150&h=150&x=1",
        "rbxthumb://type=Asset&id=5&w=150&h=150 ",
        "rbxthumb://type=Asset&id=5&w=150&h=150&format=jpeg",
        "rbxthumb://type=Asset&id=5&w=150&h=150&filters=bogus",
        // The first of a repeated key wins.
        "rbxthumb://type=Asset&id=5&id=1&w=150&h=150",
        "rbxthumb://type=Asset&id=5&w=150&h=150&w=420",
    ] {
        assert_eq!(ContentRef::parse(url), asset(150), "{url}");
    }
    let circular =
        ContentRef::parse("rbxthumb://type=AvatarHeadShot&id=156&w=48&h=48&filters=circular");
    let Some(ContentRef::Thumbnail(t)) = circular else {
        panic!("{circular:?}")
    };
    assert!(t.circular);
    assert_eq!(
        t.to_string(),
        "rbxthumb://type=AvatarHeadShot&id=156&w=48&h=48&filters=circular"
    );
    assert_eq!(t.cache_key(), "thumb-AvatarHeadShot-156-48x48-circular");
}

#[test]
fn rbxthumb_forms_that_load_nothing() {
    for url in [
        "rbxthumb://type=asset&id=5&w=150&h=150",
        "rbxthumb://type=ASSET&id=5&w=150&h=150",
        "rbxthumb://type= Asset&id=5&w=150&h=150",
        "rbxthumb://type=Asset&id=5",
        "rbxthumb://type=Asset&id=5&w=150",
        "rbxthumb://type=Asset&id=5abc&w=150&h=150",
        "rbxthumb://type=Asset&id=-5&w=150&h=150",
        "rbxthumb://type=Asset&id=0&w=150&h=150",
        " rbxthumb://type=Asset&id=5&w=150&h=150",
        "RBXTHUMB://type=Asset&id=5&w=150&h=150",
        "rbxthumb://type=Bogus&id=5&w=150&h=150",
        "rbxthumb://type=HeadShot&id=156&w=150&h=150",
        "rbxthumb://type=AvatarThumbnail&id=156&w=150&h=150",
        "rbxthumb://type=PlaceIcon&id=1818&w=150&h=150",
        // Sizes the kind doesn't take, though the service would serve some.
        "rbxthumb://type=Asset&id=5&w=30&h=30",
        "rbxthumb://type=Asset&id=5&w=576&h=324",
        "rbxthumb://type=Asset&id=5&w=0&h=0",
        "rbxthumb://type=Asset&id=5&w=420&h=0",
        "rbxthumb://type=AvatarHeadShot&id=156&w=50&h=50",
        "rbxthumb://type=AvatarHeadShot&id=156&w=720&h=720",
        "rbxthumb://type=BadgeIcon&id=1&w=420&h=420",
    ] {
        assert_eq!(ContentRef::parse(url), None, "{url}");
    }
}

#[test]
fn every_kind_takes_its_measured_sizes() {
    let taken = |kind: ThumbnailKind| {
        kind.sizes()
            .iter()
            .filter(|(w, h)| {
                let url = format!("rbxthumb://type={}&id=1&w={w}&h={h}", kind.name());
                ContentRef::parse(&url).is_some()
            })
            .count()
    };
    for kind in ThumbnailKind::ALL {
        assert_eq!(taken(kind), kind.sizes().len(), "{kind:?}");
    }
    assert_eq!(ThumbnailKind::GameIcon.sizes().len(), 5);
    assert!(ThumbnailKind::Avatar.sizes().contains(&(720, 720)));
    assert!(!ThumbnailKind::AvatarHeadShot.sizes().contains(&(720, 720)));
}

#[test]
fn old_asset_urls() {
    for url in [
        "http://www.roblox.com/Game/Tools/ThumbnailAsset.ashx?fmt=png&wd=420&ht=420&aid=5",
        "http://www.roblox.com/Game/Tools/ThumbnailAsset.ashx?wd=420&ht=420&aid=5",
        "http://www.roblox.com/game/tools/thumbnailasset.ashx?fmt=png&wd=420&ht=420&aid=5",
        "https://www.roblox.com/Game/Tools/ThumbnailAsset.ashx?fmt=png&wd=420&ht=420&aid=5",
        "HTTP://www.roblox.com/Game/Tools/ThumbnailAsset.ashx?wd=420&ht=420&aid=5",
        "http://roblox.com/Game/Tools/ThumbnailAsset.ashx?wd=420&ht=420&aid=5",
        "http://example.com/Game/Tools/ThumbnailAsset.ashx?wd=420&ht=420&aid=5",
        "http://www.roblox.com//Game/Tools/ThumbnailAsset.ashx?wd=420&ht=420&aid=5",
        " http://www.roblox.com/Game/Tools/ThumbnailAsset.ashx?wd=420&ht=420&aid=5",
        "http://www.roblox.com/Game/Tools/ThumbnailAsset.ashx?wd=420&ht=420&aid=5 ",
        "rbxhttp://Game/Tools/ThumbnailAsset.ashx?fmt=png&wd=420&ht=420&aid=5",
        "http://www.roblox.com/Game/Tools/ThumbnailAsset.ashx?FMT=png&WD=420&HT=420&AID=5",
        "http://www.roblox.com/Game/Tools/ThumbnailAsset.ashx?fmt=gif&wd=420&ht=420&aid= 5",
        "http://www.roblox.com/Game/Tools/ThumbnailAsset.ashx?wd=+420&ht=0420&aid=+05",
        "http://www.roblox.com/Game/Tools/ThumbnailAsset.ashx?fmt=png&wd=420&ht=420&assetid=5",
        "http://www.roblox.com/Game/Tools/ThumbnailAsset.ashx?width=420&height=420&aid=5",
        "http://www.roblox.com/Game/Tools/ThumbnailAsset.ashx?x=420&y=420&aid=5",
        "http://www.roblox.com/Game/Tools/ThumbnailAsset.ashx?wd=420&y=420&aid=5",
        // The first `aid` wins, and `assetid` wins over `aid`.
        "http://www.roblox.com/Game/Tools/ThumbnailAsset.ashx?wd=420&ht=420&aid=5&aid=1",
        "http://www.roblox.com/Game/Tools/ThumbnailAsset.ashx?wd=420&ht=420&aid=1&assetid=5",
        "http://www.roblox.com/thumbs/asset.ashx?assetid=5&x=420&y=420",
        "http://www.roblox.com/Thumbs/Asset.ashx?AssetID=5&Width=420&Height=420",
        "http://www.roblox.com/thumbs/asset.ashx?aid=5&x=420&y=420&format=png",
        "rbxhttp://thumbs/asset.ashx?assetid=5&x=420&y=420",
        "https://www.roblox.com/asset-thumbnail/image?assetId=5&width=420&height=420&format=png",
        "https://www.roblox.com/asset-thumbnail/image?aid=5&width=420&height=420",
    ] {
        assert_eq!(ContentRef::parse(url), asset(420), "{url}");
    }
}

#[test]
fn old_asset_urls_snap_to_150_420_or_700() {
    let url = |w: i64, h: i64| {
        ContentRef::parse(&format!(
            "http://www.roblox.com/Game/Tools/ThumbnailAsset.ashx?fmt=png&wd={w}&ht={h}&aid=5"
        ))
    };
    for (asked, got) in [
        (-5, 150),
        (0, 150),
        (20, 150),
        (130, 150),
        (285, 150),
        (286, 420),
        (560, 420),
        (561, 700),
        (735, 700),
        (99999, 700),
    ] {
        assert_eq!(url(asked, asked), asset(got), "{asked}");
    }
}

#[test]
fn old_avatar_urls() {
    use ThumbnailKind::{Avatar, AvatarBust, AvatarHeadShot, Outfit};
    let r = "http://www.roblox.com/";
    for (path, want) in [
        (
            "thumbs/avatar.ashx?userId=156&x=420&y=420&format=png",
            user(Avatar, 420),
        ),
        (
            "thumbs/avatar.ashx?userid=156&width=420&height=420",
            user(Avatar, 420),
        ),
        (
            "thumbs/avatar.ashx?userId=156&wd=420&ht=420&format=jpeg",
            user(Avatar, 420),
        ),
        (
            "avatar-thumbnail/image?userId=156&width=30&height=30",
            user(Avatar, 30),
        ),
        (
            "headshot-thumbnail/image?userId=156&width=420&height=420",
            user(AvatarHeadShot, 420),
        ),
        (
            "Headshot-Thumbnail/Image?UserId=156&Width=48&Height=48&Format=png",
            user(AvatarHeadShot, 48),
        ),
        (
            "headshot-thumbnail/image?userId=156&x=420&y=420",
            user(AvatarHeadShot, 420),
        ),
        (
            "bust-thumbnail/image?userId=156&width=420&height=420",
            user(AvatarBust, 420),
        ),
        // Bust thumbnails stop at 420.
        (
            "bust-thumbnail/image?userId=156&width=720&height=720",
            user(AvatarBust, 420),
        ),
        (
            "outfit-thumbnail/image?userOutfitId=1&width=420&height=420",
            thumb(Outfit, Target::Id(1), 420),
        ),
    ] {
        assert_eq!(ContentRef::parse(&format!("{r}{path}")), want, "{path}");
    }
    for (asked, got) in [
        (30, 48),
        (165, 150),
        (200, 180),
        (300, 352),
        (400, 420),
        (600, 720),
        (1000, 720),
    ] {
        let url = format!("{r}headshot-thumbnail/image?userId=156&width={asked}&height={asked}");
        assert_eq!(
            ContentRef::parse(&url),
            user(AvatarHeadShot, got),
            "{asked}"
        );
    }
    for (asked, got) in [(30, 30), (75, 75), (110, 110)] {
        let url = format!("{r}thumbs/avatar.ashx?userId=156&x={asked}&y={asked}");
        assert_eq!(ContentRef::parse(&url), user(Avatar, got), "{asked}");
    }
}

#[test]
fn usernames_win_over_user_ids() {
    let name = |n: &str| Target::Username(n.into());
    let r = "http://www.roblox.com/";
    for (path, want) in [
        (
            "Thumbs/Avatar.ashx?x=100&y=100&username=builderman",
            name("builderman"),
        ),
        (
            "thumbs/avatar.ashx?username=Ann_1&userId=156&x=100&y=100",
            name("Ann_1"),
        ),
        (
            "thumbs/avatar.ashx?userId=156&username=Ann_1&x=100&y=100",
            name("Ann_1"),
        ),
    ] {
        assert_eq!(
            ContentRef::parse(&format!("{r}{path}")),
            thumb(ThumbnailKind::Avatar, want, 100),
            "{path}"
        );
    }
    let headshot = ContentRef::parse(&format!(
        "{r}headshot-thumbnail/image?username=builderman&width=420&height=420"
    ))
    .unwrap();
    assert_eq!(
        headshot.to_string(),
        "https://www.roblox.com/headshot-thumbnail/image?username=builderman&width=420&height=420"
    );
    assert_eq!(
        ContentRef::parse(&headshot.to_string()),
        Some(headshot.clone())
    );
    assert_eq!(
        headshot.cache_key().as_deref(),
        Some("thumb-AvatarHeadShot-user-builderman-420x420")
    );
    // Names Roblox can't have name no one.
    assert_eq!(
        ContentRef::parse(&format!("{r}thumbs/avatar.ashx?username=../x&x=100&y=100")),
        None
    );
}

#[test]
fn old_urls_that_load_nothing() {
    let r = "http://www.roblox.com/";
    for path in [
        // No size, or half of one.
        "Game/Tools/ThumbnailAsset.ashx?aid=5",
        "Game/Tools/ThumbnailAsset.ashx?fmt=png&wd=420&aid=5",
        "Game/Tools/ThumbnailAsset.ashx?fmt=png&ht=420&aid=5",
        "Game/Tools/ThumbnailAsset.ashx?w=420&h=420&aid=5",
        "thumbs/asset.ashx?assetid=5",
        "thumbs/asset.ashx?assetid=5&x=420",
        "thumbs/avatar.ashx?userId=156",
        "headshot-thumbnail/image?userId=156",
        // Bad numbers and ids.
        "Game/Tools/ThumbnailAsset.ashx?fmt=png&wd=abc&ht=420&aid=5",
        "Game/Tools/ThumbnailAsset.ashx?fmt=png&wd=420&ht=420&aid=5abc",
        "Game/Tools/ThumbnailAsset.ashx?fmt=png&wd=420&ht=420&id=5",
        "thumbs/asset.ashx?id=5&x=420&y=420",
        "thumbs/avatar.ashx?id=156&x=420&y=420",
        "thumbs/avatar.ashx?userId=0&x=420&y=420",
        "thumbs/asset.ashx?assetid=5&x=420&y=420#frag",
        "outfit-thumbnail/image?outfitId=1&width=420&height=420",
        // Other paths.
        "Game/Tools/ThumbnailAsset.ashx/?fmt=png&wd=420&ht=420&aid=5",
        "ThumbnailAsset.ashx?fmt=png&wd=420&ht=420&aid=5",
        "x/ThumbnailAsset.ashx?fmt=png&wd=420&ht=420&aid=5",
        "Game/Tools/Foo.ashx?wd=420&ht=420&aid=5",
        "headshot-thumbnail/image/?userId=156&width=420&height=420",
        "thumbs/headshot.ashx?userId=156&x=420&y=420",
        "Thumbs/BCOverlay.ashx?username=builderman",
        "Thumbs/GroupIcon.ashx?groupid=1&x=150&y=150",
        "group-thumbnail/image?groupId=1&width=150&height=150",
        "badge-thumbnail/image?badgeId=1&width=150&height=150",
        "game-pass-thumbnail/image?gamePassId=1&width=150&height=150",
        "place-thumbnail/image?placeId=1&width=420&height=420",
        "foo/?id=5",
    ] {
        assert_eq!(ContentRef::parse(&format!("{r}{path}")), None, "{path}");
    }
    assert_eq!(
        ContentRef::parse("www.roblox.com/Game/Tools/ThumbnailAsset.ashx?wd=420&ht=420&aid=5"),
        None
    );
}
