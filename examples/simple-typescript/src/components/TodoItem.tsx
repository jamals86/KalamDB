/**
 * TodoItem Component
 * Displays a single TODO item with delete button
 */

import React from 'react';
import type { Todo } from '../types/todo';

interface TodoItemProps {
  todo: Todo;
  onDelete: (id: number) => void;
}

export function TodoItem({ todo, onDelete }: TodoItemProps) {
  return (
    <li className="todo-item">
      <div className="todo-content">
        <span className="todo-title">{todo.title}</span>
        <span className="todo-date">
          {new Date(todo.created_at).toLocaleString()}
        </span>
      </div>
      <button
        className="delete-button"
        onClick={() => onDelete(todo.id)}
        aria-label={`Delete ${todo.title}`}
      >
        🗑️
      </button>
    </li>
  );
}
