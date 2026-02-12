export const SYSTEM_SETTINGS_QUERY = `
  SELECT name, value, description, category
  FROM system.settings
  ORDER BY category, name
`;

export const SYSTEM_USERS_QUERY = `
  SELECT user_id, username, role, email, auth_type, auth_data, storage_mode, storage_id,
         failed_login_attempts, locked_until, last_login_at, last_seen,
         created_at, updated_at, deleted_at
  FROM system.users
  WHERE deleted_at IS NULL
  ORDER BY username
`;

export const SYSTEM_STATS_QUERY = `
  SELECT metric_name, metric_value
  FROM system.stats
`;
