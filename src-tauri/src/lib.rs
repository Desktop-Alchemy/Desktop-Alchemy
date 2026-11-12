pub mod app;
pub mod application_catalog;
pub mod application_icons;
pub mod applications;
pub mod backup;
mod builtin_transparent;
pub mod core;
pub mod desktop;
pub mod devices;
pub mod i18n;
pub mod input;
pub mod launch_service;
pub mod launcher;
pub mod led;
pub mod library;
pub mod management;
pub mod mcp;
pub mod model;
pub mod plugin_icons;
pub mod plugin_package;
pub mod plugins;
pub mod process_lock;
#[cfg(windows)]
pub mod raw_input;
pub mod render;
pub mod revisions;
pub mod scenes;
pub mod storage;
pub mod tray_panel;
pub mod widgets;

#[cfg(test)]
mod bindings {
    use crate::model::*;
    use alchemy_device_api::{
        ApiError, ControlBinding, DeviceDto, DisplayRegion, ErrorCode, InputControl, InputKind,
        InputTrigger, InputValue,
    };
    use ts_rs::TS;
    #[test]
    #[ignore = "Explicit code generation: pnpm bindings"]
    fn export_types() {
        let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/bindings.ts");
        let mut types = vec![
            DayFilter::decl(),
            SceneTime::decl(),
            SceneTrigger::decl(),
            ProcessMatcher::decl(),
            SceneCondition::decl(),
            Scene::decl(),
            SceneFallback::decl(),
            SceneSettings::decl(),
            ConditionTruth::decl(),
            RunningProgram::decl(),
            SceneState::decl(),
            SceneStatus::decl(),
            InputValue::decl(),
            InputKind::decl(),
            InputControl::decl(),
            DisplayRegion::decl(),
            InputTrigger::decl(),
            ControlBinding::decl(),
            DeviceDto::decl(),
            ErrorCode::decl(),
            ApiError::decl(),
            Verification::decl(),
            Capability::decl(),
            DeviceLayout::decl(),
            DeviceProfile::decl(),
            GlobalConfig::decl(),
            WorkspaceConfig::decl(),
            DeviceState::decl(),
            WorkspaceSnapshot::decl(),
            DeviceDescriptor::decl(),
            DeviceEvent::decl(),
            HardwareCommand::decl(),
            alchemy_device_api::DeviceCommand::decl(),
            alchemy_device_api::DisplayVisual::decl(),
            RegisteredIdentity::decl(),
            LaunchTarget::decl(),
            DeviceDefinition::decl(),
            KeyAction::decl(),
            KeyContentMode::decl(),
            TextPosition::decl(),
            CloseBehavior::decl(),
            ModelAppearance::decl(),
            AppSettings::decl(),
            LanguagePreference::decl(),
            UiLocale::decl(),
            UiLanguageState::decl(),
            UiMessage::decl(),
            UiError::decl(),
            TrayAction::decl(),
            TrayState::decl(),
            BackupSummary::decl(),
            BackupRestoreNotice::decl(),
            PluginIconRef::decl(),
            ImageRef::decl(),
            CatalogFolder::decl(),
            CatalogItem::decl(),
            LibraryCatalog::decl(),
            PluginIconDefinition::decl(),
            PluginIconInfo::decl(),
            LibraryFolder::decl(),
            LibraryItem::decl(),
            AssetLibrary::decl(),
            ApplicationEntry::decl(),
            RegisteredApplication::decl(),
            ShortcutTarget::decl(),
            ApplicationLibrary::decl(),
            ApplicationEdit::decl(),
            LibraryImportResult::decl(),
            LibraryEdit::decl(),
            KeyVisual::decl(),
            WidgetStyle::decl(),
            WidgetBinding::decl(),
            WidgetDefinition::decl(),
            WidgetPresentation::decl(),
            WidgetContent::decl(),
            WidgetData::decl(),
            WidgetSample::decl(),
            WidgetSource::decl(),
            WidgetCatalog::decl(),
            Page::decl(),
            LedMode::decl(),
            DeviceSettings::decl(),
            AppConfig::decl(),
            PluginManifest::decl(),
            PluginInfo::decl(),
            PluginPackageSummary::decl(),
            LogEntry::decl(),
            CommandReceipt::decl(),
            Snapshot::decl(),
        ];
        types.extend(
            crate::devices::built_in_adapters()
                .values()
                .flat_map(|adapter| adapter.public_types()),
        );
        let data=format!("// Generated from Rust model.rs. Run pnpm bindings.\nexport type JsonValue = unknown;\nexport type DeviceCapabilities = Record<string, Capability>;\n{}\n",types.into_iter().map(|s|format!("export {s}")).collect::<Vec<_>>().join("\n"));
        let data = data
            .lines()
            .map(str::trim_end)
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        if std::fs::read_to_string(&out).ok().as_deref() != Some(&data) {
            std::fs::write(out, data).unwrap();
        }
    }
}
