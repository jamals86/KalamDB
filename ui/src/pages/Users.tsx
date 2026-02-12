import { UsersList } from "@/components/users/UsersList";
import { PageLayout } from "@/components/layout/PageLayout";

export default function Users() {
  return (
    <PageLayout
      title="Users"
      description="Manage database users and permissions"
    >
      <UsersList />
    </PageLayout>
  );
}
