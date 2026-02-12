import { configureStore } from "@reduxjs/toolkit";
import authReducer from "./authSlice";
import setupReducer from "./setupSlice";
import { apiSlice } from "./apiSlice";
import sqlStudioUiReducer from "@/features/sql-studio/state/sqlStudioUiSlice";
import sqlStudioWorkspaceReducer from "@/features/sql-studio/state/sqlStudioWorkspaceSlice";

export const store = configureStore({
  reducer: {
    auth: authReducer,
    setup: setupReducer,
    sqlStudioUi: sqlStudioUiReducer,
    sqlStudioWorkspace: sqlStudioWorkspaceReducer,
    [apiSlice.reducerPath]: apiSlice.reducer,
  },
  middleware: (getDefaultMiddleware) =>
    getDefaultMiddleware().concat(apiSlice.middleware),
});

export type RootState = ReturnType<typeof store.getState>;
export type AppDispatch = typeof store.dispatch;
