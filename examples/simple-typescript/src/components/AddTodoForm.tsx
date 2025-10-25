/**
 * AddTodoForm Component
 * Form for adding new TODOs (disabled when disconnected)
 */

import React, { useState, FormEvent } from 'react';

interface AddTodoFormProps {
  onAdd: (title: string) => Promise<void>;
  isConnected: boolean;
}

export function AddTodoForm({ onAdd, isConnected }: AddTodoFormProps) {
  const [title, setTitle] = useState('');
  const [isSubmitting, setIsSubmitting] = useState(false);

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault();
    
    if (!title.trim() || !isConnected) {
      return;
    }

    setIsSubmitting(true);
    try {
      await onAdd(title.trim());
      setTitle(''); // Clear input on success
    } catch (err) {
      console.error('Failed to add TODO:', err);
      // Keep the title so user can retry
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <form className="add-todo-form" onSubmit={handleSubmit}>
      <input
        type="text"
        value={title}
        onChange={(e) => setTitle(e.target.value)}
        placeholder="What needs to be done?"
        className="todo-input"
        disabled={!isConnected || isSubmitting}
        maxLength={200}
      />
      <button
        type="submit"
        className="add-button"
        disabled={!isConnected || isSubmitting || !title.trim()}
      >
        {isSubmitting ? '➕ Adding...' : '➕ Add TODO'}
      </button>
    </form>
  );
}
