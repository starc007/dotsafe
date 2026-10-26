import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, message, type Status } from "./lib/api";
import { Setup } from "./screens/Setup";
import { Recovery } from "./screens/Recovery";
import { Unlock } from "./screens/Unlock";
import { VaultScreen } from "./screens/Vault";
import { Button, DragRegion } from "./components/ui";

type Screen =
  | { name: "loading" }
  | { name: "error"; message: string }
  | { name: "setup"; status: Status }
  | { name: "recovery"; recoveryKey: string }
  | { name: "locked"; status: Status }
  | { name: "vault" };

export default function App() {
  const [screen, setScreen] = useState<Screen>({ name: "loading" });

  const refresh = useCallback(async () => {
    try {
      const status = await api.status();
      if (!status.configured) setScreen({ name: "setup", status });
      else if (!status.unlocked) setScreen({ name: "locked", status });
      else setScreen({ name: "vault" });
    } catch (e) {
      setScreen({ name: "error", message: message(e) });
    }
  }, []);

  useEffect(() => {
    refresh();
    // Locked by idle timeout in Rust, or a command found the vault locked.
    const onLocked = () => refresh();
    window.addEventListener("dotsafe:locked", onLocked);
    const unlisten = listen("vault-locked", onLocked);
    return () => {
      window.removeEventListener("dotsafe:locked", onLocked);
      unlisten.then((f) => f());
    };
  }, [refresh]);

  switch (screen.name) {
    case "loading":
      return <DragRegion className="h-full" />;
    case "error":
      return (
        <div className="flex h-full flex-col items-center justify-center gap-3 p-8 text-center">
          <DragRegion className="fixed inset-x-0 top-0 h-12" />
          <p className="text-sm font-medium">dotsafe couldn't start</p>
          <p className="selectable max-w-sm text-muted-foreground">{screen.message}</p>
          <Button onClick={refresh}>Try again</Button>
        </div>
      );
    case "setup":
      return (
        <Setup
          defaultPath={screen.status.defaultVaultPath ?? ""}
          onCreated={(recoveryKey) => setScreen({ name: "recovery", recoveryKey })}
          onConnected={() => setScreen({ name: "vault" })}
        />
      );
    case "recovery":
      return <Recovery recoveryKey={screen.recoveryKey} onDone={() => setScreen({ name: "vault" })} />;
    case "locked":
      return <Unlock vaultPath={screen.status.vaultPath} onUnlocked={() => setScreen({ name: "vault" })} />;
    case "vault":
      return <VaultScreen onLocked={refresh} />;
  }
}
