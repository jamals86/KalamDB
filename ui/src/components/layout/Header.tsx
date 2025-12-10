import { useAuth } from "@/lib/auth";
import { Button } from "@/components/ui/button";
import { LogOut, User } from "lucide-react";

export default function Header() {
  const { user, logout } = useAuth();

  const handleLogout = async () => {
    await logout();
  };

  return (
    <header className="flex h-16 items-center justify-between border-b bg-card px-6">
      <div>
        {/* Breadcrumbs or page title can go here */}
      </div>
      <div className="flex items-center gap-4">
        <div className="flex items-center gap-2 text-sm">
          <User className="h-4 w-4" />
          <span>{user?.username}</span>
          <span className="rounded bg-primary/10 px-2 py-0.5 text-xs font-medium text-primary">
            {user?.role}
          </span>
        </div>
        <Button variant="ghost" size="sm" onClick={handleLogout}>
          <LogOut className="mr-2 h-4 w-4" />
          Logout
        </Button>
      </div>
    </header>
  );
}
