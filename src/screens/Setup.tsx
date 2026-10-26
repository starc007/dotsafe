import { useEffect, useState, type FormEvent } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { Folder, ShieldCheck } from "lucide-react";
import { api, message } from "@/lib/api";
import { displayPath } from "@/lib/paths";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Spinner } from "@/components/ui/spinner";
import { Centered } from "@/components/Centered";
import { SurfaceFooter } from "@/components/Surface";
import { Field } from "@/components/Field";

const MIN_LENGTH = 8;

export function Setup({
  defaultPath,
  onCreated,
  onConnected,
}: {
  defaultPath: string;
  onCreated: (recoveryKey: string) => void;
  onConnected: () => void;
}) {
  const [path, setPath] = useState(defaultPath);
  const [hasVault, setHasVault] = useState<boolean | null>(null);
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let current = true;
    setHasVault(null);
    api.folderHasVault(path).then((has) => current && setHasVault(has));
    return () => {
      current = false;
    };
  }, [path]);

  async function chooseFolder() {
    const picked = await open({ directory: true, defaultPath: path || undefined, title: "Choose a vault folder" });
    if (typeof picked === "string") {
      setPath(picked);
      setError(null);
    }
  }

  async function submit(e: FormEvent) {
    e.preventDefault();
    setError(null);
    if (!hasVault) {
      if (password.length < MIN_LENGTH) return setError(`Use at least ${MIN_LENGTH} characters.`);
      if (password !== confirm) return setError("Passwords don't match.");
    }
    setBusy(true);
    try {
      if (hasVault) {
        await api.connectVault(path, password);
        onConnected();
      } else {
        onCreated(await api.createVault(path, password));
      }
    } catch (err) {
      setError(message(err));
      setBusy(false);
    }
  }

  const connecting = hasVault === true;
  const inICloud = path.includes("com~apple~CloudDocs");

  return (
    <Centered
      icon={<ShieldCheck aria-hidden />}
      title={connecting ? "Connect to your vault" : "Set up dotsafe"}
      subtitle={connecting ? "This folder already has a vault." : "Encrypted on this Mac, synced through your own folder."}
    >
      <form onSubmit={submit} className="flex flex-col gap-4">
          <Field
            label="Vault folder"
            htmlFor="vault-path"
            hint={
              inICloud
                ? "iCloud Drive keeps it in sync across your Macs."
                : "Any synced folder works: Dropbox, Google Drive, Syncthing."
            }
          >
            <div className="flex gap-2">
              <div
                id="vault-path"
                title={path}
                className="flex h-9 min-w-0 flex-1 items-center gap-2 rounded-full border bg-muted/50 px-3 text-muted-foreground"
              >
                <Folder aria-hidden className="size-3.5 shrink-0" />
                <span className="truncate">{path ? displayPath(path) : "No folder chosen"}</span>
              </div>
              <Button type="button" variant="outline" className="h-9" onClick={chooseFolder}>
                Change…
              </Button>
            </div>
          </Field>

          <Field label="Master password" htmlFor="password" error={error}>
            <Input
              id="password"
              type="password"
              autoComplete={connecting ? "current-password" : "new-password"}
              autoFocus
              className="h-9"
              value={password}
              aria-invalid={!!error}
              onChange={(e) => setPassword(e.target.value)}
            />
          </Field>

          {!connecting && (
            <Field
              label="Confirm password"
              htmlFor="confirm"
              hint="There's no way to reset it. You'll get a recovery key next."
            >
              <Input
                id="confirm"
                type="password"
                autoComplete="new-password"
                className="h-9"
                value={confirm}
                onChange={(e) => setConfirm(e.target.value)}
              />
            </Field>
          )}

        <SurfaceFooter className="pt-1">
          <Button type="submit" size="lg" className="px-4" disabled={busy || !path || hasVault === null}>
            {busy && <Spinner />}
            {connecting ? "Connect" : "Create vault"}
          </Button>
        </SurfaceFooter>
      </form>
    </Centered>
  );
}
