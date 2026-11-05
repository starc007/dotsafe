import { Laptop, Moon, Palette, Sun } from "lucide-react";
import { useTheme, type ThemePreference } from "@/lib/theme";
import { Tabs, TabsList, TabsTrigger } from "@/components/motion/tabs";
import { SettingsRow } from "@/components/SettingsRow";
import { thisDevice } from "@/lib/platform";
import { Surface, SurfaceBody, SurfaceDescription, SurfaceHeader, SurfaceTitle } from "@/components/Surface";

export function AppearanceSettings() {
  const { preference, setPreference } = useTheme();
  return (
    <Surface aria-labelledby="appearance-title">
      <SurfaceHeader icon={<Palette aria-hidden />}>
        <SurfaceTitle id="appearance-title">Theme</SurfaceTitle>
        <SurfaceDescription>Applies right away and is remembered on {thisDevice}.</SurfaceDescription>
      </SurfaceHeader>
      <SurfaceBody>
        <SettingsRow label="Appearance" detail="System follows your system's light or dark setting.">
          <Tabs variant="pill" value={preference} onValueChange={(v) => setPreference(v as ThemePreference)}>
            <TabsList aria-label="Theme" className="bg-muted p-0.5">
              <TabsTrigger value="system" icon={<Laptop className="size-3.5" />} className="px-2.5 py-1 text-[13px]">
                System
              </TabsTrigger>
              <TabsTrigger value="light" icon={<Sun className="size-3.5" />} className="px-2.5 py-1 text-[13px]">
                Light
              </TabsTrigger>
              <TabsTrigger value="dark" icon={<Moon className="size-3.5" />} className="px-2.5 py-1 text-[13px]">
                Dark
              </TabsTrigger>
            </TabsList>
          </Tabs>
        </SettingsRow>
      </SurfaceBody>
    </Surface>
  );
}
