import { reactive, toRefs } from "vue";
import type { M18PageTransition, M18TransitionMode } from "../../bindings";

export type SlideDirection = Extract<M18TransitionMode, `slide_${string}`>;

type TransitionDraft = {
  draft: M18PageTransition;
  saved: M18PageTransition | undefined;
  error: string;
  lastSlideDirection: SlideDirection;
};
// Application-lifetime, module-owned editor drafts. Device/navigation changes
// retain failed edits without sharing them with another profile.
const drafts = new Map<string, TransitionDraft>();
export function transitionDraft(profileId: string) {
  let state = drafts.get(profileId);
  if (!state) {
    state = reactive<TransitionDraft>({
      draft: { mode: "none", duration_ms: 300 },
      saved: undefined,
      error: "",
      lastSlideDirection: "slide_left",
    });
    drafts.set(profileId, state);
  }
  return toRefs(state);
}
