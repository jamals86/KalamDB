import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { CheckCircle2, XCircle, Activity, Crown, Server } from 'lucide-react';
import type { ClusterHealth as ClusterHealthType } from '@/services/clusterService';

interface ClusterHealthProps {
  health: ClusterHealthType;
}

export function ClusterHealth({ health }: ClusterHealthProps) {
  const healthItems = [
    {
      label: 'Total Nodes',
      value: health.totalNodes,
      icon: Server,
      color: 'text-blue-600',
      bgColor: 'bg-blue-50',
    },
    {
      label: 'Active Nodes',
      value: health.activeNodes,
      icon: CheckCircle2,
      color: 'text-green-600',
      bgColor: 'bg-green-50',
    },
    {
      label: 'Offline Nodes',
      value: health.offlineNodes,
      icon: XCircle,
      color: health.offlineNodes > 0 ? 'text-red-600' : 'text-gray-400',
      bgColor: health.offlineNodes > 0 ? 'bg-red-50' : 'bg-gray-50',
    },
    {
      label: 'Leader Nodes',
      value: health.leaderNodes,
      icon: Crown,
      color: 'text-yellow-600',
      bgColor: 'bg-yellow-50',
    },
  ];

  return (
    <div className="space-y-4">
      {/* Overall Health Status */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Activity className="h-5 w-5" />
            Cluster Health
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="flex items-center gap-3">
            {health.healthy ? (
              <>
                <CheckCircle2 className="h-8 w-8 text-green-600" />
                <div>
                  <div className="text-lg font-semibold text-green-700">Healthy</div>
                  <div className="text-sm text-muted-foreground">
                    All nodes are operational
                  </div>
                </div>
              </>
            ) : (
              <>
                <XCircle className="h-8 w-8 text-red-600" />
                <div>
                  <div className="text-lg font-semibold text-red-700">Degraded</div>
                  <div className="text-sm text-muted-foreground">
                    {health.offlineNodes > 0 && `${health.offlineNodes} node(s) offline`}
                    {health.joiningNodes > 0 && ` ${health.joiningNodes} node(s) joining`}
                    {health.catchingUpNodes > 0 && ` ${health.catchingUpNodes} node(s) catching up`}
                  </div>
                </div>
              </>
            )}
          </div>
        </CardContent>
      </Card>

      {/* Health Metrics Grid */}
      <div className="grid grid-cols-2 lg:grid-cols-4 gap-4">
        {healthItems.map((item) => {
          const Icon = item.icon;
          return (
            <Card key={item.label}>
              <CardContent className="pt-6">
                <div className="flex items-center justify-between">
                  <div>
                    <div className="text-2xl font-bold">{item.value}</div>
                    <div className="text-sm text-muted-foreground">{item.label}</div>
                  </div>
                  <div className={`${item.bgColor} rounded-full p-3`}>
                    <Icon className={`h-6 w-6 ${item.color}`} />
                  </div>
                </div>
              </CardContent>
            </Card>
          );
        })}
      </div>
    </div>
  );
}
