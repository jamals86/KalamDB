import { executeSqlStudioQuery } from "@/services/sqlStudioService";
import type { ToastInput } from "@/components/ui/toaster-provider";

interface RunSqlOptions {
  successTitle: string;
  errorTitle: string;
  notify: (input: ToastInput) => void;
}

export async function runSql(sql: string, opts: RunSqlOptions): Promise<boolean> {
  let result: Awaited<ReturnType<typeof executeSqlStudioQuery>>;
  try {
    result = await executeSqlStudioQuery(sql);
  } catch (err) {
    opts.notify({
      title: opts.errorTitle,
      description: err instanceof Error ? err.message : String(err),
      variant: "destructive",
    });
    return false;
  }
  if (result.status === "error") {
    opts.notify({
      title: opts.errorTitle,
      description: result.errorMessage ?? "Unknown error",
      variant: "destructive",
    });
    return false;
  }
  opts.notify({ title: opts.successTitle, variant: "success" });
  return true;
}
