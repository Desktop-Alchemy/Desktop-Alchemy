//! User-curated metadata. Deleting metadata never deletes content-addressed assets.
use crate::{model::*, storage};
use anyhow::{bail, Context, Result};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::Mutex,
};

pub struct LibraryStore {
    root: PathBuf,
    state: Mutex<Result<AssetLibrary, String>>,
}

fn name(value: &str) -> Result<()> {
    if value.trim() != value
        || value.is_empty()
        || value.chars().count() > 120
        || value
            .chars()
            .any(|c| c.is_control() || c == '/' || c == '\\')
    {
        bail!(alchemy_device_api::UiFailure::new(
            "backend.b088",
            "名称须为 1–120 字，不能包含斜线、控制字符或首尾空格"
        ))
    }
    Ok(())
}
fn folder_exists(library: &AssetLibrary, id: &Option<String>) -> Result<()> {
    if id
        .as_ref()
        .is_some_and(|id| !library.folders.iter().any(|f| &f.id == id))
    {
        bail!(alchemy_device_api::UiFailure::new(
            "backend.b089",
            "目标目录不存在"
        ))
    }
    Ok(())
}
pub(crate) fn validate(library: &AssetLibrary) -> Result<()> {
    if library.version != 1 {
        bail!(alchemy_device_api::UiFailure::new(
            "backend.b090",
            "不支持的图片素材版本"
        ))
    }
    let mut ids = HashSet::new();
    let mut names = HashSet::new();
    for f in &library.folders {
        name(&f.name)?;
        if f.id.starts_with("plugin:")
            || f.id.is_empty()
            || !ids.insert(&f.id)
            || !names.insert((f.parent_id.as_ref(), f.name.to_lowercase()))
        {
            bail!(alchemy_device_api::UiFailure::new(
                "backend.b091",
                "目录 ID 或同级名称重复"
            ))
        }
        folder_exists(library, &f.parent_id)?;
        let mut visited = HashSet::from([&f.id]);
        let mut parent = f.parent_id.as_ref();
        while let Some(id) = parent {
            if !visited.insert(id) {
                bail!(alchemy_device_api::UiFailure::new(
                    "backend.b092",
                    "目录不能移动到自身或子目录中"
                ))
            }
            parent = library
                .folders
                .iter()
                .find(|f| &f.id == id)
                .context(alchemy_device_api::UiFailure::new(
                    "backend.b093",
                    "目录不存在",
                ))?
                .parent_id
                .as_ref();
        }
    }
    names.clear();
    let mut assets = HashSet::new();
    for i in &library.items {
        name(&i.name)?;
        storage::validate_asset_id(&i.asset_id)?;
        folder_exists(library, &i.folder_id)?;
        if i.id.starts_with("plugin:")
            || i.id.is_empty()
            || !ids.insert(&i.id)
            || !names.insert((i.folder_id.as_ref(), i.name.to_lowercase()))
            || !assets.insert((i.folder_id.as_ref(), &i.asset_id))
        {
            bail!(alchemy_device_api::UiFailure::new(
                "backend.b094",
                "素材 ID、同级名称或内容重复"
            ))
        }
    }
    Ok(())
}
fn unique_name(library: &AssetLibrary, folder: &Option<String>, base: &str) -> String {
    let base: String = base
        .trim()
        .chars()
        .filter(|c| !c.is_control() && *c != '/' && *c != '\\')
        .take(100)
        .collect();
    let base = if base.is_empty() { "图片" } else { &base };
    let mut candidate = base.to_string();
    let mut n = 2;
    while library
        .items
        .iter()
        .any(|i| &i.folder_id == folder && i.name.to_lowercase() == candidate.to_lowercase())
    {
        candidate = format!("{base} ({n})");
        n += 1;
    }
    candidate
}

const BUILTIN_KEY_ICONS: &[(&str, &[u8])] = &[
    (
        "上一页",
        include_bytes!("../resources/key-icons/previous.png"),
    ),
    ("下一页", include_bytes!("../resources/key-icons/next.png")),
    ("返回", include_bytes!("../resources/key-icons/back.png")),
    ("主页", include_bytes!("../resources/key-icons/home.png")),
    ("关闭", include_bytes!("../resources/key-icons/close.png")),
    ("播放", include_bytes!("../resources/key-icons/play.png")),
    ("暂停", include_bytes!("../resources/key-icons/pause.png")),
    ("停止", include_bytes!("../resources/key-icons/stop.png")),
    (
        "音量+",
        include_bytes!("../resources/key-icons/volume-up.png"),
    ),
    (
        "音量−",
        include_bytes!("../resources/key-icons/volume-down.png"),
    ),
    ("静音", include_bytes!("../resources/key-icons/mute.png")),
    (
        "麦克风",
        include_bytes!("../resources/key-icons/microphone.png"),
    ),
    (
        "截图",
        include_bytes!("../resources/key-icons/screenshot.png"),
    ),
    ("录制", include_bytes!("../resources/key-icons/record.png")),
    ("灯光", include_bytes!("../resources/key-icons/light.png")),
    ("计时器", include_bytes!("../resources/key-icons/timer.png")),
    ("锁定", include_bytes!("../resources/key-icons/lock.png")),
    (
        "设置",
        include_bytes!("../resources/key-icons/settings.png"),
    ),
    ("刷新", include_bytes!("../resources/key-icons/refresh.png")),
    (
        "应用切换",
        include_bytes!("../resources/key-icons/switch-apps.png"),
    ),
    (
        "亮度+",
        include_bytes!("../resources/key-icons/brightness-up.png"),
    ),
    (
        "亮度−",
        include_bytes!("../resources/key-icons/brightness-down.png"),
    ),
    ("音乐", include_bytes!("../resources/key-icons/music.png")),
];

fn seed_builtin_key_icons(root: &Path, library: &mut AssetLibrary) -> Result<()> {
    const FOLDER_ID: &str = "builtin-key-icons-v1";

    let folder_id = if library.folders.iter().any(|folder| folder.id == FOLDER_ID) {
        FOLDER_ID.to_owned()
    } else {
        let base = "M18 常用按键";
        let mut folder_name = base.to_owned();
        let mut suffix = 2;
        while library.folders.iter().any(|folder| {
            folder.parent_id.is_none() && folder.name.eq_ignore_ascii_case(&folder_name)
        }) {
            folder_name = format!("{base} ({suffix})");
            suffix += 1;
        }
        library.folders.push(LibraryFolder {
            id: FOLDER_ID.to_owned(),
            parent_id: None,
            name: folder_name,
        });
        FOLDER_ID.to_owned()
    };

    for (name, data) in BUILTIN_KEY_ICONS {
        let asset_id = storage::put_asset(root, data)?;
        if library.items.iter().any(|item| {
            item.folder_id.as_deref() == Some(folder_id.as_str()) && item.asset_id == asset_id
        }) {
            continue;
        }
        let folder = Some(folder_id.clone());
        library.items.push(LibraryItem {
            id: uuid::Uuid::new_v4().to_string(),
            name: unique_name(library, &folder, name),
            folder_id: folder,
            asset_id,
        });
    }
    validate(library)?;
    Ok(())
}

const BUILTIN_SOCIAL_ICONS: &[(&str, &str, &[u8])] = &[
    (
        "color",
        "Facebook",
        include_bytes!("../resources/icon-library/social-media/color/facebook.png"),
    ),
    (
        "color",
        "X",
        include_bytes!("../resources/icon-library/social-media/color/x-twitter.png"),
    ),
    (
        "color",
        "Instagram",
        include_bytes!("../resources/icon-library/social-media/color/instagram.png"),
    ),
    (
        "color",
        "LinkedIn",
        include_bytes!("../resources/icon-library/social-media/color/linkedin.png"),
    ),
    (
        "color",
        "Google",
        include_bytes!("../resources/icon-library/social-media/color/google.png"),
    ),
    (
        "color",
        "YouTube",
        include_bytes!("../resources/icon-library/social-media/color/youtube.png"),
    ),
    (
        "color",
        "Apple",
        include_bytes!("../resources/icon-library/social-media/color/apple.png"),
    ),
    (
        "color",
        "Snapchat",
        include_bytes!("../resources/icon-library/social-media/color/snapchat.png"),
    ),
    (
        "color",
        "Pinterest",
        include_bytes!("../resources/icon-library/social-media/color/pinterest.png"),
    ),
    (
        "color",
        "Medium",
        include_bytes!("../resources/icon-library/social-media/color/medium.png"),
    ),
    (
        "color",
        "GitHub",
        include_bytes!("../resources/icon-library/social-media/color/github.png"),
    ),
    (
        "color",
        "Threads",
        include_bytes!("../resources/icon-library/social-media/color/threads.png"),
    ),
    (
        "color",
        "WhatsApp",
        include_bytes!("../resources/icon-library/social-media/color/whatsapp.png"),
    ),
    (
        "color",
        "Figma",
        include_bytes!("../resources/icon-library/social-media/color/figma.png"),
    ),
    (
        "color",
        "Dribbble",
        include_bytes!("../resources/icon-library/social-media/color/dribbble.png"),
    ),
    (
        "color",
        "Reddit",
        include_bytes!("../resources/icon-library/social-media/color/reddit.png"),
    ),
    (
        "color",
        "Discord",
        include_bytes!("../resources/icon-library/social-media/color/discord.png"),
    ),
    (
        "color",
        "TikTok",
        include_bytes!("../resources/icon-library/social-media/color/tiktok.png"),
    ),
    (
        "color",
        "Tumblr",
        include_bytes!("../resources/icon-library/social-media/color/tumblr.png"),
    ),
    (
        "color",
        "Telegram",
        include_bytes!("../resources/icon-library/social-media/color/telegram.png"),
    ),
    (
        "color",
        "Bluesky",
        include_bytes!("../resources/icon-library/social-media/color/bluesky.png"),
    ),
    (
        "color",
        "Signal",
        include_bytes!("../resources/icon-library/social-media/color/signal.png"),
    ),
    (
        "color",
        "VK",
        include_bytes!("../resources/icon-library/social-media/color/vk.png"),
    ),
    (
        "color",
        "Spotify",
        include_bytes!("../resources/icon-library/social-media/color/spotify.png"),
    ),
    (
        "color",
        "Twitch",
        include_bytes!("../resources/icon-library/social-media/color/twitch.png"),
    ),
    (
        "color",
        "Messenger",
        include_bytes!("../resources/icon-library/social-media/color/messenger.png"),
    ),
    (
        "negative",
        "Facebook",
        include_bytes!("../resources/icon-library/social-media/negative/facebook.png"),
    ),
    (
        "negative",
        "X",
        include_bytes!("../resources/icon-library/social-media/negative/x-twitter.png"),
    ),
    (
        "negative",
        "Instagram",
        include_bytes!("../resources/icon-library/social-media/negative/instagram.png"),
    ),
    (
        "negative",
        "LinkedIn",
        include_bytes!("../resources/icon-library/social-media/negative/linkedin.png"),
    ),
    (
        "negative",
        "Google",
        include_bytes!("../resources/icon-library/social-media/negative/google.png"),
    ),
    (
        "negative",
        "YouTube",
        include_bytes!("../resources/icon-library/social-media/negative/youtube.png"),
    ),
    (
        "negative",
        "Apple",
        include_bytes!("../resources/icon-library/social-media/negative/apple.png"),
    ),
    (
        "negative",
        "Snapchat",
        include_bytes!("../resources/icon-library/social-media/negative/snapchat.png"),
    ),
    (
        "negative",
        "Pinterest",
        include_bytes!("../resources/icon-library/social-media/negative/pinterest.png"),
    ),
    (
        "negative",
        "Medium",
        include_bytes!("../resources/icon-library/social-media/negative/medium.png"),
    ),
    (
        "negative",
        "GitHub",
        include_bytes!("../resources/icon-library/social-media/negative/github.png"),
    ),
    (
        "negative",
        "Threads",
        include_bytes!("../resources/icon-library/social-media/negative/threads.png"),
    ),
    (
        "negative",
        "WhatsApp",
        include_bytes!("../resources/icon-library/social-media/negative/whatsapp.png"),
    ),
    (
        "negative",
        "Figma",
        include_bytes!("../resources/icon-library/social-media/negative/figma.png"),
    ),
    (
        "negative",
        "Dribbble",
        include_bytes!("../resources/icon-library/social-media/negative/dribbble.png"),
    ),
    (
        "negative",
        "Reddit",
        include_bytes!("../resources/icon-library/social-media/negative/reddit.png"),
    ),
    (
        "negative",
        "Discord",
        include_bytes!("../resources/icon-library/social-media/negative/discord.png"),
    ),
    (
        "negative",
        "TikTok",
        include_bytes!("../resources/icon-library/social-media/negative/tiktok.png"),
    ),
    (
        "negative",
        "Tumblr",
        include_bytes!("../resources/icon-library/social-media/negative/tumblr.png"),
    ),
    (
        "negative",
        "Telegram",
        include_bytes!("../resources/icon-library/social-media/negative/telegram.png"),
    ),
    (
        "negative",
        "Bluesky",
        include_bytes!("../resources/icon-library/social-media/negative/bluesky.png"),
    ),
    (
        "negative",
        "Signal",
        include_bytes!("../resources/icon-library/social-media/negative/signal.png"),
    ),
    (
        "negative",
        "VK",
        include_bytes!("../resources/icon-library/social-media/negative/vk.png"),
    ),
    (
        "negative",
        "Spotify",
        include_bytes!("../resources/icon-library/social-media/negative/spotify.png"),
    ),
    (
        "negative",
        "Twitch",
        include_bytes!("../resources/icon-library/social-media/negative/twitch.png"),
    ),
    (
        "negative",
        "Messenger",
        include_bytes!("../resources/icon-library/social-media/negative/messenger.png"),
    ),
];

fn seed_builtin_social_icons(root: &Path, library: &mut AssetLibrary) -> Result<()> {
    const FOLDER_ID: &str = "builtin-figma-social-icons-v1";
    const COLOR_ID: &str = "builtin-figma-social-color-v1";
    const NEGATIVE_ID: &str = "builtin-figma-social-negative-v1";

    let folder_id = if library.folders.iter().any(|folder| folder.id == FOLDER_ID) {
        FOLDER_ID.to_owned()
    } else {
        let base = "Figma 社交媒体图标";
        let mut folder_name = base.to_owned();
        let mut suffix = 2;
        while library.folders.iter().any(|folder| {
            folder.parent_id.is_none() && folder.name.eq_ignore_ascii_case(&folder_name)
        }) {
            folder_name = format!("{base} ({suffix})");
            suffix += 1;
        }
        library.folders.push(LibraryFolder {
            id: FOLDER_ID.to_owned(),
            parent_id: None,
            name: folder_name,
        });
        FOLDER_ID.to_owned()
    };

    for (id, name) in [(COLOR_ID, "彩色"), (NEGATIVE_ID, "反白")].into_iter() {
        if !library.folders.iter().any(|folder| folder.id == id) {
            let parent_id = Some(folder_id.clone());
            let base = name;
            let mut folder_name = base.to_owned();
            let mut suffix = 2;
            while library.folders.iter().any(|folder| {
                folder.parent_id == parent_id && folder.name.eq_ignore_ascii_case(&folder_name)
            }) {
                folder_name = format!("{base} ({suffix})");
                suffix += 1;
            }
            library.folders.push(LibraryFolder {
                id: id.to_owned(),
                parent_id,
                name: folder_name,
            });
        }
    }

    for (variant, name, data) in BUILTIN_SOCIAL_ICONS {
        let folder_id = if *variant == "color" {
            COLOR_ID
        } else {
            NEGATIVE_ID
        };
        let asset_id = storage::put_asset(root, data)?;
        if library
            .items
            .iter()
            .any(|item| item.folder_id.as_deref() == Some(folder_id) && item.asset_id == asset_id)
        {
            continue;
        }
        let folder = Some(folder_id.to_owned());
        library.items.push(LibraryItem {
            id: uuid::Uuid::new_v4().to_string(),
            name: unique_name(library, &folder, name),
            folder_id: folder,
            asset_id,
        });
    }
    validate(library)?;
    Ok(())
}

fn seed_transparent_icons(root: &Path, library: &mut AssetLibrary) -> Result<()> {
    const FOLDER_ID: &str = "builtin-transparent-icons-v1";
    const COLOR_ID: &str = "builtin-transparent-light-v1";
    const NEGATIVE_ID: &str = "builtin-transparent-dark-v1";

    let folder_id = if library.folders.iter().any(|folder| folder.id == FOLDER_ID) {
        FOLDER_ID.to_owned()
    } else {
        let base = "常用按键（透明）";
        let mut folder_name = base.to_owned();
        let mut suffix = 2;
        while library.folders.iter().any(|folder| {
            folder.parent_id.is_none() && folder.name.eq_ignore_ascii_case(&folder_name)
        }) {
            folder_name = format!("{base} ({suffix})");
            suffix += 1;
        }
        library.folders.push(LibraryFolder {
            id: FOLDER_ID.to_owned(),
            parent_id: None,
            name: folder_name,
        });
        FOLDER_ID.to_owned()
    };

    for (id, name) in [(COLOR_ID, "浅色图形"), (NEGATIVE_ID, "深色图形")].into_iter() {
        if !library.folders.iter().any(|folder| folder.id == id) {
            let parent_id = Some(folder_id.clone());
            let base = name;
            let mut folder_name = base.to_owned();
            let mut suffix = 2;
            while library.folders.iter().any(|folder| {
                folder.parent_id == parent_id && folder.name.eq_ignore_ascii_case(&folder_name)
            }) {
                folder_name = format!("{base} ({suffix})");
                suffix += 1;
            }
            library.folders.push(LibraryFolder {
                id: id.to_owned(),
                parent_id,
                name: folder_name,
            });
        }
    }

    for (variant, name, data) in crate::builtin_transparent::ICONS {
        let folder_id = if *variant == "light" {
            COLOR_ID
        } else {
            NEGATIVE_ID
        };
        let asset_id = storage::put_asset(root, data)?;
        if library
            .items
            .iter()
            .any(|item| item.folder_id.as_deref() == Some(folder_id) && item.asset_id == asset_id)
        {
            continue;
        }
        let folder = Some(folder_id.to_owned());
        library.items.push(LibraryItem {
            id: uuid::Uuid::new_v4().to_string(),
            name: unique_name(library, &folder, name),
            folder_id: folder,
            asset_id,
        });
    }
    validate(library)?;
    Ok(())
}

const LEGACY_BUILTIN_KEY_ICON_IDS: &[&str] = &[
    "ecb9ae6b350c72a6af72421c70e59cc4b88a6ab3552f67a39b912ecbeb192d62.png",
    "cdcbb0e9cb504cba7f6c55a94cb412111d7bd7d74ec1281afb1c5095d1b1dc7f.png",
    "3542f437a688df07af4fba041b87f65aaf0077bc2154acf9c0cd2d003d5851a1.png",
    "90af5d057c4e9c6f870f03c5a07b21e4da07f97247710177b5ef5199a268b318.png",
    "915b31b194554a09b092ca128be76ca9d8a407c69582ef8157b74982f57e61c2.png",
    "c4543c4e396d8466e8cb52585660e2cfa63d2a182cfc6de0f1f80f86e99e1fba.png",
    "043e477ef9f956d6cacbc06ac4b7b81672eca9b29545c4c755ce276d44f85a01.png",
    "5493ac4758108711c4a559044594d31145c32980bcc39827821afe81c39c48a6.png",
    "ee8fa9a9f6a3a5f3c71b522b7ceb4b3e76e063da88bf4c54a60c55b09a252027.png",
    "2b2403bd3bea0b9c794cb8d94910a4c9770892348b437f33d718eddf100007a7.png",
    "ffc6ef3b73559d13893ddb7fee2c92bab2e95b147807fe72bbb227d88fe8b100.png",
    "4655589aa42b3038dac65c43eed12d731999505c2d1cf90fc6fc7f8f516f74b2.png",
    "d197df1c85eadc39ac8b562a96d3bed96e398d9bb0f829dcc9cfb185332c5965.png",
    "6b3171b163d0317896d1758b21c8553352f66989ff32e4f9f1753fffd075b572.png",
    "d1488e045ca1f7a6a6ee069cb00954c9fe4809f8b2f0579b277f41ec846288c7.png",
    "07f79152098d4398e6b23486227cb69a9e92f37b5a595191eb878fb47ce580ec.png",
    "4706968fc547f9ee5ff2bb4be3d8f14a23edb94b1844332f32c50cc209d48d2f.png",
    "2f9c60acdbad5c1f4a31ecc00abb59e9374413cc519832c0afb5bbb4db0533d7.png",
    "cfb4b3f830b2f8451d9804e47b57f0c7d85f5a26923516d675eecb63c4f846fd.png",
    "f55193ddaa06df65ed91eee00b13d3ad11f1f565c55461f3b3fc36eb7ec6b048.png",
    "fa5b05913c8f290f6a7ea267256b6f330a0d94f277f40f5a466f155c4f42958c.png",
    "599f858dc126b20aa93e8b72f8cdae9b37b69a24847033d927e8dff4f78afa15.png",
    "7fda901864c7e84951a182870ec970a2737611170d36d2dba0a7c8b55a3312e6.png",
];

fn migrate_builtin_key_icons(root: &Path, library: &mut AssetLibrary) -> Result<bool> {
    const FOLDER_ID: &str = "builtin-key-icons-v1";
    let mut migrated = false;
    for (old_asset_id, (_, data)) in LEGACY_BUILTIN_KEY_ICON_IDS.iter().zip(BUILTIN_KEY_ICONS) {
        if !library.items.iter().any(|item| {
            item.folder_id.as_deref() == Some(FOLDER_ID) && item.asset_id == *old_asset_id
        }) {
            continue;
        }
        let new_asset_id = storage::put_asset(root, data)?;
        for item in library.items.iter_mut().filter(|item| {
            item.folder_id.as_deref() == Some(FOLDER_ID) && item.asset_id == *old_asset_id
        }) {
            item.asset_id = new_asset_id.clone();
            migrated = true;
        }
    }
    Ok(migrated)
}

impl LibraryStore {
    pub fn new(root: &Path, config: &AppConfig) -> Self {
        let loaded = (|| -> Result<AssetLibrary> {
            let path = root.join("library.json");
            let library_existed = path.exists();
            let mut library = if library_existed {
                let data: AssetLibrary = serde_json::from_slice(&std::fs::read(&path)?).context(
                    alchemy_device_api::UiFailure::new(
                        "backend.b095",
                        "图片素材文件损坏，已保留原文件",
                    ),
                )?;
                validate(&data)?;
                data
            } else {
                let mut library = AssetLibrary {
                    version: 1,
                    folders: vec![],
                    items: vec![],
                };
                let folder_id = uuid::Uuid::new_v4().to_string();
                let folder = Some(folder_id.clone());
                let mut seen = HashSet::new();
                for page in config
                    .pages
                    .iter()
                    .filter(|p| p.id != "builtin-brightness-test-v1")
                {
                    for asset_id in page
                        .background_asset
                        .iter()
                        .chain(page.keys.iter().filter_map(|k| k.asset_id.as_ref()))
                    {
                        if seen.insert(asset_id) && storage::asset(root, asset_id).is_ok() {
                            library.items.push(LibraryItem {
                                id: uuid::Uuid::new_v4().to_string(),
                                folder_id: folder.clone(),
                                name: unique_name(
                                    &library,
                                    &folder,
                                    &format!("素材 {}", &asset_id[..8]),
                                ),
                                asset_id: asset_id.clone(),
                            });
                        }
                    }
                }
                if !library.items.is_empty() {
                    library.folders.push(LibraryFolder {
                        id: folder_id,
                        parent_id: None,
                        name: "已有页面素材".into(),
                    });
                }
                library
            };
            let needs_seed = !root.join("builtin-key-icons-v1.json").exists();
            let needs_transparent_seed = !root.join("builtin-transparent-icons-v1.json").exists();
            if needs_transparent_seed {
                seed_transparent_icons(root, &mut library)?;
            }
            let needs_social_seed = !root.join("builtin-figma-social-icons-v1.json").exists();
            if needs_seed {
                seed_builtin_key_icons(root, &mut library)?;
            }
            if needs_social_seed {
                seed_builtin_social_icons(root, &mut library)?;
            }
            let migrated_key_icons = migrate_builtin_key_icons(root, &mut library)?;
            if !library_existed
                || needs_seed
                || needs_social_seed
                || migrated_key_icons
                || needs_transparent_seed
            {
                storage::save_json(root, "library.json", &library)?;
                if needs_seed {
                    storage::save_json(
                        root,
                        "builtin-key-icons-v1.json",
                        &serde_json::json!({ "seeded": true }),
                    )?;
                }
                if needs_transparent_seed {
                    storage::save_json(
                        root,
                        "builtin-transparent-icons-v1.json",
                        &serde_json::json!({"seeded":true}),
                    )?;
                }
                if needs_social_seed {
                    storage::save_json(
                        root,
                        "builtin-figma-social-icons-v1.json",
                        &serde_json::json!({ "seeded": true }),
                    )?;
                }
            }
            Ok(library)
        })()
        .map_err(|e| format!("图片素材无法加载：{e:#}"));
        Self {
            root: root.into(),
            state: Mutex::new(loaded),
        }
    }
    pub fn snapshot(&self) -> Result<AssetLibrary> {
        self.state
            .lock()
            .unwrap()
            .clone()
            .map_err(anyhow::Error::msg)
    }
    fn change(&self, change: impl FnOnce(&mut AssetLibrary) -> Result<()>) -> Result<AssetLibrary> {
        self.change_checked(None, change)
    }
    fn change_checked(
        &self,
        expected: Option<&str>,
        change: impl FnOnce(&mut AssetLibrary) -> Result<()>,
    ) -> Result<AssetLibrary> {
        let mut guard = self.state.lock().unwrap();
        crate::revisions::check(
            expected,
            guard.as_ref().map_err(|e| anyhow::anyhow!("{e}"))?,
        )?;
        let mut next = guard.as_ref().map_err(|e| anyhow::anyhow!("{e}"))?.clone();
        change(&mut next)?;
        validate(&next)?;
        storage::save_json(&self.root, "library.json", &next)?;
        *guard = Ok(next.clone());
        Ok(next)
    }
    pub fn edit(&self, edit: LibraryEdit) -> Result<AssetLibrary> {
        self.edit_checked(edit, None)
    }
    pub fn edit_checked(&self, edit: LibraryEdit, expected: Option<&str>) -> Result<AssetLibrary> {
        let virtual_id = |id: &String| id.starts_with("plugin:");
        let owned = match &edit {
            LibraryEdit::CreateFolder { parent_id, .. } => parent_id.iter().any(virtual_id),
            LibraryEdit::RenameFolder { id, .. } | LibraryEdit::RenameItem { id, .. } => {
                virtual_id(id)
            }
            LibraryEdit::Move {
                folder_ids,
                item_ids,
                parent_id,
            } => folder_ids
                .iter()
                .chain(item_ids)
                .chain(parent_id)
                .any(virtual_id),
            LibraryEdit::Delete {
                folder_ids,
                item_ids,
            } => folder_ids.iter().chain(item_ids).any(virtual_id),
        };
        if owned {
            bail!(alchemy_device_api::UiFailure::new(
                "backend.b096",
                "插件图标目录为只读，请通过插件管理"
            ));
        }
        self.change_checked(expected, |next| {
            match edit {
                LibraryEdit::CreateFolder { parent_id, name } => {
                    next.folders.push(LibraryFolder {
                        id: uuid::Uuid::new_v4().to_string(),
                        parent_id,
                        name,
                    });
                }
                LibraryEdit::RenameFolder { id, name } => {
                    next.folders
                        .iter_mut()
                        .find(|f| f.id == id)
                        .context(alchemy_device_api::UiFailure::new(
                            "backend.b093",
                            "目录不存在",
                        ))?
                        .name = name
                }
                LibraryEdit::RenameItem { id, name } => {
                    next.items
                        .iter_mut()
                        .find(|i| i.id == id)
                        .context(alchemy_device_api::UiFailure::new(
                            "backend.b097",
                            "素材不存在",
                        ))?
                        .name = name
                }
                LibraryEdit::Move {
                    folder_ids,
                    item_ids,
                    parent_id,
                } => {
                    folder_exists(next, &parent_id)?;
                    for id in folder_ids {
                        next.folders
                            .iter_mut()
                            .find(|f| f.id == id)
                            .context(alchemy_device_api::UiFailure::new(
                                "backend.b093",
                                "目录不存在",
                            ))?
                            .parent_id = parent_id.clone();
                    }
                    for id in item_ids {
                        next.items
                            .iter_mut()
                            .find(|i| i.id == id)
                            .context(alchemy_device_api::UiFailure::new(
                                "backend.b097",
                                "素材不存在",
                            ))?
                            .folder_id = parent_id.clone();
                    }
                }
                LibraryEdit::Delete {
                    folder_ids,
                    item_ids,
                } => {
                    for id in &folder_ids {
                        folder_exists(next, &Some(id.clone()))?;
                    }
                    if item_ids
                        .iter()
                        .any(|id| !next.items.iter().any(|i| &i.id == id))
                    {
                        bail!(alchemy_device_api::UiFailure::new(
                            "backend.b097",
                            "素材不存在"
                        ))
                    }
                    let mut removed: HashSet<String> = folder_ids.into_iter().collect();
                    loop {
                        let before = removed.len();
                        for f in &next.folders {
                            if f.parent_id.as_ref().is_some_and(|id| removed.contains(id)) {
                                removed.insert(f.id.clone());
                            }
                        }
                        if before == removed.len() {
                            break;
                        }
                    }
                    next.folders.retain(|f| !removed.contains(&f.id));
                    next.items.retain(|i| {
                        !item_ids.contains(&i.id)
                            && !i.folder_id.as_ref().is_some_and(|id| removed.contains(id))
                    });
                }
            }
            Ok(())
        })
    }
    pub fn attach(
        &self,
        asset_id: String,
        folder_id: Option<String>,
        name: String,
        replace_id: Option<String>,
        expected: &str,
    ) -> Result<AssetLibrary> {
        storage::asset(&self.root, &asset_id)?;
        self.change_checked(Some(expected), |next| {
            folder_exists(next, &folder_id)?;
            if let Some(id) = replace_id {
                let item = next
                    .items
                    .iter_mut()
                    .find(|i| i.id == id)
                    .context("Library item not found")?;
                item.asset_id = asset_id;
            } else if !next
                .items
                .iter()
                .any(|i| i.folder_id == folder_id && i.asset_id == asset_id)
            {
                next.items.push(LibraryItem {
                    id: uuid::Uuid::new_v4().to_string(),
                    name: unique_name(next, &folder_id, &name),
                    folder_id,
                    asset_id,
                });
            }
            Ok(())
        })
    }
    pub fn import(
        &self,
        paths: Vec<String>,
        folder_id: Option<String>,
    ) -> Result<Vec<LibraryImportResult>> {
        folder_exists(&self.snapshot()?, &folder_id)?;
        Ok(paths
            .into_iter()
            .map(|path| {
                let result = (|| -> Result<LibraryItem> {
                    let asset_id = storage::import(&self.root, Path::new(&path))?;
                    let mut item = None;
                    self.change(|next| {
                        folder_exists(next, &folder_id)?;
                        if let Some(existing) = next
                            .items
                            .iter()
                            .find(|i| i.folder_id == folder_id && i.asset_id == asset_id)
                        {
                            item = Some(existing.clone());
                        } else {
                            let entry = LibraryItem {
                                id: uuid::Uuid::new_v4().to_string(),
                                folder_id: folder_id.clone(),
                                name: unique_name(
                                    next,
                                    &folder_id,
                                    Path::new(&path)
                                        .file_name()
                                        .and_then(|s| s.to_str())
                                        .unwrap_or("图片"),
                                ),
                                asset_id,
                            };
                            next.items.push(entry.clone());
                            item = Some(entry);
                        }
                        Ok(())
                    })?;
                    Ok(item.unwrap())
                })();
                match result {
                    Ok(item) => LibraryImportResult {
                        path,
                        item: Some(item),
                        error: None,
                    },
                    Err(e) => LibraryImportResult {
                        path,
                        item: None,
                        error: Some(e.to_string()),
                    },
                }
            })
            .collect())
    }
}
pub fn thumbnail(root: &Path, asset_id: &str) -> Result<String> {
    let original = storage::asset(root, asset_id)?;
    let dir = root.join("cache");
    std::fs::create_dir_all(&dir)?;
    let target = dir.join(format!("library-alpha-{asset_id}.png"));
    if !target.exists() {
        let image = crate::render::resize_image(image::open(original)?, 160, 160, false);
        // Unique temporary file also permits concurrent requests for the same image.
        let tmp = dir.join(format!("library-{}.png", uuid::Uuid::new_v4()));
        image.save(&tmp)?;
        let result = std::fs::rename(&tmp, &target);
        if result.is_err() {
            let _ = std::fs::remove_file(&tmp);
            if !target.exists() {
                result?;
            }
        }
    }
    storage::data_url(&target)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (PathBuf, AppConfig) {
        let root = std::env::temp_dir().join(format!("m18-library-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        image::RgbImage::from_pixel(2, 2, image::Rgb([20, 30, 40]))
            .save(root.join("图片 空格.png"))
            .unwrap();
        (root, crate::devices::default_config())
    }
    #[test]
    fn imports_deduplicates_reports_partial_failures_and_keeps_references() {
        let (root, config) = fixture();
        let store = LibraryStore::new(&root, &config);
        let path = root.join("图片 空格.png").to_string_lossy().into_owned();
        let result = store
            .import(vec![path.clone(), path, "missing.png".into()], None)
            .unwrap();
        assert_eq!(
            result[0].item.as_ref().unwrap().id,
            result[1].item.as_ref().unwrap().id
        );
        assert!(result[2].error.is_some());
        let item = result[0].item.as_ref().unwrap();
        assert!(thumbnail(&root, &item.asset_id)
            .unwrap()
            .starts_with("data:image/png;base64,"));
        store
            .edit(LibraryEdit::Delete {
                folder_ids: vec![],
                item_ids: vec![item.id.clone()],
            })
            .unwrap();
        assert!(storage::asset(&root, &item.asset_id).unwrap().exists());
        let reloaded = LibraryStore::new(&root, &config).snapshot().unwrap();
        assert_eq!(
            reloaded.items.len(),
            BUILTIN_KEY_ICONS.len()
                + BUILTIN_SOCIAL_ICONS.len()
                + crate::builtin_transparent::ICONS.len()
        );
        assert_eq!(
            reloaded
                .items
                .iter()
                .filter(|item| item.folder_id.as_deref() == Some("builtin-key-icons-v1"))
                .count(),
            BUILTIN_KEY_ICONS.len()
        );
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn migration_runs_once_and_ignores_builtin_page() {
        let (root, mut config) = fixture();
        let page_asset = storage::import(&root, &root.join("图片 空格.png")).unwrap();
        config.pages[0].keys[0].asset_id = Some(page_asset.clone());
        config.pages[0].background_asset = Some(page_asset.clone());
        let mut builtin = crate::devices::default_page("测试");
        builtin.id = "builtin-brightness-test-v1".into();
        image::RgbImage::from_pixel(2, 2, image::Rgb([200, 100, 80]))
            .save(root.join("builtin.png"))
            .unwrap();
        builtin.keys[0].asset_id = Some(storage::import(&root, &root.join("builtin.png")).unwrap());
        config.pages.push(builtin);
        let store = LibraryStore::new(&root, &config);
        let data = store.snapshot().unwrap();
        assert!(data
            .folders
            .iter()
            .any(|folder| folder.name == "已有页面素材"));
        let builtin_folder = data
            .folders
            .iter()
            .find(|folder| folder.id == "builtin-key-icons-v1")
            .unwrap();
        assert_eq!(
            data.items
                .iter()
                .filter(|item| { item.folder_id.as_deref() == Some(builtin_folder.id.as_str()) })
                .count(),
            BUILTIN_KEY_ICONS.len()
        );
        store
            .edit(LibraryEdit::Delete {
                folder_ids: vec![data
                    .folders
                    .iter()
                    .find(|folder| folder.name == "已有页面素材")
                    .unwrap()
                    .id
                    .clone()],
                item_ids: vec![],
            })
            .unwrap();
        let reloaded = LibraryStore::new(&root, &config).snapshot().unwrap();
        assert!(!reloaded
            .items
            .iter()
            .any(|item| item.asset_id == page_asset));
        assert_eq!(
            reloaded
                .items
                .iter()
                .filter(|item| item.folder_id.as_deref() == Some("builtin-key-icons-v1"))
                .count(),
            BUILTIN_KEY_ICONS.len()
        );
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn cycles_collisions_and_failed_saves_leave_state_unchanged() {
        let (root, config) = fixture();
        let store = LibraryStore::new(&root, &config);
        let data = store
            .edit(LibraryEdit::CreateFolder {
                parent_id: None,
                name: "A".into(),
            })
            .unwrap();
        let a = data
            .folders
            .iter()
            .find(|folder| folder.name == "A")
            .unwrap()
            .id
            .clone();
        assert!(store
            .edit(LibraryEdit::CreateFolder {
                parent_id: None,
                name: "a".into()
            })
            .is_err());
        let data = store
            .edit(LibraryEdit::CreateFolder {
                parent_id: Some(a.clone()),
                name: "B".into(),
            })
            .unwrap();
        let b = data
            .folders
            .iter()
            .find(|folder| folder.name == "B")
            .unwrap()
            .id
            .clone();
        assert!(store
            .edit(LibraryEdit::Move {
                folder_ids: vec![a.clone()],
                item_ids: vec![],
                parent_id: Some(b)
            })
            .is_err());
        std::fs::create_dir(root.join("library.json.tmp")).unwrap();
        assert!(store
            .edit(LibraryEdit::RenameFolder {
                id: a.clone(),
                name: "C".into()
            })
            .is_err());
        assert_eq!(
            store
                .snapshot()
                .unwrap()
                .folders
                .iter()
                .find(|folder| folder.id == a)
                .unwrap()
                .name,
            "A"
        );
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn corrupt_manifest_is_preserved() {
        let (root, config) = fixture();
        std::fs::write(root.join("library.json"), "broken").unwrap();
        let store = LibraryStore::new(&root, &config);
        assert!(store.snapshot().is_err());
        assert!(store
            .edit(LibraryEdit::CreateFolder {
                parent_id: None,
                name: "A".into()
            })
            .is_err());
        assert_eq!(
            std::fs::read_to_string(root.join("library.json")).unwrap(),
            "broken"
        );
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn same_name_imports_move_conflicts_and_recursive_delete_preserve_assets() {
        let (root, config) = fixture();
        let store = LibraryStore::new(&root, &config);
        std::fs::create_dir(root.join("second")).unwrap();
        image::RgbImage::from_pixel(2, 2, image::Rgb([255, 1, 2]))
            .save(root.join("second/图片 空格.png"))
            .unwrap();
        let first = store
            .import(
                vec![root.join("图片 空格.png").to_string_lossy().into()],
                None,
            )
            .unwrap()[0]
            .item
            .clone()
            .unwrap();
        let second = store
            .import(
                vec![root.join("second/图片 空格.png").to_string_lossy().into()],
                None,
            )
            .unwrap()[0]
            .item
            .clone()
            .unwrap();
        assert_eq!(second.name, "图片 空格.png (2)");
        let data = store
            .edit(LibraryEdit::CreateFolder {
                parent_id: None,
                name: "分类".into(),
            })
            .unwrap();
        let folder = data
            .folders
            .iter()
            .find(|folder| folder.name == "分类")
            .unwrap()
            .id
            .clone();
        let data = store
            .edit(LibraryEdit::CreateFolder {
                parent_id: Some(folder.clone()),
                name: "子目录".into(),
            })
            .unwrap();
        let child = data
            .folders
            .iter()
            .find(|folder| folder.name == "子目录")
            .unwrap()
            .id
            .clone();
        store
            .edit(LibraryEdit::Move {
                folder_ids: vec![],
                item_ids: vec![first.id.clone()],
                parent_id: Some(child.clone()),
            })
            .unwrap();
        store
            .edit(LibraryEdit::RenameItem {
                id: second.id.clone(),
                name: first.name.clone(),
            })
            .unwrap();
        assert!(store
            .edit(LibraryEdit::Move {
                folder_ids: vec![],
                item_ids: vec![second.id.clone()],
                parent_id: Some(child)
            })
            .is_err());
        assert!(store
            .snapshot()
            .unwrap()
            .items
            .iter()
            .find(|i| i.id == second.id)
            .unwrap()
            .folder_id
            .is_none());
        let data = store
            .edit(LibraryEdit::Delete {
                folder_ids: vec![folder.clone()],
                item_ids: vec![],
            })
            .unwrap();
        assert!(!data
            .folders
            .iter()
            .any(|f| f.id == folder || f.name == "子目录"));
        assert!(data.items.iter().any(|item| item.id == second.id));
        assert_eq!(
            data.items
                .iter()
                .filter(|item| item.folder_id.as_deref() == Some("builtin-key-icons-v1"))
                .count(),
            BUILTIN_KEY_ICONS.len()
        );
        assert!(storage::asset(&root, &first.asset_id).is_ok());
        assert!(storage::asset(&root, &second.asset_id).is_ok());
        std::fs::remove_dir_all(root).unwrap();
    }
}
