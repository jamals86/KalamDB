'use client';

/**
 * User menu component – shows the logged-in Keycloak user and a logout button.
 * When Keycloak auth is not active this renders nothing.
 */

import { useKeycloak } from '@/providers/keycloak-provider';
import { KEYCLOAK_CONFIG } from '@/lib/config';
import { LogOut, User } from 'lucide-react';
import { Button } from '@/components/ui/button';

export function UserMenu() {
  // Only render when Keycloak is enabled
  if (!KEYCLOAK_CONFIG.enabled) return null;

  return <UserMenuInner />;
}

function UserMenuInner() {
  const { authenticated, user, logout } = useKeycloak();

  if (!authenticated || !user) return null;

  const displayName =
    user.firstName && user.lastName
      ? `${user.firstName} ${user.lastName}`
      : user.username;

  return (
    <div className="flex items-center gap-2 w-full">
      <div className="flex items-center gap-2 flex-1 min-w-0">
        <div className="h-7 w-7 rounded-full bg-primary/10 flex items-center justify-center shrink-0">
          <User className="h-3.5 w-3.5 text-primary" />
        </div>
        <span className="text-xs font-medium truncate">{displayName}</span>
      </div>
      <Button
        variant="ghost"
        size="icon"
        className="h-7 w-7 shrink-0"
        onClick={logout}
        title="Sign out"
      >
        <LogOut className="h-3.5 w-3.5" />
      </Button>
    </div>
  );
}
