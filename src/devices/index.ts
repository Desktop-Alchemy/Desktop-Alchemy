import GenericPreview from "./GenericPreview.vue";
import GenericControls from "./GenericControls.vue";
import { markRaw } from "vue";
import { m18Views } from "./m18/views";
import { audioDockViews } from "./ms-audio-dock/views";
import type { DeviceViews } from "./types";
export const deviceViews: Record<string, DeviceViews> = {
  "microsoft.audio-dock": audioDockViews,
  "mirabox.m18": m18Views,
};
export function viewsFor(adapterId?: string) {
  const declared = deviceViews[adapterId ?? ""];
  return {
    preview: markRaw(GenericPreview),
    controls: markRaw(GenericControls),
    ...declared,
    tests: declared?.tests ?? false,
  };
}
