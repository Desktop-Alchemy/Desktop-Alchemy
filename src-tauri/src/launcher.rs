use crate::model::ShortcutTarget;
use anyhow::{bail, Result};
use std::path::Path;

pub const WINDOWS_APP_PREFIX: &str = "shell:AppsFolder\\";

pub fn windows_app_id(path: &str) -> Option<&str> {
    path.get(..WINDOWS_APP_PREFIX.len())
        .filter(|prefix| prefix.eq_ignore_ascii_case(WINDOWS_APP_PREFIX))
        .map(|_| &path[WINDOWS_APP_PREFIX.len()..])
}

/// Structural validation only: moved/uninstalled programs must not prevent config loading.
pub fn validate(path: &str, arguments: &str, directory: Option<&str>) -> Result<()> {
    if let Some(id) = windows_app_id(path) {
        let valid_part = |s: &str| {
            !s.is_empty()
                && s.bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
        };
        if id.len() > 256
            || !id
                .split_once('!')
                .is_some_and(|(family, app)| valid_part(family) && valid_part(app))
        {
            bail!(alchemy_device_api::UiFailure::new("backend.b051", "Windows App 标识无效，请重新选择应用"))
        }
        if directory.is_some_and(|s| !s.is_empty()) {
            bail!(alchemy_device_api::UiFailure::new("backend.b052", "Windows Apps 不支持自定义工作目录"))
        }
    } else if !Path::new(path).is_absolute()
        || !matches!(
            Path::new(path)
                .extension()
                .and_then(|s| s.to_str())
                .map(str::to_ascii_lowercase)
                .as_deref(),
            Some("exe" | "lnk")
        )
    {
        bail!(alchemy_device_api::UiFailure::new("backend.b053", "请选择 .exe 程序、.lnk 快捷方式或 Windows App"))
    }
    for value in [path, arguments, directory.unwrap_or("")] {
        if value.contains('\0') || value.encode_utf16().count() > 30_000 {
            bail!(alchemy_device_api::UiFailure::new("backend.b054", "启动配置含无效字符或过长"))
        }
    }
    if directory.is_some_and(|s| !s.is_empty() && !Path::new(s).is_absolute()) {
        bail!(alchemy_device_api::UiFailure::new("backend.b055", "工作目录须为完整路径"))
    }
    Ok(())
}

#[cfg(windows)]
pub(crate) mod platform {
    use super::*;
    use anyhow::Context;
    use windows::{
        core::{Interface, PCWSTR},
        Win32::{
            Foundation::HWND,
            System::Com::*,
            UI::{
                Shell::*,
                WindowsAndMessaging::{SHOW_WINDOW_CMD, SW_SHOWNORMAL},
            },
        },
    };

    pub(super) struct Com;
    impl Com {
        pub(super) fn new() -> Result<Self> {
            unsafe {
                CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
            }
            Ok(Self)
        }
    }
    impl Drop for Com {
        fn drop(&mut self) {
            unsafe {
                CoUninitialize();
            }
        }
    }
    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }
    fn string(buffer: &[u16]) -> Result<String> {
        let end = buffer
            .iter()
            .position(|c| *c == 0)
            .context(alchemy_device_api::UiFailure::new("backend.b056", "快捷方式字段过长"))?;
        Ok(String::from_utf16(&buffer[..end])?)
    }
    fn display_name(item: &IShellItem, kind: SIGDN) -> Result<String> {
        unsafe {
            let value = item.GetDisplayName(kind)?;
            let result = value.to_string();
            CoTaskMemFree(Some(value.0.cast()));
            Ok(result?)
        }
    }
    pub(super) fn installed_apps() -> Result<Vec<crate::model::RegisteredApplication>> {
        let _com = Com::new()?;
        let mut apps = Vec::new();
        unsafe {
            let folder: IShellItem =
                SHGetKnownFolderItem(&FOLDERID_AppsFolder, KF_FLAG_DEFAULT, None)?;
            let items: IEnumShellItems = folder.BindToHandler(None, &BHID_EnumItems)?;
            loop {
                let mut batch = [None];
                let mut fetched = 0;
                items.Next(&mut batch, Some(&mut fetched))?;
                if fetched == 0 {
                    break;
                }
                let item = batch[0].take().context(alchemy_device_api::UiFailure::new("backend.b057", "Windows 应用条目为空"))?;
                // Keep the original Shell identity for desktop and packaged apps alike.
                let Ok(id) = display_name(&item, SIGDN_DESKTOPABSOLUTEPARSING) else {
                    continue;
                };

                let Ok(name) = display_name(&item, SIGDN_NORMALDISPLAY) else {
                    continue;
                };
                if !name.trim().is_empty() {
                    apps.push(crate::model::RegisteredApplication {
                        id: id.clone(),
                        name,
                        target: crate::model::LaunchTarget::RegisteredApplication {
                            identity: crate::model::RegisteredIdentity::AppsFolder {
                                parsing_name: id,
                            },
                            arguments: String::new(),
                        },
                        icon_asset_id: None,
                    });
                }
            }
        }
        apps.sort_by_cached_key(|app| (app.name.to_lowercase(), app.id.to_lowercase()));
        let mut seen = std::collections::HashSet::new();
        apps.retain(|app| seen.insert(app.id.to_lowercase()));
        Ok(apps)
    }
    pub(super) fn registered(
        identity: &crate::model::RegisteredIdentity,
        arguments: &str,
    ) -> Result<()> {
        if let crate::model::RegisteredIdentity::Aumid { id } = identity {
            return run(&format!("{WINDOWS_APP_PREFIX}{id}"), arguments, None);
        }
        let crate::model::RegisteredIdentity::AppsFolder { parsing_name } = identity else {
            unreachable!()
        };
        let _com = Com::new()?;
        let item = apps_folder_item(parsing_name)?;
        unsafe {
            let pidl = SHGetIDListFromObject(&item)?;
            let params = wide(arguments);
            let verb = wide("open");
            let mut info = SHELLEXECUTEINFOW {
                cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
                fMask: SEE_MASK_INVOKEIDLIST | SEE_MASK_FLAG_NO_UI | SEE_MASK_NOASYNC,
                lpIDList: pidl.cast(),
                lpParameters: PCWSTR(params.as_ptr()),
                lpVerb: PCWSTR(verb.as_ptr()),
                nShow: SW_SHOWNORMAL.0,
                ..Default::default()
            };
            let result = ShellExecuteExW(&mut info);
            CoTaskMemFree(Some(pidl.cast()));
            result.context(alchemy_device_api::UiFailure::new("backend.b058", "Windows 未能启动已注册应用"))?;
        }
        Ok(())
    }
    /// Caller owns the COM apartment; resolve the exact original item, including
    /// desktop registrations whose parsing name is not an ordinary file path.
    pub(crate) fn apps_folder_item(parsing_name: &str) -> Result<IShellItem> {
        unsafe {
            let folder: IShellItem =
                SHGetKnownFolderItem(&FOLDERID_AppsFolder, KF_FLAG_DEFAULT, None)?;
            let items: IEnumShellItems = folder.BindToHandler(None, &BHID_EnumItems)?;
            loop {
                let mut batch = [None];
                let mut fetched = 0;
                items.Next(&mut batch, Some(&mut fetched))?;
                if fetched == 0 {
                    bail!(alchemy_device_api::UiFailure::new("backend.b059", "所选应用已不在 AppsFolder 中，请重新选择"))
                }
                if let Some(item) = batch[0].take() {
                    if display_name(&item, SIGDN_DESKTOPABSOLUTEPARSING)
                        .is_ok_and(|name| name.eq_ignore_ascii_case(parsing_name))
                    {
                        return Ok(item);
                    }
                }
            }
        }
    }
    pub(super) fn uri(uri: &str) -> Result<()> {
        let _com = Com::new()?;
        let scheme = wide(uri.split_once(':').context(alchemy_device_api::UiFailure::new("backend.b060", "协议地址无效"))?.0);
        let registered = [
            ASSOCSTR_COMMAND,
            ASSOCSTR_DELEGATEEXECUTE,
            ASSOCSTR_DROPTARGET,
        ]
        .into_iter()
        .any(|kind| unsafe {
            let mut size = 0;
            AssocQueryStringW(
                ASSOCF_IS_PROTOCOL | ASSOCF_NOFIXUPS,
                kind,
                PCWSTR(scheme.as_ptr()),
                PCWSTR::null(),
                None,
                &mut size,
            )
            .is_ok()
                && size > 1
        });
        if !registered {
            bail!(alchemy_device_api::UiFailure::new("backend.b061", "此 URL／协议尚未注册处理程序，请先安装或选择默认应用"))
        }
        let uri = wide(uri);
        let verb = wide("open");
        let mut info = SHELLEXECUTEINFOW {
            cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
            fMask: SEE_MASK_FLAG_NO_UI | SEE_MASK_NOASYNC,
            lpVerb: PCWSTR(verb.as_ptr()),
            lpFile: PCWSTR(uri.as_ptr()),
            nShow: SW_SHOWNORMAL.0,
            ..Default::default()
        };
        unsafe {
            ShellExecuteExW(&mut info)
                .context(alchemy_device_api::UiFailure::new("backend.b062", "Windows 无法打开该 URL／协议，请确认已安装对应应用"))?;
        }
        Ok(())
    }
    pub(super) fn choose(owner: usize) -> Result<Option<String>> {
        let _com = Com::new()?;
        unsafe {
            let dialog: IFileOpenDialog =
                CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)?;
            // The ordinary dialog plugin dereferences .lnk files on Windows,
            // which would silently discard their arguments and working directory.
            dialog.SetOptions(
                FOS_FILEMUSTEXIST
                    | FOS_PATHMUSTEXIST
                    | FOS_FORCEFILESYSTEM
                    | FOS_NODEREFERENCELINKS,
            )?;
            dialog.SetTitle(windows::core::w!("选择程序或快捷方式"))?;
            dialog.SetFileTypes(&[Common::COMDLG_FILTERSPEC {
                pszName: windows::core::w!("程序或快捷方式"),
                pszSpec: windows::core::w!("*.exe;*.lnk"),
            }])?;
            if let Err(e) = dialog.Show(Some(HWND(owner as *mut _))) {
                if e.code() == windows::Win32::Foundation::ERROR_CANCELLED.to_hresult() {
                    return Ok(None);
                }
                return Err(e.into());
            }
            let value = dialog.GetResult()?.GetDisplayName(SIGDN_FILESYSPATH)?;
            let result = value.to_string();
            CoTaskMemFree(Some(value.0.cast()));
            Ok(Some(result?))
        }
    }
    // Called only while the current thread owns a COM apartment.
    fn read_shortcut(path: &str) -> Result<(ShortcutTarget, SHOW_WINDOW_CMD)> {
        unsafe {
            let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
            let persist: IPersistFile = link.cast()?;
            persist.Load(PCWSTR(wide(path).as_ptr()), STGM_READ)?;
            link.Resolve(
                HWND::default(),
                SLR_NO_UI.0 as u32
                    | SLR_NOUPDATE.0 as u32
                    | SLR_NOSEARCH.0 as u32
                    | SLR_NOTRACK.0 as u32,
            )?;
            let mut buffer = vec![0; 32_768];
            link.GetPath(&mut buffer, std::ptr::null_mut(), 0)?;
            let target = string(&buffer)?;
            buffer.fill(0);
            link.GetArguments(&mut buffer)?;
            let arguments = string(&buffer)?;
            buffer.fill(0);
            link.GetWorkingDirectory(&mut buffer)?;
            let directory = string(&buffer)?;
            Ok((
                ShortcutTarget {
                    path: target,
                    arguments,
                    working_directory: if directory.is_empty() {
                        None
                    } else {
                        Some(directory)
                    },
                },
                link.GetShowCmd()?,
            ))
        }
    }
    pub(super) fn resolve(path: &str) -> Result<ShortcutTarget> {
        let _com = Com::new()?;
        let (target, _) = read_shortcut(path)?;
        if !Path::new(&target.path)
            .extension()
            .is_some_and(|s| s.eq_ignore_ascii_case("exe"))
        {
            bail!(alchemy_device_api::UiFailure::new("backend.b063", "快捷方式的目标不是可执行程序"))
        }
        validate(
            &target.path,
            &target.arguments,
            target.working_directory.as_deref(),
        )?;
        if !Path::new(&target.path).is_file() {
            bail!(alchemy_device_api::UiFailure::new("backend.b064", "快捷方式的目标程序不存在，请重新选择"))
        }
        Ok(target)
    }
    pub(super) fn run(path: &str, arguments: &str, directory: Option<&str>) -> Result<()> {
        let _com = Com::new()?;
        if let Some(id) = windows_app_id(path) {
            unsafe {
                let manager: IApplicationActivationManager =
                    CoCreateInstance(&ApplicationActivationManager, None, CLSCTX_INPROC_SERVER)?;
                manager
                    .ActivateApplication(
                        PCWSTR(wide(id).as_ptr()),
                        PCWSTR(wide(arguments).as_ptr()),
                        AO_NOERRORUI,
                    )
                    .context(alchemy_device_api::UiFailure::new("backend.b065", "Windows 未能启动应用，请确认该应用仍已安装"))?;
            }
            return Ok(());
        }
        let mut target = path.to_string();
        let mut params = arguments.to_string();
        let mut cwd = directory.filter(|s| !s.is_empty()).map(str::to_owned);
        let mut show = SW_SHOWNORMAL;
        if Path::new(path)
            .extension()
            .is_some_and(|s| s.eq_ignore_ascii_case("lnk"))
        {
            let (resolved, show_cmd) = read_shortcut(path)?;
            target = resolved.path;
            params = match (resolved.arguments.is_empty(), arguments.is_empty()) {
                (true, _) => arguments.into(),
                (_, true) => resolved.arguments,
                _ => format!("{} {arguments}", resolved.arguments),
            };
            if cwd.is_none() {
                cwd = resolved.working_directory;
            }
            show = show_cmd;
        }
        if !Path::new(&target)
            .extension()
            .is_some_and(|s| s.eq_ignore_ascii_case("exe"))
        {
            bail!(alchemy_device_api::UiFailure::new("backend.b063", "快捷方式的目标不是可执行程序"))
        }
        validate(&target, &params, cwd.as_deref())?;
        if !Path::new(&target).is_file() {
            bail!(alchemy_device_api::UiFailure::new("backend.b066", "程序不存在，请重新选择程序或快捷方式"))
        }
        let cwd = cwd.unwrap_or_else(|| {
            Path::new(&target)
                .parent()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        });
        if !Path::new(&cwd).is_dir() {
            bail!(alchemy_device_api::UiFailure::new("backend.b067", "工作目录不存在"))
        }
        let target = wide(&target);
        let params = wide(&params);
        let cwd = wide(&cwd);
        let verb = wide("open");
        let mut info = SHELLEXECUTEINFOW {
            cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
            fMask: SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI,
            lpVerb: PCWSTR(verb.as_ptr()),
            lpFile: PCWSTR(target.as_ptr()),
            lpParameters: PCWSTR(params.as_ptr()),
            lpDirectory: PCWSTR(cwd.as_ptr()),
            nShow: show.0,
            ..Default::default()
        };
        unsafe {
            ShellExecuteExW(&mut info).context(alchemy_device_api::UiFailure::new("backend.b068", "Windows 未能启动程序"))?;
        }
        Ok(())
    }
}

pub fn choose_program(owner: usize) -> Result<Option<String>> {
    #[cfg(windows)]
    {
        platform::choose(owner)
    }
    #[cfg(not(windows))]
    {
        let _ = owner;
        bail!(alchemy_device_api::UiFailure::new("backend.b069", "选择程序仅支持 Windows"))
    }
}

/// Read a shortcut without launching it or modifying the link or application index.
pub fn resolve_shortcut(path: &str) -> Result<ShortcutTarget> {
    validate(path, "", None)?;
    if !Path::new(path)
        .extension()
        .is_some_and(|s| s.eq_ignore_ascii_case("lnk"))
    {
        bail!(alchemy_device_api::UiFailure::new("backend.b070", "请选择 .lnk 快捷方式"))
    }
    if !Path::new(path).is_file() {
        bail!(alchemy_device_api::UiFailure::new("backend.b071", "快捷方式不存在，请重新选择"))
    }
    #[cfg(windows)]
    {
        platform::resolve(path)
    }
    #[cfg(not(windows))]
    {
        bail!(alchemy_device_api::UiFailure::new("backend.b072", "读取快捷方式仅支持 Windows"))
    }
}

pub fn launch(path: &str, arguments: &str, directory: Option<&str>) -> Result<()> {
    validate(path, arguments, directory)?;
    if windows_app_id(path).is_none() && !Path::new(path).is_file() {
        bail!(alchemy_device_api::UiFailure::new("backend.b073", "程序或快捷方式不存在，请重新选择"))
    }
    #[cfg(windows)]
    {
        platform::run(path, arguments, directory)
    }
    #[cfg(not(windows))]
    {
        bail!(alchemy_device_api::UiFailure::new("backend.b074", "启动程序仅支持 Windows"))
    }
}

pub fn registered_applications() -> Result<Vec<crate::model::RegisteredApplication>> {
    #[cfg(windows)]
    {
        platform::installed_apps()
    }
    #[cfg(not(windows))]
    {
        bail!(alchemy_device_api::UiFailure::new("backend.b075", "Windows Apps 仅支持 Windows"))
    }
}

pub fn launch_registered(
    identity: &crate::model::RegisteredIdentity,
    arguments: &str,
) -> Result<()> {
    #[cfg(windows)]
    {
        platform::registered(identity, arguments)
    }
    #[cfg(not(windows))]
    {
        let _ = (identity, arguments);
        bail!(alchemy_device_api::UiFailure::new("backend.b076", "已注册应用仅支持 Windows"))
    }
}
pub fn open_uri(uri: &str) -> Result<()> {
    #[cfg(windows)]
    {
        platform::uri(uri)
    }
    #[cfg(not(windows))]
    {
        let _ = uri;
        bail!(alchemy_device_api::UiFailure::new("backend.b077", "URL／协议启动仅支持 Windows"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_app_targets_validate_without_installation_and_reject_other_shell_targets() {
        let app = "shell:AppsFolder\\Microsoft.WindowsCalculator_8wekyb3d8bbwe!App";
        assert!(validate(app, "参数 空格", None).is_ok());
        assert!(validate(&app.to_ascii_lowercase(), "", Some("")).is_ok());
        assert!(validate(app, "", Some("C:\\Work")).is_err());
        assert!(validate(app, "a\0b", None).is_err());
        for target in [
            "shell:AppsFolder\\",
            "shell:AppsFolder\\a!",
            "shell:AppsFolder\\!App",
            "shell:AppsFolder\\a!b!c",
            "shell:AppsFolder\\a!../x",
            "shell:AppsFolder\\a!b c",
            "shell:AppsFolder\\a!b\0",
            "shell:startup",
            "https://example.com",
        ] {
            assert!(validate(target, "", None).is_err(), "{target}");
        }
    }
    #[test]
    fn validates_shape_without_requiring_installed_program() {
        let path = std::env::temp_dir().join("不存在 空格.exe");
        assert!(validate(path.to_str().unwrap(), "\"参数 空格\"", None).is_ok());
        assert!(launch(path.to_str().unwrap(), "", None).is_err());
        assert!(validate("relative.exe", "", None).is_err());
        assert!(validate(path.to_str().unwrap(), "a\0b", None).is_err());
        assert!(validate(path.to_str().unwrap(), "", Some("relative")).is_err());
        assert!(validate("https://example.com/a.exe", "", None).is_err());
    }
}
