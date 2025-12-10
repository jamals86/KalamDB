import { useState } from 'react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { User, useUsers } from '@/hooks/useUsers';
import { Loader2 } from 'lucide-react';

interface UserFormProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  user?: User;
  onSuccess: () => void;
}

const ROLES = ['user', 'service', 'dba', 'system'];

export function UserForm({ open, onOpenChange, user, onSuccess }: UserFormProps) {
  const { createUser, updateUser } = useUsers();
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  
  const [formData, setFormData] = useState({
    username: user?.username || '',
    password: '',
    role: user?.role || 'user',
    email: user?.email || '',
  });

  const isEditing = !!user;

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setIsSubmitting(true);
    setError(null);

    try {
      if (isEditing) {
        const updateData: { role?: string; password?: string } = {};
        if (formData.role !== user.role) {
          updateData.role = formData.role;
        }
        if (formData.password) {
          updateData.password = formData.password;
        }
        await updateUser(user.user_id, updateData);
      } else {
        if (!formData.username.trim()) {
          throw new Error('Username is required');
        }
        if (!formData.password.trim()) {
          throw new Error('Password is required');
        }
        await createUser({
          username: formData.username,
          password: formData.password,
          role: formData.role,
          email: formData.email || undefined,
        });
      }
      onSuccess();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to save user');
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{isEditing ? 'Edit User' : 'Create User'}</DialogTitle>
          <DialogDescription>
            {isEditing
              ? 'Update user details and permissions'
              : 'Create a new database user'}
          </DialogDescription>
        </DialogHeader>

        <form onSubmit={handleSubmit} className="space-y-4">
          <div className="space-y-2">
            <label className="text-sm font-medium">Username</label>
            <Input
              value={formData.username}
              onChange={(e) => setFormData({ ...formData, username: e.target.value })}
              disabled={isEditing}
              placeholder="Enter username"
            />
          </div>

          <div className="space-y-2">
            <label className="text-sm font-medium">
              Password {isEditing && '(leave blank to keep current)'}
            </label>
            <Input
              type="password"
              value={formData.password}
              onChange={(e) => setFormData({ ...formData, password: e.target.value })}
              placeholder={isEditing ? '••••••••' : 'Enter password'}
            />
          </div>

          <div className="space-y-2">
            <label className="text-sm font-medium">Role</label>
            <select
              value={formData.role}
              onChange={(e) => setFormData({ ...formData, role: e.target.value })}
              className="w-full px-3 py-2 border rounded-md text-sm"
            >
              {ROLES.map((role) => (
                <option key={role} value={role}>
                  {role}
                </option>
              ))}
            </select>
          </div>

          <div className="space-y-2">
            <label className="text-sm font-medium">Email (optional)</label>
            <Input
              type="email"
              value={formData.email}
              onChange={(e) => setFormData({ ...formData, email: e.target.value })}
              placeholder="user@example.com"
              disabled={isEditing}
            />
          </div>

          {error && (
            <div className="p-3 text-sm text-red-700 bg-red-50 border border-red-200 rounded">
              {error}
            </div>
          )}

          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button type="submit" disabled={isSubmitting}>
              {isSubmitting && <Loader2 className="h-4 w-4 mr-2 animate-spin" />}
              {isEditing ? 'Save Changes' : 'Create User'}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
