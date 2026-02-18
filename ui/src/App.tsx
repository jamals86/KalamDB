import { BrowserRouter, Routes, Route, Navigate } from "react-router-dom";
import { AuthProvider } from "./lib/auth";
import { SqlPreviewProvider } from "./components/sql-preview";
import Login from "./pages/Login";
import SetupWizard from "./pages/SetupWizard";
import Dashboard from "./pages/Dashboard";
import SqlStudio from "./pages/SqlStudio";
import Users from "./pages/Users";
import Jobs from "./pages/Jobs";
import LiveQueries from "./pages/LiveQueries";
import Logging from "./pages/Logging";
import Settings from "./pages/Settings";
import StreamingTopics from "./pages/StreamingTopics";
import StreamingTopicDetail from "./pages/StreamingTopicDetail";
import StreamingGroups from "./pages/StreamingGroups";
import StreamingOffsets from "./pages/StreamingOffsets";
import ProtectedRoute from "./components/auth/ProtectedRoute";
import SetupGuard from "./components/auth/SetupGuard";
import Layout from "./components/layout/Layout";

function App() {
  return (
    <BrowserRouter basename="/ui">
      <AuthProvider>
        <SqlPreviewProvider>
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
              <Route path="jobs" element={<Jobs />} />
              <Route path="live-queries" element={<LiveQueries />} />
              <Route path="streaming" element={<Navigate to="/streaming/topics" replace />} />
              <Route path="streaming/topics" element={<StreamingTopics />} />
              <Route path="streaming/topics/:topicId" element={<StreamingTopicDetail />} />
              <Route path="streaming/groups" element={<StreamingGroups />} />
              <Route path="streaming/offsets" element={<StreamingOffsets />} />
              <Route path="logging" element={<Logging />} />
              <Route path="logging/:tab" element={<Logging />} />
              <Route path="settings" element={<Settings />} />
              <Route path="settings/:category" element={<Settings />} />
            </Route>
          </Routes>
        </SetupGuard>
        </SqlPreviewProvider>
      </AuthProvider>
    </BrowserRouter>
  );
}

export default App;
