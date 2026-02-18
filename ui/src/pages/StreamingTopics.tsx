import { useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { RefreshCw, Search, ArrowRight } from "lucide-react";
import { PageLayout } from "@/components/layout/PageLayout";
import { StreamingTabs } from "@/features/streaming/components/StreamingTabs";
import { useGetStreamingTopicsQuery } from "@/store/apiSlice";
import { buildTopicSqlSnippet } from "@/features/streaming/sql";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { formatTimestamp } from "@/lib/formatters";

function formatNullableTimestamp(value: string | null): string {
  if (!value) {
    return "-";
  }
  return formatTimestamp(value, undefined, "iso8601-datetime", "utc");
}

function formatNullableNumber(value: number | null): string {
  if (value === null || value === undefined) {
    return "-";
  }
  return value.toLocaleString();
}

export default function StreamingTopics() {
  const navigate = useNavigate();
  const {
    data: topics = [],
    isFetching,
    error,
    refetch,
  } = useGetStreamingTopicsQuery();
  const [search, setSearch] = useState("");

  const filteredTopics = useMemo(() => {
    const query = search.trim().toLowerCase();
    if (!query) {
      return topics;
    }
    return topics.filter(
      (topic) =>
        topic.topicId.toLowerCase().includes(query) ||
        topic.name.toLowerCase().includes(query),
    );
  }, [search, topics]);

  const errorMessage =
    error && "error" in error && typeof error.error === "string"
      ? error.error
      : error
        ? "Failed to load topics"
        : null;

  return (
    <PageLayout
      title="Streaming"
      description="Topic inventory and streaming diagnostics"
      actions={(
        <Button variant="outline" size="sm" onClick={() => void refetch()} disabled={isFetching}>
          <RefreshCw className={`mr-1.5 h-4 w-4 ${isFetching ? "animate-spin" : ""}`} />
          Refresh
        </Button>
      )}
    >
      <StreamingTabs />

      <div className="relative max-w-sm">
        <Search className="pointer-events-none absolute left-3 top-2.5 h-4 w-4 text-muted-foreground" />
        <Input
          placeholder="Search topics..."
          className="pl-9"
          value={search}
          onChange={(event) => setSearch(event.target.value)}
        />
      </div>

      {errorMessage && (
        <Card className="border-destructive/30 bg-destructive/5">
          <CardContent className="pt-4 text-sm text-destructive">{errorMessage}</CardContent>
        </Card>
      )}

      <Card>
        <CardHeader className="pb-2">
          <CardTitle className="text-base">Topics</CardTitle>
          <CardDescription>
            {filteredTopics.length} topic{filteredTopics.length === 1 ? "" : "s"} visible
          </CardDescription>
        </CardHeader>
        <CardContent>
          <div className="rounded-md border">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Topic</TableHead>
                  <TableHead>Partitions</TableHead>
                  <TableHead>Routes</TableHead>
                  <TableHead>Retention (sec)</TableHead>
                  <TableHead>Updated</TableHead>
                  <TableHead className="w-[170px]">Actions</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {filteredTopics.length === 0 ? (
                  <TableRow>
                    <TableCell colSpan={6} className="py-8 text-center text-muted-foreground">
                      {search.trim() ? "No topics match your search." : "No topics found."}
                    </TableCell>
                  </TableRow>
                ) : (
                  filteredTopics.map((topic) => (
                    <TableRow key={topic.topicId}>
                      <TableCell className="font-mono text-xs">{topic.topicId}</TableCell>
                      <TableCell>{topic.partitions}</TableCell>
                      <TableCell>{topic.routeCount}</TableCell>
                      <TableCell>{formatNullableNumber(topic.retentionSeconds)}</TableCell>
                      <TableCell className="font-mono text-xs">{formatNullableTimestamp(topic.updatedAt)}</TableCell>
                      <TableCell>
                        <div className="flex items-center gap-2">
                          <Button
                            size="sm"
                            variant="outline"
                            onClick={() => {
                              navigate("/sql", {
                                state: {
                                  prefillSql: buildTopicSqlSnippet(topic.topicId),
                                  prefillTitle: `Topic ${topic.topicId}`,
                                },
                              });
                            }}
                          >
                            Open SQL
                          </Button>
                          <Button
                            size="sm"
                            onClick={() => navigate(`/streaming/topics/${encodeURIComponent(topic.topicId)}`)}
                          >
                            Inspect
                            <ArrowRight className="ml-1 h-3.5 w-3.5" />
                          </Button>
                        </div>
                      </TableCell>
                    </TableRow>
                  ))
                )}
              </TableBody>
            </Table>
          </div>
        </CardContent>
      </Card>
    </PageLayout>
  );
}

