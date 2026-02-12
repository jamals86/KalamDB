import { useMemo, useState } from 'react';
import { useAuth } from '@/lib/auth';
import { useDeleteUserMutation, useGetUsersListQuery } from '@/store/apiSlice';
import type { User } from '@/services/userService';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { TimestampDisplay } from '@/components/datatype-display/TimestampDisplay';
import { UserForm } from './UserForm';
import { DeleteUserDialog } from './DeleteUserDialog';
import { Search, Plus, Pencil, Trash2, Loader2, RefreshCw } from 'lucide-react';

export function UsersList() {
  const { user: currentUser } = useAuth();
  const {
    data: users = [],
    isFetching: isLoading,
    error,
    refetch,
  } = useGetUsersListQuery();
  const [deleteUserMutation] = useDeleteUserMutation();
  const [searchQuery, setSearchQuery] = useState('');
  const [isCreateOpen, setIsCreateOpen] = useState(false);
  const [editingUser, setEditingUser] = useState<User | null>(null);
  const [deletingUser, setDeletingUser] = useState<User | null>(null);

  const filteredUsers = useMemo(
    () =>
      users.filter((user) => {
        if (!searchQuery.trim()) return true;
        const query = searchQuery.toLowerCase();
        return (
          user.username.toLowerCase().includes(query) ||
          user.email?.toLowerCase().includes(query) ||
          user.role.toLowerCase().includes(query)
        );
      }),
    [searchQuery, users],
  );

  const getRoleBadgeColor = (role: string) => {
    switch (role) {
      case 'system':
        return 'bg-purple-100 text-purple-800';
      case 'dba':
        return 'bg-blue-100 text-blue-800';
      case 'service':
        return 'bg-green-100 text-green-800';
      default:
        return 'bg-gray-100 text-gray-800';
    }
  };

  if (error) {
    return (
      <div className="p-4 bg-red-50 border border-red-200 rounded-lg">
        <p className="text-red-700">{"error" in error ? error.error : "Failed to fetch users"}</p>
        <Button variant="outline" onClick={() => void refetch()} className="mt-2">
          Retry
        </Button>
      </div>
    );
  }

  return (
    <div className="space-y-4">
      {/* Toolbar */}
      <div className="flex items-center justify-between gap-4">
        <div className="relative flex-1 max-w-sm">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
          <Input
            placeholder="Search users..."
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            className="pl-9"
          />
        </div>
        <div className="flex items-center gap-2">
          <Button variant="outline" size="icon" onClick={() => void refetch()} disabled={isLoading}>
            <RefreshCw className={`h-4 w-4 ${isLoading ? 'animate-spin' : ''}`} />
          </Button>
          <Button onClick={() => setIsCreateOpen(true)}>
            <Plus className="h-4 w-4 mr-2" />
            Create User
          </Button>
        </div>
      </div>

      {/* Table */}
      {isLoading && users.length === 0 ? (
        <div className="flex items-center justify-center py-8">
          <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
        </div>
      ) : (
        <div className="border rounded-lg">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Username</TableHead>
                <TableHead>Role</TableHead>
                <TableHead>Email</TableHead>
                <TableHead>Created</TableHead>
                <TableHead className="w-[100px]">Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {filteredUsers.length === 0 ? (
                <TableRow>
                  <TableCell colSpan={5} className="text-center py-8 text-muted-foreground">
                    {searchQuery ? 'No users match your search' : 'No users found'}
                  </TableCell>
                </TableRow>
              ) : (
                filteredUsers.map((user, index) => (
                  <TableRow key={user.user_id || `user-${index}`}>
                    <TableCell className="font-medium">{user.username}</TableCell>
                    <TableCell>
                      <span className={`px-2 py-1 rounded-full text-xs font-medium ${getRoleBadgeColor(user.role)}`}>
                        {user.role}
                      </span>
                    </TableCell>
                    <TableCell className="text-muted-foreground">
                      {user.email || '—'}
                    </TableCell>
                    <TableCell className="text-muted-foreground">
                      {user.created_at ? (
                        <TimestampDisplay value={user.created_at} />
                      ) : (
                        '—'
                      )}
                    </TableCell>
                    <TableCell>
                      <div className="flex items-center gap-1">
                        <Button
                          variant="ghost"
                          size="icon"
                          onClick={() => setEditingUser(user)}
                        >
                          <Pencil className="h-4 w-4" />
                        </Button>
                        <Button
                          variant="ghost"
                          size="icon"
                          onClick={() => setDeletingUser(user)}
                          disabled={currentUser?.username === user.username}
                        >
                          <Trash2 className="h-4 w-4" />
                        </Button>
                      </div>
                    </TableCell>
                  </TableRow>
                ))
              )}
            </TableBody>
          </Table>
        </div>
      )}

      {/* Create User Dialog */}
      <UserForm
        open={isCreateOpen}
        onOpenChange={setIsCreateOpen}
        onSuccess={() => {
          setIsCreateOpen(false);
          void refetch();
        }}
      />

      {/* Edit User Dialog */}
      {editingUser && (
        <UserForm
          open={true}
          onOpenChange={() => setEditingUser(null)}
          user={editingUser}
          onSuccess={() => {
            setEditingUser(null);
            void refetch();
          }}
        />
      )}

      {/* Delete User Dialog */}
      {deletingUser && (
        <DeleteUserDialog
          open={true}
          onOpenChange={() => setDeletingUser(null)}
          user={deletingUser}
          onConfirm={async () => {
            if (currentUser?.username === deletingUser.username) {
              throw new Error("Cannot delete your own account");
            }
            await deleteUserMutation({ username: deletingUser.username }).unwrap();
            setDeletingUser(null);
            void refetch();
          }}
        />
      )}
    </div>
  );
}
