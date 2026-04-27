import { useMemo, useState, useEffect } from "react";
import { Plus, AlertCircle, Trash2, Loader2, Pencil } from "lucide-react";
import { useAppDispatch, useAppSelector } from "@/store/hooks";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { useToast } from "@/components/ui/toaster-provider";
import {
  selectEditorMode,
  selectEditorDraft,
  selectEditorOriginal,
  selectEditorSelectedTableKey,
} from "@/features/sql-studio/state/selectors";
import type { StudioNamespace, StudioTable } from "@/components/sql-studio-v2/shared/types";
import { discardEdit, setDraft, startEditTable } from "@/features/sql-studio/state/editorTabSlice";
import { ColumnRow } from "./ColumnRow";
import { ConfirmDialog } from "./ConfirmDialog";
import { isReadOnlyNamespace, newDraftColumn, tableToDraft, type DraftColumn } from "./types";
import {
  generateAlterTableSql,
  generateCreateTableSql,
  generateDropTableSql,
  validateDraft,
} from "./ddl-generator";
import { DestructiveConfirmDialog } from "./DestructiveConfirmDialog";
import { runSql } from "./run-sql";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import equal from "fast-deep-equal";
import { executeSqlStudioQuery } from "@/services/sqlStudioService";

function MetaRow({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="flex items-baseline gap-2 overflow-hidden">
      <span className="shrink-0 text-[10px] uppercase tracking-wide text-muted-foreground">
        {label}
      </span>
      <span className={`truncate ${mono ? "font-mono text-[11px]" : ""}`} title={value}>
        {value}
      </span>
    </div>
  );
}

const YEAR_3000_MS = 3.25e13;

function formatTimestamp(value: string | number | null | undefined): string {
  if (value == null) return "—";
  let ms: number;
  if (typeof value === "string") {
    ms = Date.parse(value);
  } else {
    ms = value > YEAR_3000_MS ? value / 1000 : value;
  }
  if (!Number.isFinite(ms)) return String(value);
  return new Date(ms).toLocaleString();
}

interface EditTableFormProps {
  schema: StudioNamespace[];
  onAfterSave?: () => void | Promise<void>;
  isSchemaRefreshing?: boolean;
}

export function EditTableForm({ schema, onAfterSave, isSchemaRefreshing }: EditTableFormProps) {
  const dispatch = useAppDispatch();
  const mode = useAppSelector(selectEditorMode);
  const draft = useAppSelector(selectEditorDraft);
  const original = useAppSelector(selectEditorOriginal);
  const selectedTableKey = useAppSelector(selectEditorSelectedTableKey);

  const selectedTable: StudioTable | null = useMemo(() => {
    if (!selectedTableKey) return null;
    const [ns, name] = selectedTableKey.split(".");
    if (!ns || !name) return null;
    const namespace = schema.find((n) => n.name === ns);
    return namespace?.tables.find((t) => t.name === name) ?? null;
  }, [schema, selectedTableKey]);

  const [rowCount, setRowCount] = useState<number | null>(null);
  const [rowCountLoading, setRowCountLoading] = useState(false);

  useEffect(() => {
    if (!selectedTable) {
      setRowCount(null);
      return;
    }
    let cancelled = false;
    setRowCount(null);
    setRowCountLoading(true);
    void executeSqlStudioQuery(
      `SELECT COUNT(*) AS c FROM ${selectedTable.namespace}.${selectedTable.name}`,
    )
      .then((result) => {
        if (cancelled) return;
        if (result.status === "error") {
          setRowCount(null);
          return;
        }
        const raw = result.rows?.[0]?.c ?? result.rows?.[0]?.[0];
        const n = typeof raw === "number" ? raw : Number(raw);
        setRowCount(Number.isFinite(n) ? n : null);
      })
      .finally(() => {
        if (!cancelled) setRowCountLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [selectedTable]);

  const validation = useMemo(
    () => (draft ? validateDraft(draft) : null),
    [draft],
  );
  const isDirty = useMemo(() => {
    if (!draft) return false;
    if (mode === "create") return true;
    if (!original) return false;
    return !equal(draft, original);
  }, [draft, original, mode]);
  const [showErrors, setShowErrors] = useState(false);
  const [showDropConfirm, setShowDropConfirm] = useState(false);
  const [showDropColsConfirm, setShowDropColsConfirm] = useState(false);
  const [showDiscardConfirm, setShowDiscardConfirm] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [focusColumnId, setFocusColumnId] = useState<string | null>(null);
  const [reloadKeyAfterRefresh, setReloadKeyAfterRefresh] = useState<string | null>(null);
  const { notify } = useToast();
  useEffect(() => {
    setShowErrors(false);
  }, [draft?.namespace, draft?.name, mode]);

  useEffect(() => {
    if (!reloadKeyAfterRefresh) return;
    if (isSchemaRefreshing) return;
    const [ns, name] = reloadKeyAfterRefresh.split(".");
    const fresh = schema.find((n) => n.name === ns)?.tables.find((t) => t.name === name);
    if (fresh) {
      dispatch(
        startEditTable({
          tableKey: reloadKeyAfterRefresh,
          draft: tableToDraft(fresh),
        }),
      );
    } else {
      dispatch(discardEdit());
    }
    setReloadKeyAfterRefresh(null);
  }, [reloadKeyAfterRefresh, schema, isSchemaRefreshing, dispatch]);

  if (mode === "idle" || !draft) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 px-6 text-center">
        <div className="rounded-full bg-muted p-4">
          <Pencil className="h-6 w-6 text-muted-foreground" />
        </div>
        <p className="text-base font-medium">No table selected</p>
        <p className="max-w-md text-sm text-muted-foreground">
          Pick a table from the sidebar to edit its schema, or use the <span className="font-medium">+</span> next to <span className="font-medium">Tables</span> in the sidebar to create one.
        </p>
      </div>
    );
  }

  const isCreating = mode === "create";
  const isEditing = mode === "edit";
  const isReadOnly = isReadOnlyNamespace(draft.namespace);

  const updateDraftColumn = (next: DraftColumn) => {
    dispatch(
      setDraft({
        ...draft,
        columns: draft.columns.map((c) => (c.id === next.id ? next : c)),
      }),
    );
  };

  const addColumn = () => {
    const newCol = newDraftColumn();
    setFocusColumnId(newCol.id);
    dispatch(
      setDraft({
        ...draft,
        columns: [...draft.columns, newCol],
      }),
    );
  };

  const deleteColumn = (col: DraftColumn) => {
    if (col.isNew) {
      dispatch(
        setDraft({
          ...draft,
          columns: draft.columns.filter((c) => c.id !== col.id),
        }),
      );
    } else {
      updateDraftColumn({ ...col, isDeleted: !col.isDeleted });
    }
  };

  const droppingColumns = draft.columns.filter((c) => c.isDeleted && !c.isNew);

  const handleSave = async () => {
    if (validation?.hasAny) {
      setShowErrors(true);
      return;
    }
    if (isEditing && droppingColumns.length > 0) {
      setShowDropColsConfirm(true);
      return;
    }
    await doSave();
  };

  const doSave = async () => {
    let sql: string;
    if (isCreating) {
      sql = generateCreateTableSql(draft);
    } else if (isEditing && original) {
      sql = generateAlterTableSql(original, draft);
      if (!sql.trim()) {
        return;
      }
    } else {
      return;
    }

    setIsSaving(true);
    try {
      const ok = await runSql(sql, {
        successTitle: isCreating
          ? `Created ${draft.namespace}.${draft.name}`
          : `Updated ${draft.namespace}.${draft.name}`,
        errorTitle: isCreating ? "Couldn't create table" : "Couldn't save changes",
        notify,
      });
      if (ok) {
        const targetKey = isCreating
          ? `${draft.namespace}.${draft.name}`
          : (selectedTableKey ?? `${draft.namespace}.${draft.name}`);
        setReloadKeyAfterRefresh(targetKey);
        onAfterSave?.();
      }
    } finally {
      setIsSaving(false);
    }
  };

  const handleDiscard = () => {
    if (isDirty) {
      setShowDiscardConfirm(true);
      return;
    }
  };

  const liveColumns = draft.columns.filter((c) => !c.isDeleted || !c.isNew);

  return (
    <div className="flex h-full min-h-0 flex-col overflow-hidden">
      <div className="flex shrink-0 items-center justify-between border-b border-border bg-background px-4 py-3">
        <div>
          <h2 className="text-sm font-semibold">
            {isReadOnly ? "View Table" : isCreating ? "New Table" : "Edit Table"}
          </h2>
          <p className="text-xs text-muted-foreground">
            {isCreating
              ? `Define a new table under ${draft.namespace}.`
              : `${isReadOnly ? "Viewing" : "Editing"} ${draft.namespace}.${draft.name}`}
          </p>
        </div>
        <TooltipProvider delayDuration={250}>
          <div className="flex items-center gap-2">
            {isEditing && !isReadOnly && (
              <Tooltip>
                <TooltipTrigger asChild>
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon"
                    className="h-8 w-8 text-muted-foreground hover:bg-destructive/10 hover:text-destructive"
                    onClick={() => setShowDropConfirm(true)}
                    aria-label="Drop table"
                  >
                    <Trash2 className="h-4 w-4" />
                  </Button>
                </TooltipTrigger>
                <TooltipContent>Drop table</TooltipContent>
              </Tooltip>
            )}
            {!isReadOnly && (
              <>
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  onClick={handleDiscard}
                  disabled={!isDirty}
                  title={!isDirty ? "Nothing to discard" : undefined}
                >
                  Discard
                </Button>
                <Button
                  type="button"
                  size="sm"
                  onClick={() => void handleSave()}
                  disabled={!isDirty || isSaving}
                  title={
                    !isDirty
                      ? "No changes to save"
                      : validation?.hasAny
                        ? "Fix the highlighted errors first"
                        : undefined
                  }
                >
                  {isSaving ? (
                    <Loader2 className="h-3.5 w-3.5 animate-spin" />
                  ) : null}
                  {isSaving
                    ? isCreating
                      ? "Creating…"
                      : "Saving…"
                    : isCreating
                      ? "Create Table"
                      : "Save Changes"}
                </Button>
              </>
            )}
          </div>
        </TooltipProvider>
      </div>

      <div className="min-h-0 flex-1 overflow-auto">
        <div className="space-y-6 px-6 py-4">
          {isReadOnly && (
            <div className="rounded-md border border-amber-500/40 bg-amber-500/5 px-3 py-2 text-xs text-amber-700 dark:text-amber-400">
              <strong>Read-only:</strong> {draft.namespace} is a KalamDB-managed namespace. You can browse the schema but not modify it.
            </div>
          )}

          {showErrors && validation && validation.table.length > 0 && (
            <div className="flex items-start gap-2 rounded-md border border-destructive/40 bg-destructive/5 px-3 py-2 text-xs text-destructive">
              <AlertCircle className="mt-0.5 h-3.5 w-3.5 shrink-0" />
              <ul className="space-y-0.5">
                {validation.table.map((err, idx) => (
                  <li key={`${err}-${idx}`}>{err}</li>
                ))}
              </ul>
            </div>
          )}

          <section className="space-y-4">
            <div className="flex items-baseline gap-2">
              <h3 className="text-sm font-medium">Namespace</h3>
              <code className="rounded bg-muted px-1.5 py-0.5 font-mono text-xs text-foreground">
                {draft.namespace}
              </code>
              {!isEditing && (
                <span className="text-[11px] text-muted-foreground">
                  (pick a different one from the sidebar before creating)
                </span>
              )}
            </div>
            <label className="flex flex-col gap-1.5">
              <h3 className="text-sm font-medium">Table name</h3>
              <Input
                value={draft.name}
                onChange={(e) => dispatch(setDraft({ ...draft, name: e.target.value }))}
                disabled={isEditing}
                placeholder="e.g. users"
                className="h-9 text-sm"
                autoFocus={isCreating}
              />
              {showErrors && validation?.name && (
                <span className="text-[11px] text-destructive">{validation.name}</span>
              )}
            </label>
          </section>

          <section className="space-y-2">
            <div className="flex items-center justify-between">
              <h3 className="text-sm font-medium">Columns</h3>
              {!isReadOnly && (
                <Button type="button" variant="secondary" size="sm" onClick={addColumn} className="gap-1.5 text-xs">
                  <Plus className="h-3.5 w-3.5" />
                  Add column
                </Button>
              )}
            </div>
            <div className="overflow-hidden rounded-md border border-border">
              <table className="w-full text-xs">
                <thead className="bg-muted/40">
                  <tr className="border-b border-border text-[10px] font-medium uppercase tracking-wide text-muted-foreground">
                    <th className="w-8 px-2 py-2 text-center">PK</th>
                    <th className="px-2 py-2 text-left">Name</th>
                    <th className="w-32 px-2 py-2 text-left">Type</th>
                    <th className="w-12 px-2 py-2 text-center">Null</th>
                    <th className="w-12 px-2 py-2 text-center">Uniq</th>
                    <th className="w-44 px-2 py-2 text-left">Default</th>
                    <th className="w-9 px-1 py-2"></th>
                  </tr>
                </thead>
                <tbody>
                  {liveColumns.map((col) => (
                    <ColumnRow
                      key={col.id}
                      column={col}
                      isEditingExistingTable={isEditing}
                      readOnly={isReadOnly}
                      autoFocusName={col.id === focusColumnId}
                      error={showErrors ? validation?.columns[col.id] ?? null : null}
                      onChange={updateDraftColumn}
                      onDelete={() => deleteColumn(col)}
                    />
                  ))}
                  {liveColumns.length === 0 && !isReadOnly && (
                    <tr>
                      <td colSpan={7} className="px-3 py-8">
                        <div className="flex flex-col items-center gap-2 text-center">
                          <p className="text-xs text-muted-foreground">
                            This table has no columns yet.
                          </p>
                          <Button type="button" size="sm" onClick={addColumn} className="gap-1.5">
                            <Plus className="h-3.5 w-3.5" />
                            Add your first column
                          </Button>
                        </div>
                      </td>
                    </tr>
                  )}
                  {liveColumns.length === 0 && isReadOnly && (
                    <tr>
                      <td colSpan={7} className="px-3 py-6 text-center text-muted-foreground">
                        No columns.
                      </td>
                    </tr>
                  )}
                </tbody>
              </table>
            </div>
            {isEditing && (
              <p className="text-[11px] text-muted-foreground">
                Note: primary-key and unique flags can't be changed on existing columns
                (KalamDB doesn't support those ALTERs). You can rename columns, change type,
                nullable and default, or add/drop columns.
              </p>
            )}
          </section>

          {isEditing && selectedTable && (
            <section className="space-y-2">
              <h3 className="text-sm font-medium">Metadata</h3>
              <div className="grid grid-cols-2 gap-x-6 gap-y-2 rounded-md border border-border bg-muted/20 px-4 py-3 text-xs">
                <MetaRow label="Type" value={selectedTable.tableType ?? "—"} />
                <MetaRow
                  label="Rows"
                  value={
                    rowCountLoading
                      ? "loading…"
                      : rowCount !== null
                        ? rowCount.toLocaleString()
                        : "—"
                  }
                />
                <MetaRow
                  label="Version"
                  value={selectedTable.version != null ? String(selectedTable.version) : "—"}
                />
                <MetaRow
                  label="Storage ID"
                  value={selectedTable.storageId ?? "—"}
                  mono
                />
                <MetaRow
                  label="Created"
                  value={formatTimestamp(selectedTable.createdAt)}
                />
                <MetaRow
                  label="Updated"
                  value={formatTimestamp(selectedTable.updatedAt)}
                />
              </div>
            </section>
          )}

        </div>
      </div>

      <ConfirmDialog
        open={showDropColsConfirm}
        title={`Drop ${droppingColumns.length} column${droppingColumns.length === 1 ? "" : "s"}?`}
        description={
          <>
            <p>
              This save will permanently delete the following column{droppingColumns.length === 1 ? "" : "s"}:
            </p>
            <ul className="mt-2 list-disc pl-5 font-mono text-xs">
              {droppingColumns.map((c) => (
                <li key={c.id}>{c.name}</li>
              ))}
            </ul>
            <p className="mt-2">All data in {droppingColumns.length === 1 ? "it" : "them"} will be lost. This cannot be undone.</p>
          </>
        }
        confirmLabel="Save changes"
        variant="destructive"
        onConfirm={() => {
          setShowDropColsConfirm(false);
          void doSave();
        }}
        onClose={() => setShowDropColsConfirm(false)}
      />

      <ConfirmDialog
        open={showDiscardConfirm}
        title="Discard changes?"
        description="You have unsaved edits. They will be lost."
        confirmLabel="Discard"
        variant="destructive"
        onConfirm={() => {
          setShowDiscardConfirm(false);
          if (isEditing && selectedTableKey && selectedTable) {
            dispatch(
              startEditTable({
                tableKey: selectedTableKey,
                draft: tableToDraft(selectedTable),
              }),
            );
          } else {
            dispatch(discardEdit());
          }
        }}
        onClose={() => setShowDiscardConfirm(false)}
      />

      <DestructiveConfirmDialog
        open={showDropConfirm}
        title={`Drop table "${draft.name}"`}
        description={
          <>
            This will permanently delete the table{" "}
            <strong className="text-foreground">
              {draft.namespace}.{draft.name}
            </strong>{" "}
            and all of its data. This cannot be undone.
          </>
        }
        expected={draft.name}
        confirmLabel="Drop table"
        onConfirm={() => {
          setShowDropConfirm(false);
          const sql = generateDropTableSql(draft.namespace, draft.name);
          const tableName = `${draft.namespace}.${draft.name}`;
          void runSql(sql, {
            successTitle: `Dropped ${tableName}`,
            errorTitle: "Drop failed",
            notify,
          }).then((ok) => {
            if (ok) {
              dispatch(discardEdit());
              onAfterSave?.();
            }
          });
        }}
        onClose={() => setShowDropConfirm(false)}
      />
    </div>
  );
}

