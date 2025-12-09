import { BrowserRouter, Routes, Route, Navigate } from "react-router-dom";
import { AuthProvider } from "./lib/auth";
import Login from "./pages/Login";
import Dashboard from "./pages/Dashboard";
import SqlStudio from "./pages/SqlStudio";
import Users from "./pages/Users";
import Storages from "./pages/Storages";
import Namespaces from "./pages/Namespaces";
import AuditLogs from "./pages/AuditLogs";
import ServerLogs from "./pages/ServerLogs";
import Settings from "./pages/Settings";
import ProtectedRoute from "./components/auth/ProtectedRoute";
import Layout from "./components/layout/Layout";

function App() {
  return (
    <BrowserRouter basename="/ui">
      <AuthProvider>
        <Routes>
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
            <Route path="audit-logs" element={<AuditLogs />} />
            <Route path="server-logs" element={<ServerLogs />} />
            <Route path="settings" element={<Settings />} />
          </Route>
        </Routes>
      </AuthProvider>
    </BrowserRouter>
  );
}

export default App;
