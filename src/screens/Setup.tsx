import { useEffect, useState, type FormEvent } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { Folder } from "lucide-react";
import { api, message } from "@/lib/api";
import { displayPath } from "@/lib/paths";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Spinner } from "@/components/ui/spinner";
import { Centered, Panel } from "@/components/Centered";
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
      title={connecting ? "Connect to your vault" : "Set up dotsafe"}
      subtitle={
        connecting
          ? "This folder already has a vault. Enter its master password to use it on this Mac."
          : "Secrets are encrypted on this Mac with your master password, then synced through a folder you choose."
      }
    >
      <form onSubmit={submit}>
        <Panel>
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
                className="flex h-8 min-w-0 flex-1 items-center gap-2 rounded-lg border bg-muted/50 px-2.5 text-muted-foreground"
              >
                <Folder aria-hidden className="size-3.5 shrink-0" />
                <span className="truncate">{path ? displayPath(path) : "No folder chosen"}</span>
              </div>
              <Button type="button" variant="outline" onClick={chooseFolder}>
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
                value={confirm}
                onChange={(e) => setConfirm(e.target.value)}
              />
            </Field>
          )}

          <Button type="submit" className="mt-1 w-full" disabled={busy || !path || hasVault === null}>
            {busy && <Spinner />}
            {connecting ? "Connect" : "Create vault"}
          </Button>
        </Panel>
      </form>
    </Centered>
  );
}
