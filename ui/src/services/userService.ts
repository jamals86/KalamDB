import { executeSql } from "@/lib/kalam-client";
import { fetchSystemUsers } from "@/services/systemTableService";
import {
  buildCreateUserSql,
  buildDeleteUserSql,
  buildUpdateUserEmailSql,
  buildUpdateUserPasswordSql,
  buildUpdateUserRoleSql,
  buildUpdateUserStorageSql,
  type CreateUserInput,
  type UpdateUserInput,
} from "@/services/sql/queries/userQueries";

export interface User {
  user_id: string;
  username: string;
  role: string;
  email: string | null;
  auth_type: string;
  auth_data: string | null;
  storage_mode: string | null;
  storage_id: string | null;
  failed_login_attempts: string | null;
  locked_until: string | null;
  last_login_at: string | null;
  last_seen: string | null;
  created_at: string;
  updated_at: string;
  deleted_at: string | null;
}

export type { CreateUserInput, UpdateUserInput };

function toNullableString(value: unknown): string | null {
  if (value === null || value === undefined) {
    return null;
  }
  return String(value);
}

export function mapUsers(rows: Record<string, unknown>[]): User[] {
  return rows.map((row) => ({
    user_id: String(row.user_id ?? ""),
    username: String(row.username ?? ""),
    role: String(row.role ?? ""),
    email: row.email as string | null,
    auth_type: String(row.auth_type ?? "internal"),
    auth_data: row.auth_data as string | null,
    storage_mode: row.storage_mode as string | null,
    storage_id: row.storage_id as string | null,
    failed_login_attempts: toNullableString(row.failed_login_attempts),
    locked_until: toNullableString(row.locked_until),
    last_login_at: toNullableString(row.last_login_at),
    last_seen: toNullableString(row.last_seen),
    created_at: String(row.created_at ?? ""),
    updated_at: String(row.updated_at ?? ""),
    deleted_at: row.deleted_at as string | null,
  }));
}

export async function fetchUsers(): Promise<User[]> {
  const rows = await fetchSystemUsers();
  return mapUsers(rows);
}

export async function createUser(input: CreateUserInput): Promise<void> {
  await executeSql(buildCreateUserSql(input));
  const storageSql = buildUpdateUserStorageSql(input.username, input.storage_mode, input.storage_id);
  if (storageSql) {
    await executeSql(storageSql);
  }
}

export async function updateUser(username: string, input: UpdateUserInput): Promise<void> {
  if (input.role) {
    await executeSql(buildUpdateUserRoleSql(username, input.role));
  }
  if (input.password) {
    await executeSql(buildUpdateUserPasswordSql(username, input.password));
  }
  if (input.email !== undefined) {
    await executeSql(buildUpdateUserEmailSql(username, input.email));
  }
  const storageSql = buildUpdateUserStorageSql(username, input.storage_mode, input.storage_id);
  if (storageSql) {
    await executeSql(storageSql);
  }
}

export async function deleteUser(username: string): Promise<void> {
  await executeSql(buildDeleteUserSql(username));
}
