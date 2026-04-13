import { Search } from "lucide-react";
import { useAuth } from "@/lib/auth";
import { Input } from "@/components/ui/input";
import BackendStatusIndicator from "./BackendStatusIndicator";
import { NotificationsDropdown } from "./NotificationsDropdown";
import { UserMenu } from "./UserMenu";

const logoUrl = `${import.meta.env.BASE_URL}branding/kalamdb_logo.png`;

export default function Header() {
  const { user, logout } = useAuth();

  return (
    <header className="sticky top-0 z-40 flex h-14 shrink-0 items-center gap-4 border-b border-border bg-card px-4 text-card-foreground md:px-6">
      <div className="flex shrink-0 items-center gap-2.5">
        <img
          src={logoUrl}
          alt="KalamDB"
          className="h-7 w-auto shrink-0 object-contain"
        />
        <div className="leading-tight">
          <p className="text-xs font-semibold tracking-[0.16em] text-muted-foreground">ADMIN UI</p>
          <p className="text-[10px] text-muted-foreground">Embedded UI v2</p>
        </div>
      </div>

      <div className="hidden min-w-0 flex-1 items-center md:flex">
        <div className="relative w-full max-w-2xl">
          <Search className="pointer-events-none absolute left-2 top-2.5 h-4 w-4 text-muted-foreground" />
          <Input
            placeholder="Search jobs, tables, namespaces..."
            className="h-9 bg-muted/60 pl-8"
          />
        </div>
      </div>

      <div className="ml-auto flex shrink-0 items-center gap-2">
        <BackendStatusIndicator />
        <NotificationsDropdown />
        <UserMenu
          username={user?.username ?? "User"}
          role={user?.role ?? "user"}
          onLogout={logout}
        />
      </div>
    </header>
  );
}
