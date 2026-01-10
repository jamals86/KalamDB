import { useEffect } from 'react';
import { useCluster } from '@/hooks/useCluster';
import { ClusterHealth } from '@/components/cluster/ClusterHealth';
import { ClusterNodesList } from '@/components/cluster/ClusterNodesList';
import { Button } from '@/components/ui/button';
import { Activity } from 'lucide-react';

export default function Cluster() {
  const { nodes, health, isLoading, error, fetchCluster } = useCluster();

  useEffect(() => {
    fetchCluster();
    // Auto-refresh every 5 seconds
    const interval = setInterval(fetchCluster, 5000);
    return () => clearInterval(interval);
  }, [fetchCluster]);

  if (error) {
    return (
      <div className="p-6 space-y-6">
        <div>
          <h1 className="text-3xl font-bold">Cluster</h1>
          <p className="text-muted-foreground">
            View cluster health and node status
          </p>
        </div>
        <div className="p-4 bg-red-50 border border-red-200 rounded-lg">
          <p className="text-red-700">{error}</p>
          <Button variant="outline" onClick={fetchCluster} className="mt-2">
            Retry
          </Button>
        </div>
      </div>
    );
  }

  return (
    <div className="p-6 space-y-6">
      <div>
        <h1 className="text-3xl font-bold flex items-center gap-2">
          <Activity className="h-8 w-8" />
          Cluster
        </h1>
        <p className="text-muted-foreground">
          View cluster health and node status
        </p>
      </div>

      {health && <ClusterHealth health={health} />}

      <ClusterNodesList 
        nodes={nodes} 
        isLoading={isLoading} 
        onRefresh={fetchCluster}
      />
    </div>
  );
}
