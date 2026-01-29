import { BrowserRouter, Routes, Route, Navigate } from "react-router-dom";
import { AuthProvider } from "./lib/auth";
import Login from "./pages/Login";
import SetupWizard from "./pages/SetupWizard";
import Dashboard from "./pages/Dashboard";
import SqlStudio from "./pages/SqlStudio";
import Users from "./pages/Users";
import Namespaces from "./pages/Namespaces";
import LiveQueries from "./pages/LiveQueries";
import Logging from "./pages/Logging";
import Settings from "./pages/Settings";
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
              <Route path="namespaces" element={<Namespaces />} />
              <Route path="live-queries" element={<LiveQueries />} />
              <Route path="logging" element={<Logging />} />
              <Route path="logging/:tab" element={<Logging />} />
              <Route path="settings" element={<Settings />} />
              <Route path="settings/:category" element={<Settings />} />
            </Route>
          </Routes>
        </SetupGuard>
      </AuthProvider>
    </BrowserRouter>
  );
}

export default App;
