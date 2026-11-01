import type { ServiceField } from "@/lib/api";
import { Input } from "@/components/ui/input";
import { Field } from "@/components/Field";

/** The settings a service asks for, as labelled inputs. */
export function ServiceFields({
  fields,
  values,
  idPrefix,
  className,
  onChange,
}: {
  fields: ServiceField[];
  values: Record<string, string>;
  idPrefix: string;
  /** For the inputs, e.g. a card background inside a muted panel. */
  className?: string;
  onChange: (values: Record<string, string>) => void;
}) {
  return (
    <>
      {fields.map((f) => (
        <Field key={f.key} label={f.required ? f.label : `${f.label} (optional)`} htmlFor={`${idPrefix}-${f.key}`} hint={f.hint}>
          <Input
            id={`${idPrefix}-${f.key}`}
            autoComplete="off"
            spellCheck={false}
            className={className ?? "h-9"}
            value={values[f.key] ?? ""}
            onChange={(e) => onChange({ ...values, [f.key]: e.target.value })}
          />
        </Field>
      ))}
    </>
  );
}

/** Whether every required field has a value. */
export function fieldsComplete(fields: ServiceField[], values: Record<string, string>) {
  return fields.every((f) => !f.required || (values[f.key] ?? "").trim() !== "");
}
