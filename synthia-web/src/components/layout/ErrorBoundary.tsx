import { Component, type ErrorInfo, type ReactNode } from 'react';
import { Box, Button, Flex, Heading, Text, Code } from '@radix-ui/themes';

interface Props {
  children: ReactNode;
}

interface State {
  error: Error | null;
}

/**
 * Top-level error boundary. Catches render-phase errors in any
 * descendant and renders a recoverable fallback instead of a
 * blank screen. Lives just inside the router so a thrown error
 * from any page can be caught.
 *
 * Recovery:
 *  - "Reload app"  → window.location.reload() (clean slate)
 *  - "Try again"   → clears state and re-renders the tree
 *
 * The boundary logs the original error + component stack to the
 * browser console for debugging. We deliberately avoid sending
 * the error to a remote service — telemetry is out of MVP scope.
 */
export class ErrorBoundary extends Component<Props, State> {
  override state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  override componentDidCatch(error: Error, info: ErrorInfo): void {
    console.error('[ErrorBoundary] uncaught render error:', error, info.componentStack);
  }

  private readonly handleReload = (): void => {
    window.location.reload();
  };

  private readonly handleReset = (): void => {
    this.setState({ error: null });
  };

  override render(): ReactNode {
    if (this.state.error) {
      return (
        <Box
          style={{
            minHeight: '100vh',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            padding: 'var(--spacing-xl)',
            background: 'var(--bg-primary)',
          }}
        >
          <Flex direction="column" gap="4" align="start" style={{ maxWidth: 560 }}>
            <Heading as="h1" size="6" color="red">
              Something went wrong
            </Heading>
            <Text color="gray" size="2">
              The UI caught an unexpected error. You can try again without losing your session
              state, or reload the page for a clean slate.
            </Text>
            <Box
              style={{
                width: '100%',
                padding: 'var(--spacing-md)',
                background: 'var(--bg-secondary)',
                border: '1px solid var(--border-subtle)',
                borderRadius: 'var(--radius-md)',
                fontFamily: 'var(--font-mono)',
                fontSize: 'var(--fs-sm)',
                overflow: 'auto',
                maxHeight: 240,
              }}
            >
              <Code>{this.state.error.message}</Code>
            </Box>
            <Flex gap="2">
              <Button onClick={this.handleReset}>Try again</Button>
              <Button variant="soft" color="gray" onClick={this.handleReload}>
                Reload app
              </Button>
            </Flex>
          </Flex>
        </Box>
      );
    }
    return this.props.children;
  }
}