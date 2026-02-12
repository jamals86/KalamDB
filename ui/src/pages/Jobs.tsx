import { JobList } from "@/components/jobs/JobList";
import { PageLayout } from "@/components/layout/PageLayout";

export default function Jobs() {
  return (
    <PageLayout
      title="Jobs"
      description="View and monitor background jobs in the system"
    >
      <JobList />
    </PageLayout>
  );
}
