//! Shared, typed launch targets. Device adapters never invoke the Shell directly.
use crate::model::{KeyAction, LaunchTarget, RegisteredIdentity};
use anyhow::{bail, Context, Result};

pub fn action_target(action: &KeyAction) -> Option<LaunchTarget> {
    match action {
        KeyAction::Launch { target } => Some(target.clone()),
        _ => None,
    }
}
pub fn validate(target: &LaunchTarget) -> Result<()> {
    let valid_text =
        |text: &str| !text.contains(['\0', '\r', '\n']) && text.encode_utf16().count() <= 30_000;
    match target {
        LaunchTarget::Program {
            path,
            arguments,
            working_directory,
        } => crate::launcher::validate(path, arguments, working_directory.as_deref()),
        LaunchTarget::RegisteredApplication {
            identity,
            arguments,
        } => {
            if !valid_text(arguments) {
                bail!(alchemy_device_api::UiFailure::new("backend.b078", "启动参数无效或过长"))
            }
            match identity {
                RegisteredIdentity::Aumid { id } => crate::launcher::validate(
                    &format!("{}{id}", crate::launcher::WINDOWS_APP_PREFIX),
                    arguments,
                    None,
                ),
                RegisteredIdentity::AppsFolder { parsing_name } => {
                    if parsing_name.trim().is_empty() || !valid_text(parsing_name) {
                        bail!(alchemy_device_api::UiFailure::new("backend.b079", "已注册应用标识无效"))
                    }
                    Ok(())
                }
            }
        }
        LaunchTarget::Uri { uri } => {
            if uri.trim() != uri || !valid_text(uri) || uri.chars().any(char::is_control) {
                bail!(alchemy_device_api::UiFailure::new("backend.b080", "URL／协议地址含无效字符"))
            }
            if uri.chars().any(char::is_whitespace) || uri.contains(['"', '<', '>', '\\', '`']) {
                bail!(alchemy_device_api::UiFailure::new("backend.b081", "URL／协议地址中的特殊字符须编码"))
            }
            let mut bytes = uri.bytes();
            while let Some(byte) = bytes.next() {
                if byte == b'%'
                    && (!bytes.next().is_some_and(|byte| byte.is_ascii_hexdigit())
                        || !bytes.next().is_some_and(|byte| byte.is_ascii_hexdigit()))
                {
                    bail!(alchemy_device_api::UiFailure::new("backend.b082", "URL 编码无效"))
                }
            }
            url::Url::parse(uri).context(alchemy_device_api::UiFailure::new("backend.b083", "URL／协议地址结构无效"))?;
            let Some((scheme, rest)) = uri.split_once(':') else {
                bail!(alchemy_device_api::UiFailure::new("backend.b084", "请输入完整 URL 或协议 URI"))
            };
            if !scheme
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphabetic)
                || !scheme
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"+.-".contains(&b))
            {
                bail!(alchemy_device_api::UiFailure::new("backend.b085", "协议名称无效"))
            }
            if matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https")
                && (!rest.starts_with("//")
                    || rest[2..].split(['/', '?', '#']).next().is_none_or(|host| {
                        host.is_empty() || host.chars().any(char::is_whitespace)
                    }))
            {
                bail!(alchemy_device_api::UiFailure::new("backend.b086", "网页地址缺少有效主机名"))
            }
            Ok(())
        }
    }
}
pub fn execute(target: &LaunchTarget) -> Result<()> {
    validate(target)?;
    match target {
        LaunchTarget::Program {
            path,
            arguments,
            working_directory,
        } => crate::launcher::launch(path, arguments, working_directory.as_deref()),
        LaunchTarget::RegisteredApplication {
            identity,
            arguments,
        } => crate::launcher::launch_registered(identity, arguments),
        LaunchTarget::Uri { uri } => crate::launcher::open_uri(uri),
    }
}
pub fn icon(root: &std::path::Path, target: &LaunchTarget) -> Result<String> {
    validate(target)?;
    match target {
        LaunchTarget::Program { path, .. } => crate::application_icons::cache(root, path),
        LaunchTarget::RegisteredApplication { identity, .. } => {
            crate::application_catalog::ApplicationCatalog::icon(root, identity)
        }
        LaunchTarget::Uri { .. } => bail!(alchemy_device_api::UiFailure::new("backend.b087", "URL／协议使用默认图标")),
    }
}
pub fn refresh_icon(root: &std::path::Path, target: &LaunchTarget) -> Result<String> {
    validate(target)?;
    if let LaunchTarget::RegisteredApplication { identity, .. } = target {
        crate::application_catalog::ApplicationCatalog::refresh_icon(root, identity)
    } else {
        icon(root, target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn uri_validation_accepts_opaque_protocols_without_splitting_arguments() {
        for uri in [
            "msteams:",
            "custom-app://join/a%20b?x=1&y=2#part",
            "https://example.com/a?q=中文",
        ] {
            assert!(validate(&LaunchTarget::Uri { uri: uri.into() }).is_ok());
        }
        for uri in [
            "cmd /c start",
            "https://",
            "1bad:x",
            " custom:x",
            "custom:x\n",
        ] {
            assert!(validate(&LaunchTarget::Uri { uri: uri.into() }).is_err());
        }
    }
}
