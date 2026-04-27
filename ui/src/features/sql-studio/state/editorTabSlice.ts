import { createSlice, type PayloadAction } from "@reduxjs/toolkit";
import type { DraftTable, EditorMode } from "@/components/sql-studio-v2/table-editor/types";

interface EditorTabState {
  mode: EditorMode;
  selectedTableKey: string | null;
  draft: DraftTable | null;
  original: DraftTable | null;
}

const initialState: EditorTabState = {
  mode: "idle",
  selectedTableKey: null,
  draft: null,
  original: null,
};

const editorTabSlice = createSlice({
  name: "editorTab",
  initialState,
  reducers: {
    startCreateTable(state, action: PayloadAction<{ namespace: string; emptyDraft: DraftTable }>) {
      state.mode = "create";
      state.selectedTableKey = null;
      state.draft = action.payload.emptyDraft;
      state.original = JSON.parse(JSON.stringify(action.payload.emptyDraft));
    },
    startEditTable(
      state,
      action: PayloadAction<{ tableKey: string; draft: DraftTable }>,
    ) {
      state.mode = "edit";
      state.selectedTableKey = action.payload.tableKey;
      state.draft = action.payload.draft;
      state.original = JSON.parse(JSON.stringify(action.payload.draft));
    },
    discardEdit(state) {
      state.mode = "idle";
      state.selectedTableKey = null;
      state.draft = null;
      state.original = null;
    },
    setDraft(state, action: PayloadAction<DraftTable>) {
      state.draft = action.payload;
    },
  },
});

export const { startCreateTable, startEditTable, discardEdit, setDraft } = editorTabSlice.actions;
export default editorTabSlice.reducer;
