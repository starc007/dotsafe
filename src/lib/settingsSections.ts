import { BadgeCheck, Keyboard, Palette, ShieldCheck, type LucideIcon } from "lucide-react";

export type SettingsSection = "license" | "appearance" | "security" | "shortcuts";

/** The settings sections, in sidebar order. */
export const SETTINGS_SECTIONS: { id: SettingsSection; label: string; description: string; icon: LucideIcon }[] = [
  { id: "license", label: "License", description: "Your dotsafe license on this Mac.", icon: BadgeCheck },
  { id: "appearance", label: "Appearance", description: "Light, dark, or match your Mac.", icon: Palette },
  { id: "security", label: "Security", description: "Your vault, password and how dotsafe protects it.", icon: ShieldCheck },
  { id: "shortcuts", label: "Shortcuts", description: "Everything in dotsafe works from the keyboard.", icon: Keyboard },
];
