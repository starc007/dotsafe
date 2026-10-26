import { KeyRound } from "lucide-react";
import { Centered } from "@/components/Centered";
import { RecoveryKeyPanel } from "@/components/RecoveryKeyPanel";

export function Recovery({ recoveryKey, onDone }: { recoveryKey: string; onDone: () => void }) {
  return (
    <Centered
      icon={<KeyRound aria-hidden />}
      title="Save your recovery key"
      subtitle="The only way back in if you forget your password."
    >
      <RecoveryKeyPanel recoveryKey={recoveryKey} onDone={onDone} />
    </Centered>
  );
}
