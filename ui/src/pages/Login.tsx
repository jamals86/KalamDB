import { useNavigate, useLocation } from "react-router-dom";
import { useAuth } from "@/lib/auth";
import { useEffect } from "react";
import LoginForm from "@/components/auth/LoginForm";
import { Database } from "lucide-react";

export default function Login() {
  const { isAuthenticated } = useAuth();
  const navigate = useNavigate();
  const location = useLocation();

  const from = (location.state as { from?: { pathname: string } })?.from?.pathname || "/dashboard";

  useEffect(() => {
    if (isAuthenticated) {
      navigate(from, { replace: true });
    }
  }, [isAuthenticated, navigate, from]);

  const handleLoginSuccess = () => {
    navigate(from, { replace: true });
  };

  return (
    <div className="flex min-h-screen flex-col items-center justify-center bg-background p-4">
      <div className="mb-8 flex items-center gap-2">
        <Database className="h-10 w-10 text-primary" />
        <span className="text-3xl font-bold">KalamDB</span>
      </div>
      <LoginForm onSuccess={handleLoginSuccess} />
      <p className="mt-8 text-center text-sm text-muted-foreground">
        Sign in with a user that has <strong>dba</strong> or <strong>system</strong> role
      </p>
    </div>
  );
}
