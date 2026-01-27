import { useEffect, type ReactNode } from "react";
import { useNavigate, useLocation } from "react-router-dom";
import { Loader2, Database } from "lucide-react";
import { useAppDispatch, useAppSelector } from "@/store/hooks";
import { checkSetupStatus } from "@/store/setupSlice";

interface SetupGuardProps {
  children: ReactNode;
}

export default function SetupGuard({ children }: SetupGuardProps) {
  const dispatch = useAppDispatch();
  const navigate = useNavigate();
  const location = useLocation();
  const { needsSetup, isCheckingStatus } = useAppSelector((state) => state.setup);

  // Check setup status on mount
  useEffect(() => {
    dispatch(checkSetupStatus());
  }, [dispatch]);

  // Redirect based on setup status
  useEffect(() => {
    if (isCheckingStatus) return;

    const isOnSetupPage = location.pathname === "/setup";

    if (needsSetup === true && !isOnSetupPage) {
      // Server needs setup, redirect to setup page
      navigate("/setup", { replace: true });
    } else if (needsSetup === false && isOnSetupPage) {
      // Server is already set up, redirect to login
      navigate("/login", { replace: true });
    }
  }, [needsSetup, isCheckingStatus, location.pathname, navigate]);

  // Show loading while checking status
  if (isCheckingStatus) {
    return (
      <div className="flex min-h-screen flex-col items-center justify-center bg-background">
        <div className="flex items-center gap-3 mb-4">
          <Database className="h-10 w-10 text-primary" />
          <span className="text-2xl font-bold">KalamDB</span>
        </div>
        <div className="flex items-center gap-2 text-muted-foreground">
          <Loader2 className="h-5 w-5 animate-spin" />
          <span>Checking server status...</span>
        </div>
      </div>
    );
  }

  return <>{children}</>;
}
