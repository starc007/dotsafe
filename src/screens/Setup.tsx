import { useEffect, useState, type FormEvent } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { ArrowLeft, Folder, ShieldCheck } from "lucide-react";
import { api, message, type BucketInput } from "@/lib/api";
import { displayPath } from "@/lib/paths";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Spinner } from "@/components/ui/spinner";
import { Tabs, TabsList, TabsTrigger } from "@/components/motion/tabs";
import { Centered } from "@/components/Centered";
import { SurfaceFooter } from "@/components/Surface";
import { Field } from "@/components/Field";

const MIN_LENGTH = 8;

type Where = "folder" | "bucket";

type Props = {
  defaultPath: string;
  onCreated: (recoveryKey: string) => void;
  onConnected: () => void;
};

/** First run: create a vault, or connect to one, in a synced folder or a bucket. */
export function Setup(props: Props) {
  const [where, setWhere] = useState<Where>("folder");
  // Whether the chosen place already holds a vault, so we're connecting.
  const [connecting, setConnecting] = useState(false);

  return (
    <Centered
      icon={<ShieldCheck aria-hidden />}
      title={connecting ? "Connect to your vault" : "Set up dotsafe"}
      subtitle={
        connecting
          ? `This ${where === "folder" ? "folder" : "bucket"} already has a vault.`
          : `Encrypted on this Mac, ${where === "folder" ? "synced through your own folder" : "stored in your own bucket"}.`
      }
    >
      <Tabs
        value={where}
        onValueChange={(value) => {
          setWhere(value as Where);
          setConnecting(false);
        }}
        className="mb-4"
      >
        <TabsList aria-label="Where to keep the vault" className="w-full bg-muted p-0.5">
          <TabsTrigger value="folder" className="flex-1 py-1 text-[13px]">
            Synced folder
          </TabsTrigger>
          <TabsTrigger value="bucket" className="flex-1 py-1 text-[13px]">
            S3 or R2 bucket
          </TabsTrigger>
        </TabsList>
      </Tabs>
      {where === "folder" ? (
        <FolderSetup {...props} onConnectingChange={setConnecting} />
      ) : (
        <BucketSetup {...props} onConnectingChange={setConnecting} />
      )}
    </Centered>
  );
}

type FormProps = Props & { onConnectingChange: (connecting: boolean) => void };

function FolderSetup({ defaultPath, onCreated, onConnected, onConnectingChange }: FormProps) {
  const [path, setPath] = useState(defaultPath);
  const [hasVault, setHasVault] = useState<boolean | null>(null);
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let current = true;
    setHasVault(null);
    api.folderHasVault(path).then((has) => {
      if (!current) return;
      setHasVault(has);
      onConnectingChange(has);
    });
    return () => {
      current = false;
    };
  }, [path, onConnectingChange]);

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
    const invalid = !hasVault && checkNewPassword(password, confirm);
    if (invalid) return setError(invalid);
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

      <PasswordFields
        connecting={connecting}
        password={password}
        confirm={confirm}
        error={error}
        onPassword={setPassword}
        onConfirm={setConfirm}
      />

      <SurfaceFooter className="pt-1">
        <Button type="submit" size="lg" className="px-4" disabled={busy || !path || hasVault === null}>
          {busy && <Spinner />}
          {connecting ? "Connect" : "Create vault"}
        </Button>
      </SurfaceFooter>
    </form>
  );
}

const EMPTY_BUCKET: BucketInput = {
  endpoint: "",
  region: "auto",
  bucket: "",
  prefix: "dotsafe",
  accessKeyId: "",
  secretAccessKey: "",
};

function BucketSetup({ onCreated, onConnected, onConnectingChange }: FormProps) {
  const [bucket, setBucket] = useState(EMPTY_BUCKET);
  // null until the bucket is checked; then whether it already has a vault.
  const [hasVault, setHasVault] = useState<boolean | null>(null);
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const set = (field: keyof BucketInput) => (e: React.ChangeEvent<HTMLInputElement>) =>
    setBucket((b) => ({ ...b, [field]: e.target.value }));

  async function check(e: FormEvent) {
    e.preventDefault();
    setError(null);
    setBusy(true);
    try {
      const has = await api.bucketHasVault(bucket);
      setHasVault(has);
      onConnectingChange(has);
    } catch (err) {
      setError(message(err));
    } finally {
      setBusy(false);
    }
  }

  async function submit(e: FormEvent) {
    e.preventDefault();
    setError(null);
    const invalid = !hasVault && checkNewPassword(password, confirm);
    if (invalid) return setError(invalid);
    setBusy(true);
    try {
      if (hasVault) {
        await api.connectBucketVault(bucket, password);
        onConnected();
      } else {
        onCreated(await api.createBucketVault(bucket, password));
      }
    } catch (err) {
      setError(message(err));
      setBusy(false);
    }
  }

  function back() {
    setHasVault(null);
    onConnectingChange(false);
    setPassword("");
    setConfirm("");
    setError(null);
  }

  if (hasVault === null) {
    const ready = bucket.endpoint.trim() && bucket.bucket.trim() && bucket.accessKeyId.trim() && bucket.secretAccessKey.trim();
    return (
      <form onSubmit={check} className="flex flex-col gap-4">
        <Field label="Endpoint" htmlFor="s3-endpoint" hint="Cloudflare R2, AWS S3, Backblaze B2 or MinIO.">
          <Input
            id="s3-endpoint"
            autoFocus
            className="h-9"
            placeholder="https://<account-id>.r2.cloudflarestorage.com"
            autoComplete="off"
            spellCheck={false}
            value={bucket.endpoint}
            onChange={set("endpoint")}
          />
        </Field>
        <div className="grid grid-cols-[1fr_7rem] gap-3">
          <Field label="Bucket" htmlFor="s3-bucket">
            <Input id="s3-bucket" className="h-9" autoComplete="off" spellCheck={false} value={bucket.bucket} onChange={set("bucket")} />
          </Field>
          <Field label="Region" htmlFor="s3-region">
            <Input id="s3-region" className="h-9" autoComplete="off" spellCheck={false} value={bucket.region} onChange={set("region")} />
          </Field>
        </div>
        <Field label="Folder in the bucket" htmlFor="s3-prefix">
          <Input id="s3-prefix" className="h-9" autoComplete="off" spellCheck={false} value={bucket.prefix} onChange={set("prefix")} />
        </Field>
        <Field label="Access key ID" htmlFor="s3-key-id">
          <Input
            id="s3-key-id"
            className="h-9 font-mono text-[12.5px] md:text-[12.5px]"
            autoComplete="off"
            spellCheck={false}
            value={bucket.accessKeyId}
            onChange={set("accessKeyId")}
          />
        </Field>
        <Field
          label="Secret access key"
          htmlFor="s3-secret"
          error={error}
          hint="Kept in your Mac's keychain. Give this key access to this bucket only."
        >
          <Input
            id="s3-secret"
            type="password"
            className="h-9"
            autoComplete="off"
            aria-invalid={!!error}
            value={bucket.secretAccessKey}
            onChange={set("secretAccessKey")}
          />
        </Field>
        <SurfaceFooter className="pt-1">
          <Button type="submit" size="lg" className="px-4" disabled={busy || !ready}>
            {busy && <Spinner />}
            Continue
          </Button>
        </SurfaceFooter>
      </form>
    );
  }

  return (
    <form onSubmit={submit} className="flex flex-col gap-4">
      <p className="truncate rounded-full bg-muted/50 px-3 py-2 text-muted-foreground" title={bucket.endpoint}>
        s3://{bucket.bucket.trim()}
        {bucket.prefix.trim() && `/${bucket.prefix.trim().replace(/^\/+|\/+$/g, "")}`}
      </p>
      <PasswordFields
        connecting={hasVault}
        password={password}
        confirm={confirm}
        error={error}
        onPassword={setPassword}
        onConfirm={setConfirm}
        autoFocus
      />
      <SurfaceFooter className="pt-1">
        <Button type="button" variant="ghost" size="lg" onClick={back} disabled={busy}>
          <ArrowLeft aria-hidden />
          Back
        </Button>
        <Button type="submit" size="lg" className="px-4" disabled={busy}>
          {busy && <Spinner />}
          {hasVault ? "Connect" : "Create vault"}
        </Button>
      </SurfaceFooter>
    </form>
  );
}

function PasswordFields({
  connecting,
  password,
  confirm,
  error,
  onPassword,
  onConfirm,
  autoFocus = true,
}: {
  connecting: boolean;
  password: string;
  confirm: string;
  error: string | null;
  onPassword: (value: string) => void;
  onConfirm: (value: string) => void;
  autoFocus?: boolean;
}) {
  return (
    <>
      <Field label="Master password" htmlFor="password" error={error}>
        <Input
          id="password"
          type="password"
          autoComplete={connecting ? "current-password" : "new-password"}
          autoFocus={autoFocus}
          className="h-9"
          value={password}
          aria-invalid={!!error}
          onChange={(e) => onPassword(e.target.value)}
        />
      </Field>
      {!connecting && (
        <Field label="Confirm password" htmlFor="confirm" hint="There's no way to reset it. You'll get a recovery key next.">
          <Input
            id="confirm"
            type="password"
            autoComplete="new-password"
            className="h-9"
            value={confirm}
            onChange={(e) => onConfirm(e.target.value)}
          />
        </Field>
      )}
    </>
  );
}

/** Why a new password can't be used, or null if it's fine. */
function checkNewPassword(password: string, confirm: string): string | null {
  if (password.length < MIN_LENGTH) return `Use at least ${MIN_LENGTH} characters.`;
  if (password !== confirm) return "Passwords don't match.";
  return null;
}
