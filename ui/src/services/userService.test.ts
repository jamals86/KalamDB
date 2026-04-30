import { describe, expect, it } from "vitest";
import type { User } from "@/services/userService";

describe("User type", () => {
  it("uses the schema-backed user_id field directly", () => {
    const user: User = {
      user_id: "root",
      role: "system",
      email: "root@localhost",
      auth_type: "password",
      auth_data: null,
      storage_mode: "table",
      storage_id: null,
      failed_login_attempts: 0,
      locked_until: null,
      last_login_at: null,
      last_seen: null,
      created_at: "2026-01-01T00:00:00Z",
      updated_at: "2026-01-01T00:00:00Z",
      deleted_at: null,
    };

    expect(user.user_id).toBe("root");
  });
});
