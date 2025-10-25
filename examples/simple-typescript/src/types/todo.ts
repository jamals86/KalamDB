/**
 * TODO item type definition
 */
export interface Todo {
  id: number;
  title: string;
  completed: boolean;
  created_at: string;
}

/**
 * Type for creating a new TODO (id is auto-generated)
 */
export interface NewTodo {
  title: string;
  completed?: boolean;
}
