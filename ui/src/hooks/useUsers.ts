import { useState, useCallback } from 'react';
import { executeSql } from '../lib/kalam-client';
import { useAuth } from '../lib/auth';

export interface User {
  user_id: string;
  username: string;
  role: string;
  email: string | null;
  auth_type: string;
  storage_mode: string | null;
  storage_id: string | null;
  created_at: string;
  updated_at: string;
  deleted_at: string | null;
}

export interface CreateUserInput {
  username: string;
  password: string;
  role?: string;
  email?: string;
  display_name?: string;
}

export interface UpdateUserInput {
  role?: string;
  email?: string;
  password?: string;
}

export function useUsers() {
  const [users, setUsers] = useState<User[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const { user: currentUser } = useAuth();

  const fetchUsers = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const rows = await executeSql(`
        SELECT user_id, username, role, email, auth_type, storage_mode, storage_id, created_at, updated_at, deleted_at
        FROM system.users
        WHERE deleted_at IS NULL
        ORDER BY username
      `);
      
      const userList = rows.map((row) => ({
        user_id: String(row.user_id ?? ''),
        username: String(row.username ?? ''),
        role: String(row.role ?? ''),
        email: row.email as string | null,
        auth_type: String(row.auth_type ?? 'internal'),
        storage_mode: row.storage_mode as string | null,
        storage_id: row.storage_id as string | null,
        created_at: String(row.created_at ?? ''),
        updated_at: String(row.updated_at ?? ''),
        deleted_at: row.deleted_at as string | null,
      }));
      
      setUsers(userList);
      return userList;
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Failed to fetch users';
      setError(errorMessage);
      throw err;
    } finally {
      setIsLoading(false);
    }
  }, []);

  const createUser = useCallback(async (input: CreateUserInput) => {
    setError(null);
    try {
      // Use CREATE USER SQL command
      let sql = `CREATE USER '${input.username}' WITH PASSWORD '${input.password}'`;
      if (input.role) {
        sql += ` ROLE '${input.role}'`;
      }
      await executeSql(sql);
      await fetchUsers(); // Refresh the list
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Failed to create user';
      setError(errorMessage);
      throw err;
    }
  }, [fetchUsers]);

  const updateUser = useCallback(async (userId: string, input: UpdateUserInput) => {
    setError(null);
    try {
      // Find the user first to get username
      const user = users.find(u => u.user_id === userId);
      if (!user) {
        throw new Error('User not found');
      }

      // Update user via ALTER USER
      if (input.role) {
        await executeSql(`ALTER USER '${user.username}' SET ROLE '${input.role}'`);
      }
      if (input.password) {
        await executeSql(`ALTER USER '${user.username}' SET PASSWORD '${input.password}'`);
      }
      // Note: email and display_name may not be supported via ALTER USER
      // If needed, use direct UPDATE on system.users table
      
      await fetchUsers(); // Refresh the list
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Failed to update user';
      setError(errorMessage);
      throw err;
    }
  }, [users, fetchUsers]);

  const deleteUser = useCallback(async (userId: string) => {
    setError(null);
    
    // Prevent self-deletion - compare with user's username since currentUser uses 'id' not 'user_id'
    const userToDelete = users.find(u => u.user_id === userId);
    if (currentUser && userToDelete && currentUser.username === userToDelete.username) {
      const errorMessage = 'Cannot delete your own account';
      setError(errorMessage);
      throw new Error(errorMessage);
    }
    
    try {
      // Find the user to get username
      const user = users.find(u => u.user_id === userId);
      if (!user) {
        throw new Error('User not found');
      }
      
      await executeSql(`DROP USER '${user.username}'`);
      await fetchUsers(); // Refresh the list
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Failed to delete user';
      setError(errorMessage);
      throw err;
    }
  }, [currentUser, users, fetchUsers]);

  const searchUsers = useCallback((query: string) => {
    if (!query.trim()) {
      return users;
    }
    const lowerQuery = query.toLowerCase();
    return users.filter(user => 
      user.username.toLowerCase().includes(lowerQuery) ||
      user.email?.toLowerCase().includes(lowerQuery) ||
      user.role.toLowerCase().includes(lowerQuery)
    );
  }, [users]);

  // Check if user can be deleted - compare by username since User uses user_id but UserInfo uses id
  const canDeleteUser = useCallback((userId: string) => {
    const user = users.find(u => u.user_id === userId);
    return !user || user.username !== currentUser?.username;
  }, [users, currentUser]);

  return {
    users,
    isLoading,
    error,
    fetchUsers,
    createUser,
    updateUser,
    deleteUser,
    searchUsers,
    canDeleteUser,
  };
}
