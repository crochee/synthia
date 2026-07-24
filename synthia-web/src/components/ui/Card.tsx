import type { HTMLAttributes, ReactNode } from 'react';
import './Card.css';

export interface CardProps extends HTMLAttributes<HTMLDivElement> {
  title?: string;
  glow?: 'green' | 'cyan' | 'red' | 'none';
  children: ReactNode;
}

/**
 * Neon Terminal-styled content card.
 * Renders a bordered container with optional title and glow effect.
 */
export function Card({ title, glow = 'green', className, children, ...rest }: CardProps) {
  const classes = ['nt-card', `nt-card--glow-${glow}`, className].filter(Boolean).join(' ');
  return (
    <div className={classes} {...rest}>
      {title && <div className="nt-card__title">{title}</div>}
      <div className="nt-card__body">{children}</div>
    </div>
  );
}
