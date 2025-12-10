import { JobList } from "@/components/jobs/JobList";

export default function Jobs() {
  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-3xl font-bold">Jobs</h1>
        <p className="text-muted-foreground">
          View and monitor background jobs in the system
        </p>
      </div>

      <JobList />
    </div>
  );
}
