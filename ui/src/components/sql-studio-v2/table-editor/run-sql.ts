import { executeSqlStudioQuery } from "@/services/sqlStudioService";

export async function executeSqlPreviewStatement(stmt: string): Promise<void> {
  const result = await executeSqlStudioQuery(stmt);
  if (result.status === "error") {
    throw new Error(result.errorMessage ?? "Execution failed");
  }
}
