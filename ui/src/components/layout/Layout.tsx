import { Outlet, useLocation } from "react-router-dom";
import Sidebar from "./Sidebar";
import Header from "./Header";

export default function Layout() {
  const location = useLocation();
  const isSqlWorkspace = location.pathname.startsWith("/sql");

  return (
    <div className="flex h-screen flex-col bg-background">
      <Header />
      <div className="flex min-h-0 flex-1 overflow-hidden">
        <Sidebar />
        <main className={isSqlWorkspace ? "min-h-0 flex-1 overflow-hidden p-0" : "flex-1 overflow-auto p-0"}>
          <Outlet />
        </main>
      </div>
    </div>
  );
}
