import { StorageList } from "@/components/storages/StorageList";

export default function Storages() {
  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-3xl font-bold">Storages</h1>
        <p className="text-muted-foreground">
          View storage configurations and status
        </p>
      </div>

      <StorageList />
    </div>
  );
}
