import { Component, ErrorInfo, ReactNode } from "react";

interface ErrorBoundaryProps {
  children: ReactNode;
  fallback?: ReactNode;
}

interface ErrorBoundaryState {
  hasError: boolean;
  error: Error | null;
}

export class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  constructor(props: ErrorBoundaryProps) {
    super(props);
    this.state = { hasError: false, error: null };
  }

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, errorInfo: ErrorInfo): void {
    console.error("ErrorBoundary caught:", error, errorInfo);
  }

  render(): ReactNode {
    if (this.state.hasError) {
      if (this.props.fallback) return this.props.fallback;

      return (
        <div className="flex flex-col items-center justify-center min-h-[60vh] p-6 space-y-4">
          <div className="text-center space-y-2">
            <h2 className="text-xl font-bold text-foreground">Failed to load this view</h2>
            <p className="text-sm text-muted-foreground max-w-md">
              An unexpected error occurred while rendering this tab. The rest of the application
              is still functional. Try navigating away and back, or starting a new game.
            </p>
          </div>
          <details className="text-xs text-muted-foreground max-w-lg">
            <summary className="cursor-pointer select-none">Error details</summary>
            <pre className="mt-2 p-3 bg-muted rounded overflow-auto whitespace-pre-wrap">
              {this.state.error?.message ?? "Unknown error"}
            </pre>
          </details>
          <button
            onClick={() => this.setState({ hasError: false, error: null })}
            className="px-4 py-2 rounded bg-primary text-primary-foreground text-sm font-medium hover:opacity-90"
          >
            Try Again
          </button>
        </div>
      );
    }

    return this.props.children;
  }
}
