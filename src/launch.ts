import type { LaunchTarget } from "./bindings";
export const programTarget = (): LaunchTarget => ({
  type: "program",
  path: "",
  arguments: "",
  working_directory: null,
});
export function targetLabel(target: LaunchTarget): string {
  if (target.type === "program") return target.path;
  if (target.type === "uri") return target.uri;
  return target.identity.type === "apps_folder"
    ? target.identity.parsing_name
    : target.identity.id;
}
