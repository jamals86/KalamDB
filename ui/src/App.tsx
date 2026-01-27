import { BrowserRouter, Routes, Route, Navigate } from "react-router-dom";
import { AuthProvider } from "./lib/auth";
import Login from "./pages/Login";
import SetupWizard from "./pages/SetupWizard";
import Dashboard from "./pages/Dashboard";
import SqlStudio from "./pages/SqlStudio";
import Users from "./pages/Users";
import Storages from "./pages/Storages";
import Namespaces from "./pages/Namespaces";
import Jobs from "./pages/Jobs";
import LiveQueries from "./pages/LiveQueries";
import AuditLogs from "./pages/AuditLogs";
import ServerLogs from "./pages/ServerLogs";
import Settings from "./pages/Settings";
import Cluster from "./pages/Cluster";
import ProtectedRoute from "./components/auth/ProtectedRoute";
import SetupGuard from "./components/auth/SetupGuard";
import Layout from "./components/layout/Layout";

function App() {
  return (
    <BrowserRouter basename="/ui">
      <AuthProvider>
        <SetupGuard>
          <Routes>
            <Route path="/setup" element={<SetupWizard />} />
            <Route path="/login" element={<Login />} />
            <Route
              path="/"
              element={
                <ProtectedRoute>
                  <Layout />
                </ProtectedRoute>
              }
            >
              <Route index element={<Navigate to="/dashboard" replace />} />
              <Route path="dashboard" element={<Dashboard />} />
              <Route path="sql" element={<SqlStudio />} />
              <Route path="users" element={<Users />} />
              <Route path="storages" element={<Storages />} />
              <Route path="namespaces" element={<Namespaces />} />
              <Route path="jobs" element={<Jobs />} />
              <Route path="live-queries" element={<LiveQueries />} />
              <Route path="cluster" element={<Cluster />} />
              <Route path="audit-logs" element={<AuditLogs />} />
              <Route path="server-logs" element={<ServerLogs />} />
              <Route path="settings" element={<Settings />} />
            </Route>
          </Routes>
        </SetupGuard>
      </AuthProvider>
    </BrowserRouter>
  );
}

export default App;
