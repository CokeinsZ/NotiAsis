import DashboardApp from "@/components/DashboardApp";

export const dynamic = "force-dynamic";

export default async function DashboardPage({
  params,
}: {
  params: Promise<{ businessId: string }>;
}) {
  const { businessId } = await params;
  const id = Number(businessId);

  if (!Number.isInteger(id) || id < 1) {
    return (
      <main className="flex min-h-screen items-center justify-center bg-black">
        <p className="text-moon">Empresa inválida.</p>
      </main>
    );
  }

  return <DashboardApp businessId={id} />;
}
