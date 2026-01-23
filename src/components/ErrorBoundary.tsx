// Dad Cam - Phase 3 Error Boundary Component
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
          <div style={{
            display: 'flex',
            flexDirection: 'column',
            alignItems: 'center',
            justifyContent: 'center',
            height: '100vh',
            backgroundColor: '#0f0f0f',
            color: 'white',
            padding: '20px',
          }}>
            <h2 style={{ color: '#ff6666', marginBottom: '16px' }}>Something went wrong</h2>
            <pre style={{
              backgroundColor: '#1a1a1a',
              padding: '16px',
              borderRadius: '8px',
              maxWidth: '600px',
              overflow: 'auto',
              color: '#888',
              fontSize: '13px',
            }}>
              {this.state.error?.message}
            </pre>
            <button
              onClick={this.handleReset}
              style={{
                marginTop: '24px',
                padding: '10px 20px',
                backgroundColor: '#4a9eff',
                border: 'none',
                borderRadius: '4px',
                color: 'white',
                cursor: 'pointer',
                fontSize: '14px',
              }}
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
