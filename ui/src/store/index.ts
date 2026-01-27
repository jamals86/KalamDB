import { configureStore } from "@reduxjs/toolkit";
import authReducer from "./authSlice";
import setupReducer from "./setupSlice";
import { apiSlice } from "./apiSlice";

export const store = configureStore({
  reducer: {
    auth: authReducer,
    setup: setupReducer,
    [apiSlice.reducerPath]: apiSlice.reducer,
  },
  middleware: (getDefaultMiddleware) =>
    getDefaultMiddleware().concat(apiSlice.middleware),
});

export type RootState = ReturnType<typeof store.getState>;
export type AppDispatch = typeof store.dispatch;
