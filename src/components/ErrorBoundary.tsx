// Dad Cam - Phase 3 Error Boundary Component
// Uses Braun Design Language tokens
import { Component, ReactNode } from 'react';

interface Props {
  children: ReactNode;
  fallback?: ReactNode;
}

interface State {
  hasError: boolean;
  error: Error | null;
}

export class ErrorBoundary extends Component<Props, State> {
  state: State = { hasError: false, error: null };

  static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, errorInfo: React.ErrorInfo) {
    console.error('Error caught by boundary:', error, errorInfo);
  }

  handleReset = () => {
    this.setState({ hasError: false, error: null });
  };

  render() {
    if (this.state.hasError) {
      return (
        this.props.fallback || (
          <div className="error-boundary">
            <h2 className="error-boundary-title">Something went wrong</h2>
            <pre className="error-boundary-message">
              {this.state.error?.message}
            </pre>
            <button
              className="primary-button error-boundary-button"
              onClick={this.handleReset}
            >
              Try Again
            </button>
          </div>
        )
      );
    }

    return this.props.children;
  }
}
