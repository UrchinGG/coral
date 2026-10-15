import { ErrorState } from "../kit/States";

export function AppFailure({ title, error }: { title: string; error: unknown }) {
  return (
    <main className="mx-auto flex min-h-dvh max-w-xl flex-col justify-center gap-3 p-6">
      <h1 className="text-22 font-semibold">{title}</h1>
      <ErrorState error={error} onRetry={() => window.location.reload()} />
    </main>
  );
}
