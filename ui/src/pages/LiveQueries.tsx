import { LiveQueryList } from '@/components/live-queries/LiveQueryList';

export default function LiveQueries() {
  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-3xl font-bold">Live Queries</h1>
        <p className="text-muted-foreground">
          Monitor active WebSocket subscriptions and live query connections
        </p>
      </div>

      <LiveQueryList />
    </div>
  );
}
