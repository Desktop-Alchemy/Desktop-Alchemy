import {
  computed,
  ref,
  watch,
  onScopeDispose,
  inject,
  provide,
  type InjectionKey,
} from "vue";
import { useDeviceContext } from "../context";
import { errorText, errorDetail, messageText, t } from "../../i18n";
import type { AudioDockState, AudioDockSettingsUpdate } from "../../bindings";
type DockContext = ReturnType<typeof createAudioDock>;
const dockKey: InjectionKey<DockContext> = Symbol("audio-dock");
export function useAudioDock(): DockContext {
  const existing = inject(dockKey);
  if (existing) return existing;
  const context = createAudioDock();
  provide(dockKey, context);
  return context;
}
function createAudioDock() {
  const base = useDeviceContext();
  const value = ref<AudioDockState>();
  const busy = ref(false);
  const error = ref("");
  const diagnostic = ref("");
  let generation = 0;
  async function request(update?: AudioDockSettingsUpdate) {
    const deviceId = base.currentProfile?.device_id;
    if (!deviceId || !base.ready || !base.desktop || busy.value) return;
    const before = generation;
    busy.value = true;
    error.value = "";
    diagnostic.value = "";
    try {
      const state = await base.moduleRequest<AudioDockState>(
        deviceId,
        update ? "update" : "read",
        update ?? null,
      );
      if (before === generation) {
        value.value = state;
        error.value = state.error
          ? state.ui_message
            ? messageText(state.ui_message)
            : t("errors.deviceFailed")
          : "";
        diagnostic.value = state.error ?? "";
      }
      return state;
    } catch (e) {
      if (before === generation) {
        error.value = errorText(e);
        diagnostic.value = errorDetail(e);
      }
    } finally {
      if (before === generation) busy.value = false;
    }
  }
  watch(
    // Compare source values, not a new array on every workspace snapshot.
    [() => base.currentProfile?.device_id, () => base.ready],
    () => {
      generation++;
      busy.value = false;
      value.value = undefined;
      if (base.ready) void request();
    },
    { immediate: true },
  );
  onScopeDispose(() => generation++);
  return new Proxy(base, {
    get(target, key) {
      if (key === "audioDockState")
        return (
          value.value ??
          (base.currentProfile?.module_state as AudioDockState | undefined)
        );
      if (key === "audioDockPending") return busy.value;
      if (key === "audioDockError") return error.value;
      if (key === "audioDockErrorDetail") return diagnostic.value;
      if (key === "refreshAudioDock") return () => request();
      if (key === "updateAudioDock") return request;
      return Reflect.get(target, key);
    },
  }) as typeof base & {
    audioDockState: AudioDockState | undefined;
    audioDockPending: boolean;
    audioDockError: string;
    audioDockErrorDetail: string;
    refreshAudioDock: () => Promise<AudioDockState | undefined>;
    updateAudioDock: (
      update: AudioDockSettingsUpdate,
    ) => Promise<AudioDockState | undefined>;
  };
}
