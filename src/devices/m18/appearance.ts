import { t } from "../../i18n";
import { onScopeDispose, ref, watch } from "vue";
import { useDeviceContext } from "../context";
import type { ImageRef } from "../../bindings";

export function useModelImage(image: () => ImageRef | null) {
  const w = useDeviceContext();
  const source = ref("");
  const error = ref("");
  let generation = 0;
  watch(
    [() => JSON.stringify(image()), () => w.library.revision],
    async () => {
      const current = ++generation;
      const reference = image();
      source.value = "";
      error.value = "";
      if (!reference) return;
      const result = await w.call<string>(
        "image_thumbnail",
        { image: reference },
        undefined,
        true,
      );
      if (current !== generation) return;
      if (result) source.value = result;
      else error.value = t("ui.brandingImageUnavailableShowingTheDefault");
    },
    { immediate: true },
  );
  onScopeDispose(() => generation++);
  return { source, error };
}
