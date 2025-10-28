/**
 * App Component
 * Feature: 006-docker-wasm-examples
 * 
 * Main application component composing all features
 */

import { useTodos } from './hooks/useTodos';
import { ConnectionStatus } from './components/ConnectionStatus';
import { AddTodoForm } from './components/AddTodoForm';
import { TodoList } from './components/TodoList';
import './styles/App.css';

export function App() {
  const {
    todos,
    connectionStatus,
    addTodo,
    deleteTodo,
    toggleTodo,
    isLoading,
    error
  } = useTodos();

  const isDisabled = connectionStatus !== 'connected';

  return (
    <div className="app">
      <header className="app-header">
        <div className="header-content">
          <h1 className="app-title">
            <span className="title-icon">✓</span>
            KalamDB TODO
          </h1>
          <ConnectionStatus status={connectionStatus} />
        </div>
        <p className="app-subtitle">
          Real-time TODO app powered by KalamDB WASM client
        </p>
      </header>

      <main className="app-main">
        {error && (
          <div className="error-banner">
            <strong>Error:</strong> {error}
            <p className="error-help">
              Make sure KalamDB server is running and .env is configured correctly.
            </p>
          </div>
        )}

        {isLoading && (
          <div className="loading-state">
            <div className="spinner"></div>
            <p>Loading TODOs...</p>
          </div>
        )}

        {!isLoading && !error && (
          <>
            <AddTodoForm 
              onAdd={addTodo} 
              disabled={isDisabled}
            />
            
            <TodoList
              todos={todos}
              onDelete={deleteTodo}
              onToggle={toggleTodo}
              disabled={isDisabled}
            />
          </>
        )}
      </main>

      <footer className="app-footer">
        <p>
          Built with <a href="https://github.com/yourusername/KalamDB" target="_blank" rel="noopener noreferrer">KalamDB</a>
          {' '} • {' '}
          <span className="todo-count">{todos.length} total TODO{todos.length !== 1 ? 's' : ''}</span>
        </p>
      </footer>
    </div>
  );
}
