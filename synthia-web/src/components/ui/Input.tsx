import type { InputHTMLAttributes, ReactNode } from 'react';
import './Input.css';

export interface InputProps extends InputHTMLAttributes<HTMLInputElement> {
  label?: string;
  error?: string;
  icon?: ReactNode;
}

/**
 * Neon Terminal-styled text input.
 * Includes optional label, error message, and leading icon.
 */
export function Input({ label, error, icon, className, id, ...rest }: InputProps) {
  const inputId = id ?? `nt-input-${Math.random().toString(36).slice(2, 9)}`;
  return (
    <div className={`nt-input-wrapper ${className ?? ''}`}>
      {label && (
        <label htmlFor={inputId} className="nt-input-label">
          {label}
        </label>
      )}
      <div className={`nt-input-container ${error ? 'nt-input--error' : ''}`}>
        {icon && <span className="nt-input-icon">{icon}</span>}
        <input id={inputId} className="nt-input" {...rest} />
      </div>
      {error && <span className="nt-input-error">{error}</span>}
    </div>
  );
}
