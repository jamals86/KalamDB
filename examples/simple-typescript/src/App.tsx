/**
 * App Component
 * Main application component that composes all components and manages TODO state
 */

import React from 'react';
import { useTodos } from './hooks/useTodos';
import { ConnectionStatus } from './components/ConnectionStatus';
import { AddTodoForm } from './components/AddTodoForm';
import { TodoList } from './components/TodoList';
import './styles/App.css';

function App() {
  const { todos, isConnected, isLoading, error, addTodo, deleteTodo } = useTodos();

  const handleAddTodo = async (title: string) => {
    await addTodo({ title });
  };

  if (isLoading) {
    return (
      <div className="app">
        <div className="container">
          <h1>KalamDB TODO App</h1>
          <p>Loading...</p>
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="app">
        <div className="container">
          <h1>KalamDB TODO App</h1>
          <div className="error">
            <p>❌ Error: {error}</p>
            <p>Make sure KalamDB server is running and .env is configured correctly.</p>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="app">
      <div className="container">
        <header className="app-header">
          <h1>📝 KalamDB TODO App</h1>
          <ConnectionStatus isConnected={isConnected} />
        </header>

        <main>
          <AddTodoForm onAdd={handleAddTodo} isConnected={isConnected} />
          <TodoList todos={todos} onDelete={deleteTodo} />
        </main>

        <footer className="app-footer">
          <p>
            Real-time sync with <strong>KalamDB</strong> + localStorage caching
          </p>
          <p className="hint">
            💡 Open in multiple tabs to see real-time synchronization
          </p>
        </footer>
      </div>
    </div>
  );
}

export default App;
