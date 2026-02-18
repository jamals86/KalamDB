import { useMemo, useState } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import { RefreshCw } from "lucide-react";
import { PageLayout } from "@/components/layout/PageLayout";
import { StreamingTabs } from "@/features/streaming/components/StreamingTabs";
import {
  useGetStreamingConsumerGroupsQuery,
  useGetStreamingOffsetsQuery,
  useGetStreamingTopicsQuery,
} from "@/store/apiSlice";
import { buildGroupSqlSnippet, buildTopicSqlSnippet } from "@/features/streaming/sql";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { formatTimestamp } from "@/lib/formatters";

const ALL_FILTERS = "__all__";

function formatNullableTimestamp(value: string | null): string {
  if (!value) {
    return "-";
  }
  return formatTimestamp(value, undefined, "iso8601-datetime", "utc");
}

export default function StreamingOffsets() {
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const initialTopic = searchParams.get("topic");
  const initialGroup = searchParams.get("group");
  const [selectedTopic, setSelectedTopic] = useState(initialTopic || ALL_FILTERS);
  const [selectedGroup, setSelectedGroup] = useState(initialGroup || ALL_FILTERS);

  const { data: topics = [] } = useGetStreamingTopicsQuery();
  const { data: groups = [] } = useGetStreamingConsumerGroupsQuery();

  const filters = useMemo(() => ({
    topicId: selectedTopic === ALL_FILTERS ? undefined : selectedTopic,
    groupId: selectedGroup === ALL_FILTERS ? undefined : selectedGroup,
    limit: 1000,
  }), [selectedGroup, selectedTopic]);

  const {
    data: offsets = [],
    isFetching,
    error,
    refetch,
  } = useGetStreamingOffsetsQuery(filters);

  const errorMessage =
    error && "error" in error && typeof error.error === "string"
      ? error.error
      : error
        ? "Failed to load offsets"
        : null;

  const selectedTopicId = selectedTopic === ALL_FILTERS ? null : selectedTopic;
  const selectedGroupId = selectedGroup === ALL_FILTERS ? null : selectedGroup;

  return (
    <PageLayout
      title="Streaming"
      description="Offset snapshots by topic, group, and partition"
      actions={(
        <Button variant="outline" size="sm" onClick={() => void refetch()} disabled={isFetching}>
          <RefreshCw className={`mr-1.5 h-4 w-4 ${isFetching ? "animate-spin" : ""}`} />
          Refresh
        </Button>
      )}
    >
      <StreamingTabs />

      <Card>
        <CardHeader className="pb-2">
          <CardTitle className="text-base">Filters</CardTitle>
          <CardDescription>Limit offset diagnostics by topic and/or consumer group</CardDescription>
        </CardHeader>
        <CardContent className="grid gap-3 md:grid-cols-2">
          <div className="space-y-1">
            <p className="text-xs uppercase tracking-wide text-muted-foreground">Topic</p>
            <Select value={selectedTopic} onValueChange={setSelectedTopic}>
              <SelectTrigger>
                <SelectValue placeholder="All topics" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value={ALL_FILTERS}>All topics</SelectItem>
                {topics.map((topic) => (
                  <SelectItem key={topic.topicId} value={topic.topicId}>{topic.topicId}</SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          <div className="space-y-1">
            <p className="text-xs uppercase tracking-wide text-muted-foreground">Group</p>
            <Select value={selectedGroup} onValueChange={setSelectedGroup}>
              <SelectTrigger>
                <SelectValue placeholder="All groups" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value={ALL_FILTERS}>All groups</SelectItem>
                {groups.map((group) => (
                  <SelectItem key={group.groupId} value={group.groupId}>{group.groupId}</SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        </CardContent>
      </Card>

      {errorMessage && (
        <Card className="border-destructive/30 bg-destructive/5">
          <CardContent className="pt-4 text-sm text-destructive">{errorMessage}</CardContent>
        </Card>
      )}

      <Card>
        <CardHeader className="pb-2">
          <CardTitle className="text-base">Offsets</CardTitle>
          <CardDescription>{offsets.length} row{offsets.length === 1 ? "" : "s"} returned</CardDescription>
        </CardHeader>
        <CardContent className="space-y-3">
          <div className="flex flex-wrap items-center gap-2">
            {selectedTopicId && (
              <Button
                size="sm"
                variant="outline"
                onClick={() =>
                  navigate("/sql", {
                    state: {
                      prefillSql: buildTopicSqlSnippet(selectedTopicId),
                      prefillTitle: `Topic ${selectedTopicId}`,
                    },
                  })
                }
              >
                Open Topic SQL
              </Button>
            )}
            {selectedGroupId && (
              <Button
                size="sm"
                variant="outline"
                onClick={() =>
                  navigate("/sql", {
                    state: {
                      prefillSql: buildGroupSqlSnippet(selectedGroupId),
                      prefillTitle: `Group ${selectedGroupId}`,
                    },
                  })
                }
              >
                Open Group SQL
              </Button>
            )}
          </div>

          <div className="rounded-md border">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Topic</TableHead>
                  <TableHead>Group</TableHead>
                  <TableHead>Partition</TableHead>
                  <TableHead>Last Acked</TableHead>
                  <TableHead>Next Offset</TableHead>
                  <TableHead>Updated</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {offsets.length === 0 ? (
                  <TableRow>
                    <TableCell colSpan={6} className="py-8 text-center text-muted-foreground">
                      No offsets found for the selected filters.
                    </TableCell>
                  </TableRow>
                ) : (
                  offsets.map((offset) => (
                    <TableRow key={`${offset.topicId}:${offset.groupId}:${offset.partitionId}`}>
                      <TableCell className="font-mono text-xs">{offset.topicId}</TableCell>
                      <TableCell className="font-mono text-xs">{offset.groupId}</TableCell>
                      <TableCell>{offset.partitionId}</TableCell>
                      <TableCell>{offset.lastAckedOffset}</TableCell>
                      <TableCell>{offset.nextOffset}</TableCell>
                      <TableCell className="font-mono text-xs">{formatNullableTimestamp(offset.updatedAt)}</TableCell>
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

