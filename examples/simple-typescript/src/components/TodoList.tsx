/**
 * TodoList Component
 * Feature: 006-docker-wasm-examples
 * 
 * Displays list of TODOs with empty state
 */

import type { Todo } from '../types/todo';
import { TodoItem } from './TodoItem';

interface TodoListProps {
  todos: Todo[];
  onDelete: (id: number) => Promise<void>;
  onToggle: (id: number) => Promise<void>;
  disabled: boolean;
}

export function TodoList({ todos, onDelete, onToggle, disabled }: TodoListProps) {
  if (todos.length === 0) {
    return (
      <div className="empty-state">
        <div className="empty-icon">📝</div>
        <h3>No TODOs yet</h3>
        <p>Add your first TODO to get started!</p>
      </div>
    );
  }

  // Separate active and completed todos
  const activeTodos = todos.filter(t => !t.completed);
  const completedTodos = todos.filter(t => t.completed);

  return (
    <div className="todo-list">
      {activeTodos.length > 0 && (
        <div className="todo-section">
          <h3 className="section-title">
            Active ({activeTodos.length})
          </h3>
          {activeTodos.map(todo => (
            <TodoItem
              key={todo.id}
              todo={todo}
              onDelete={onDelete}
              onToggle={onToggle}
              disabled={disabled}
            />
          ))}
        </div>
      )}

      {completedTodos.length > 0 && (
        <div className="todo-section">
          <h3 className="section-title">
            Completed ({completedTodos.length})
          </h3>
          {completedTodos.map(todo => (
            <TodoItem
              key={todo.id}
              todo={todo}
              onDelete={onDelete}
              onToggle={onToggle}
              disabled={disabled}
            />
          ))}
        </div>
      )}
    </div>
  );
}
