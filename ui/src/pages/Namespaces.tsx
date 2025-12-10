import { NamespaceList } from "@/components/namespaces/NamespaceList";

export default function Namespaces() {
  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-3xl font-bold">Namespaces</h1>
        <p className="text-muted-foreground">
          View and manage database namespaces
        </p>
      </div>

      <NamespaceList />
    </div>
  );
}
