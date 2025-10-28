/**
 * TodoItem Component
 * Feature: 006-docker-wasm-examples
 * 
 * Individual TODO item with delete action
 */

import type { Todo } from '../types/todo';

interface TodoItemProps {
  todo: Todo;
  onDelete: (id: number) => Promise<void>;
  onToggle: (id: number) => Promise<void>;
  disabled: boolean;
}

export function TodoItem({ todo, onDelete, onToggle, disabled }: TodoItemProps) {
  const handleDelete = async () => {
    if (disabled) return;
    
    if (confirm(`Delete "${todo.title}"?`)) {
      try {
        await onDelete(todo.id);
      } catch (error) {
        alert(`Failed to delete: ${error instanceof Error ? error.message : 'Unknown error'}`);
      }
    }
  };

  const handleToggle = async () => {
    if (disabled) return;
    
    try {
      await onToggle(todo.id);
    } catch (error) {
      alert(`Failed to toggle: ${error instanceof Error ? error.message : 'Unknown error'}`);
    }
  };

  return (
    <div className={`todo-item ${todo.completed ? 'completed' : ''}`}>
      <div className="todo-content">
        <input
          type="checkbox"
          checked={todo.completed}
          onChange={handleToggle}
          disabled={disabled}
          className="todo-checkbox"
        />
        <span className="todo-title">{todo.title}</span>
      </div>
      
      <div className="todo-meta">
        <span className="todo-date">
          {new Date(todo.created_at).toLocaleDateString()}
        </span>
        <button
          onClick={handleDelete}
          disabled={disabled}
          className="delete-button"
          title="Delete TODO"
        >
          ✕
        </button>
      </div>
    </div>
  );
}
