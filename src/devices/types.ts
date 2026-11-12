import type { Component } from "vue";
import type { IconName } from "../icons";

export interface DeviceViews {
  preview?: Component;
  details?: Component;
  poster?: string;
  icon?: IconName;
  controls?: Component;
  tests: Component | false;
  controlLabel?: string;
  sections?: { id: string; icon: IconName; label: string }[];
}
