import { TreeItem } from "./TreeItem";
import type { SchemaNode } from "../types";

interface SchemaTreeProps {
  nodes: SchemaNode[];
  onToggle: (path: string[]) => void;
  onInsert: (text: string) => void;
  onShowProperties: (namespace: string, tableName: string, columns: SchemaNode[]) => void;
  onContextMenu: (e: React.MouseEvent, namespace: string, tableName: string, columns: SchemaNode[]) => void;
}

export function SchemaTree({
  nodes,
  onToggle,
  onInsert,
  onShowProperties,
  onContextMenu,
}: SchemaTreeProps) {
  const renderTree = (treeNodes: SchemaNode[], path: string[] = []): React.ReactNode => {
    return treeNodes.map((node) => (
      <TreeItem
        key={[...path, node.name].join(".")}
        node={node}
        path={path}
        onToggle={onToggle}
        onInsert={onInsert}
        onShowProperties={onShowProperties}
        onContextMenu={onContextMenu}
        renderChildren={renderTree}
      />
    ));
  };

  return <>{renderTree(nodes)}</>;
}
