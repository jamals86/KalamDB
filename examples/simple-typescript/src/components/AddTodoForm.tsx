/**
 * AddTodoForm Component
 * Feature: 006-docker-wasm-examples
 * 
 * Form for adding new TODOs (disabled when disconnected)
 */

import { useState, FormEvent } from 'react';

interface AddTodoFormProps {
  onAdd: (title: string) => Promise<void>;
  disabled: boolean;
}

export function AddTodoForm({ onAdd, disabled }: AddTodoFormProps) {
  const [title, setTitle] = useState('');
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault();
    
    if (!title.trim()) {
      setError('Title cannot be empty');
      return;
    }

    if (title.length > 500) {
      setError('Title must be 500 characters or less');
      return;
    }

    setIsSubmitting(true);
    setError(null);

    try {
      await onAdd(title);
      setTitle(''); // Clear input on success
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to add TODO');
    } finally {
      setIsSubmitting(false);
    }
  };

  const isDisabled = disabled || isSubmitting;

  return (
    <form onSubmit={handleSubmit} className="add-todo-form">
      <div className="form-group">
        <input
          type="text"
          value={title}
          onChange={(e) => {
            setTitle(e.target.value);
            setError(null); // Clear error on input
          }}
          placeholder="What needs to be done?"
          disabled={isDisabled}
          maxLength={500}
          className="todo-input"
          autoFocus
        />
        <button
          type="submit"
          disabled={isDisabled}
          className="add-button"
        >
          {isSubmitting ? '...' : 'Add'}
        </button>
      </div>
      
      {error && (
        <div className="error-message">
          {error}
        </div>
      )}
      
      {disabled && (
        <div className="warning-message">
          ⚠️ Disconnected - reconnect to add TODOs
        </div>
      )}

      <div className="character-count">
        {title.length}/500
      </div>
    </form>
  );
}
