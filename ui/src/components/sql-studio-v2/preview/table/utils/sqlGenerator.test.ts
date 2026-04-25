import { describe, expect, it } from "vitest";
import type { RowDeletion, RowEdit } from "@/hooks/useTableChanges";
import { generateSqlStatements } from "./sqlGenerator";

describe("generateSqlStatements", () => {
  it("wraps multiple tracked mutations in an explicit transaction", () => {
    const edits = new Map<number, RowEdit>([
      [
        0,
        {
          rowIndex: 0,
          primaryKeyValues: { id: 1 },
          cellEdits: {
            name: {
              columnName: "name",
              oldValue: "Ada",
              newValue: "Grace",
            },
            email: {
              columnName: "email",
              oldValue: "ada@example.com",
              newValue: "grace@example.com",
            },
          },
        },
      ],
    ]);

    const generated = generateSqlStatements("public", "users", edits, new Map());

    expect(generated.isTransactional).toBe(true);
    expect(generated.mutationCount).toBe(2);
    expect(generated.statements).toEqual([
      "BEGIN;",
      "UPDATE public.users SET name = 'Grace', email = 'grace@example.com' WHERE id = 1;",
      "COMMIT;",
    ]);
    expect(generated.fullSql).toContain("BEGIN;");
    expect(generated.fullSql).toContain("COMMIT;");
  });

  it("keeps a single mutation as a direct statement", () => {
    const deletions = new Map<number, RowDeletion>([
      [
        2,
        {
          rowIndex: 2,
          primaryKeyValues: { id: 9 },
        },
      ],
    ]);

    const generated = generateSqlStatements("public", "users", new Map(), deletions);

    expect(generated.isTransactional).toBe(false);
    expect(generated.mutationCount).toBe(1);
    expect(generated.statements).toEqual([
      "DELETE FROM public.users WHERE id = 9;",
    ]);
  });
});