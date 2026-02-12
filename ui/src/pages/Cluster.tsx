import { ClusterHealth } from '@/components/cluster/ClusterHealth';
import { ClusterNodesList } from '@/components/cluster/ClusterNodesList';
import { Button } from '@/components/ui/button';
import { useGetClusterSnapshotQuery } from '@/store/apiSlice';

export default function Cluster() {
  const {
    data,
    isFetching: isLoading,
    error,
    refetch,
  } = useGetClusterSnapshotQuery(undefined, {
    pollingInterval: 5000,
  });
  const nodes = data?.nodes ?? [];
  const health = data?.health ?? null;
  const errorMessage =
    error && "error" in error && typeof error.error === "string"
      ? error.error
      : error
        ? "Failed to fetch cluster information"
        : null;

  if (errorMessage) {
    return (
      <div className="space-y-4">
        <div className="p-4 bg-red-50 border border-red-200 rounded-lg">
          <p className="text-red-700">{errorMessage}</p>
          <Button variant="outline" onClick={() => void refetch()} className="mt-2">
            Retry
          </Button>
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-4">
      {health && <ClusterHealth health={health} />}

      <ClusterNodesList 
        nodes={nodes} 
        isLoading={isLoading} 
        onRefresh={() => void refetch()}
      />
    </div>
  );
}
