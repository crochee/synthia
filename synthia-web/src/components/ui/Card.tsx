import { Card as RadixCard, Heading, Text } from '@radix-ui/themes';
import type { ReactNode } from 'react';

export interface CardProps {
  title?: ReactNode;
  /** Visual variant — Radix `variant` prop on Card */
  variant?: 'surface' | 'classic' | 'ghost';
  children: ReactNode;
  className?: string;
}

/**
 * Wrapper around Radix Themes' Card primitive.
 *
 * - `variant="surface"` (default) = white card with subtle border.
 * - `variant="classic"` = background-tinted card.
 * - `variant="ghost"` = borderless card for embedded use.
 */
export function Card({ title, variant = 'surface', className, children }: CardProps) {
  return (
    <RadixCard variant={variant} className={className}>
      {title && (
        <Heading as="h3" size="3" mb="2" weight="medium">
          {title}
        </Heading>
      )}
      <Text as="div" size="2">
        {children}
      </Text>
    </RadixCard>
  );
}
