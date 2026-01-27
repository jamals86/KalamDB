import { createApi } from "@reduxjs/toolkit/query/react";
import { executeSql } from "../lib/kalam-client";

// Define a common base for SQL-based queries
// Since Kalam-link SDK uses internal fetch, we might want to wrap it or use custom baseQuery
const sqlBaseQuery = async ({ sql }: { sql: string }) => {
  try {
    const result = await executeSql(sql);
    return { data: result };
  } catch (error: any) {
    return { error: { status: 'CUSTOM_ERROR', error: error.message || 'SQL Execution failed' } };
  }
};

export const apiSlice = createApi({
  reducerPath: "api",
  baseQuery: sqlBaseQuery,
  tagTypes: ["Settings", "Users", "Jobs", "Namespaces", "Storages"],
  endpoints: (builder) => ({
    getSettings: builder.query<any[], void>({
      query: () => ({
        sql: `
          SELECT name, value, description, category
          FROM system.settings
          ORDER BY category, name
        `,
      }),
      providesTags: ["Settings"],
    }),
    getUsers: builder.query<any[], void>({
      query: () => ({
        sql: `
          SELECT user_id, username, role, email, auth_type, storage_mode, storage_id, created_at, updated_at, deleted_at
          FROM system.users
          WHERE deleted_at IS NULL
          ORDER BY username
        `,
      }),
      providesTags: ["Users"],
    }),
    getJobs: builder.query<any[], void>({
      query: () => ({
        sql: `
          SELECT job_id, job_type, status, parameters, created_at, started_at, completed_at, error, node_id, progress
          FROM system.jobs
          ORDER BY created_at DESC
          LIMIT 100
        `,
      }),
      providesTags: ["Jobs"],
    }),
  }),
});

export const { useGetSettingsQuery, useGetUsersQuery, useGetJobsQuery } = apiSlice;
