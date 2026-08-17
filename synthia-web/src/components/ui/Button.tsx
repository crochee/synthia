import { Button as RadixButton } from '@radix-ui/themes';
import type { ButtonHTMLAttributes, ReactNode } from 'react';

type ButtonVariant = 'classic' | 'solid' | 'soft' | 'surface' | 'outline' | 'ghost';
type ButtonSize = '1' | '2' | '3' | '4';
type ButtonColor = 'blue' | 'green' | 'red' | 'gray';

export interface ButtonProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, 'color'> {
  variant?: ButtonVariant;
  size?: ButtonSize;
  color?: ButtonColor;
  loading?: boolean;
  children: ReactNode;
}

/**
 * Wrapper around Radix Themes' Button.
 *
 * - `variant` maps to Radix Button variants.
 * - `loading` shows a built-in spinner and disables the button.
 * - `color` defaults to `blue` (Radix accent color).
 */
export function Button({
  variant = 'solid',
  size = '2',
  color = 'blue',
  loading = false,
  disabled,
  className,
  children,
  ...rest
}: ButtonProps) {
  return (
    <RadixButton
      variant={variant}
      size={size}
      color={color}
      loading={loading}
      disabled={disabled || loading}
      className={className}
      {...rest}
    >
      {children}
    </RadixButton>
  );
}
