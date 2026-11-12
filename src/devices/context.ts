import { t } from "../i18n";
import { inject, provide, computed, type InjectionKey } from "vue";
import type { useDesktopAlchemy } from "../store";
import type { HardwareCommand, KeyVisual, DeviceSettings } from "../bindings";
type Store = ReturnType<typeof useDesktopAlchemy>;
const key: InjectionKey<Store> = Symbol("device-context");
export function provideDeviceContext(store: Store, profileId: string) {
  const draft = store.draft;
  const initialPage = store.page;
  const profile = computed(() =>
    store.workspace.devices.find((p) => p.profile_id === profileId),
  );
  const target = <T>(command: string, args: Record<string, unknown> = {}) =>
    store.targetCall<T>(profileId, profile.value?.device_id, command, args);
  const context = new Proxy(store, {
    get(source, property) {
      if (property === "selectedProfileId") return profileId;
      if (property === "currentProfile") return profile.value;
      if (property === "draft")
        return source.selectedProfileId === profileId ? source.draft : draft;
      if (property === "page")
        return source.selectedProfileId === profileId
          ? source.page
          : initialPage;
      if (property === "state") return profile.value?.state ?? source.state;
      if (property === "device") return profile.value?.state.devices[0];
      if (property === "ready")
        return (
          profile.value?.state.connection === "ready" &&
          !profile.value.config_error
        );
      if (property === "modelAppearance")
        return (
          source.settings.model_appearances[profileId] ?? { type: "default" }
        );
      if (property === "call") return target;
      if (property === "hardware")
        return (command: HardwareCommand, allowPersistent = false) =>
          target("hardware", { command, allowPersistent });
      if (property === "setLedSettings")
        return (settings: DeviceSettings) =>
          target("set_led_settings", { settings });
      if (property === "preview")
        return (visual: KeyVisual) => target<string>("preview_key", { visual });
      if (property === "edit")
        return () => {
          if (source.selectedProfileId === profileId) source.edit();
        };
      return Reflect.get(source, property);
    },
  });
  provide(key, context);
  return context;
}
export function useDeviceContext(): Store {
  const value = inject(key);
  if (!value) throw new Error(t("ui.deviceComponentRequiresAnExplicitContext"));
  return value;
}
