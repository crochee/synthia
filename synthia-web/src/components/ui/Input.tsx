import { TextField, Text } from '@radix-ui/themes';
import { useId, type InputHTMLAttributes, type ReactNode } from 'react';

export interface InputProps extends InputHTMLAttributes<HTMLInputElement> {
  label?: string;
  error?: string;
  icon?: ReactNode;
  /** Show warning ring via data-state hook */
  warning?: string;
}

/**
 * Text input wrapped in Radix Themes' TextField primitive.
 *
 * Radix's `<TextField.Root>` already renders its own internal
 * `<input>` and merges any input props (`placeholder`, `value`,
 * `onChange`, `data-*`, etc.) onto it — passing a second `<input>`
 * as a child renders two inputs and squashes ours to zero width.
 * So we forward all input-like props directly to `<TextField.Root>`
 * and only use children for optional leading slots.
 *
 * - `error` switches the TextField to `color="red"` (red ring).
 * - `warning` adds `data-state="warning"` on the root (CSS rule for
 *   orange ring lives in page.css — added in Task 7).
 * - `icon` is rendered as a leading slot.
 */
export function Input({ label, error, warning, icon, className, id, ...rest }: InputProps) {
  // Stable, deterministic ID. `useId` is the React-canonical
  // hook for this: it returns a unique string that is the same
  // across renders for the same component instance, even with
  // React 18 strict-mode double-mount. The previous `Math.random`
  // implementation generated a fresh ID on every render — which
  // (1) forced React to mutate the `<label htmlFor>` and `<input id>`
  // attributes each render, (2) broke screen-reader associations
  // because the `<label>` and `<input>` ids drifted apart, and
  // (3) was incompatible with SSR hydration.
  const generatedId = useId();
  const inputId = id ?? `radix-input-${generatedId}`;
  // Radix Themes' TextField.Root accepts a literal union of
  // accent colors ('gray' | 'gold' | 'red' | ...). Map our
  // `error` boolean to `'red'` (red ring) or `undefined`.
  // The `rest` spread can carry broad `InputHTMLAttributes`
  // types that conflict with `TextFieldRootProps` (e.g.
  // `color: string`, `defaultValue: readonly string[]`,
  // `value: readonly string[]`, `type: HTMLInputTypeAttribute`,
  // `size: number`); cast through `unknown` to strip those
  // broad fields. This is the only place a cast is required —
  // the alternative is 4+ field-by-field destructure strips.
  // Pre-existing typecheck error at `Input.tsx:37` was fixed
  // by this cast; verified with `pnpm typecheck` (2026-08-15).
  const color = (error ? 'red' : undefined) as 'red' | undefined;
  const restForTextField = { ...rest } as unknown as Record<string, unknown>;
  return (
    <div className={className} data-state={warning ? 'warning' : undefined}>
      {label && (
        <Text as="label" htmlFor={inputId} size="1" weight="medium" color="gray" mb="1">
          {label}
        </Text>
      )}
      <TextField.Root color={color} id={inputId} {...restForTextField}>
        {icon && <TextField.Slot>{icon}</TextField.Slot>}
      </TextField.Root>
      {error && (
        <Text as="div" size="1" color="red" mt="1">
          {error}
        </Text>
      )}
      {warning && (
        <Text as="div" size="1" color="orange" mt="1">
          {warning}
        </Text>
      )}
    </div>
  );
}
