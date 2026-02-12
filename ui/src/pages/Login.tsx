import { useNavigate, useLocation } from "react-router-dom";
import { useAuth } from "@/lib/auth";
import { useEffect } from "react";
import LoginForm from "@/components/auth/LoginForm";
import AuthSplitLayout from "@/components/auth/AuthSplitLayout";

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
    <AuthSplitLayout
      title="Log in to KalamDB"
      description="Sign in with an account that has DBA or system privileges."
      panelTitle="Run the control plane"
      panelDescription="Monitor jobs, manage users, and operate SQL Studio from one consistent interface."
      panelFootnote="Secure access"
    >
      <LoginForm onSuccess={handleLoginSuccess} />
    </AuthSplitLayout>
  );
}
