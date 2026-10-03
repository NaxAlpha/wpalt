import { h } from "preact";
import { useId, useState, useEffect } from "preact/hooks";
import { breakpoint } from "./design-tokens.json";

// Shared semantics and states. Native elements preserve keyboard and form behavior.
export function Button({
  children,
  variant,
  busy = false,
  disabled,
  ...props
}) {
  return (
    <button
      type="button"
      {...props}
      class={props.class || variant || ""}
      disabled={disabled || busy}
      aria-busy={busy || undefined}
    >
      {children}
    </button>
  );
}
export function Field({
  label,
  description,
  error,
  children,
  multiline = false,
  ...input
}) {
  const id = useId();
  const hint = error || description;
  const Control = multiline ? "textarea" : "input";
  return (
    <div class="ui-field">
      <label for={id}>{label}</label>
      <Control
        {...input}
        id={id}
        aria-invalid={error ? "true" : undefined}
        aria-describedby={hint ? id + "-hint" : undefined}
      />
      {hint && (
        <small id={id + "-hint"} class={error ? "ui-field-error" : "help"}>
          {hint}
        </small>
      )}
      {children}
    </div>
  );
}
export function SelectField({ label, description, error, children, ...props }) {
  const id = useId();
  const hint = error || description;
  return (
    <div class="ui-field">
      <label for={id}>{label}</label>
      <select
        {...props}
        id={id}
        aria-invalid={error ? "true" : undefined}
        aria-describedby={hint ? id + "-hint" : undefined}
      >
        {children}
      </select>
      {hint && (
        <small id={id + "-hint"} class={error ? "ui-field-error" : "help"}>
          {hint}
        </small>
      )}
    </div>
  );
}
export function Notice({ children, error = false }) {
  return (
    <p
      class={"notice" + (error ? " error" : "")}
      role={error ? "alert" : "status"}
    >
      {children}
    </p>
  );
}

export function Disclosure({
  summary,
  children,
  collapseOnNarrow = false,
  initialOpen = false,
}) {
  const query = `(max-width: ${breakpoint.mobile}px)`;
  const [open, setOpen] = useState(() =>
    collapseOnNarrow ? !matchMedia(query).matches : initialOpen,
  );
  useEffect(() => {
    if (!collapseOnNarrow) return;
    const media = matchMedia(query);
    const change = () => setOpen(!media.matches);
    media.addEventListener("change", change);
    return () => media.removeEventListener("change", change);
  }, [collapseOnNarrow, query]);
  return (
    <details
      open={open}
      onToggle={(event) => setOpen(event.currentTarget.open)}
    >
      <summary>{summary}</summary>
      {children}
    </details>
  );
}
