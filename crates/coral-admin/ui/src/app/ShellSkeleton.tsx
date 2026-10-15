export function ShellSkeleton() {
  return (
    <div aria-busy className="flex min-h-dvh flex-col">
      <div className="h-topbar border-b border-border bg-surface" />
      <div className="grid flex-1 grid-cols-[minmax(0,1fr)] lg:grid-cols-[var(--spacing-sidebar)_minmax(0,1fr)]">
        <div className="hidden border-r border-border bg-surface lg:block" />
        <div className="flex flex-col gap-2 px-6 pt-5">
          <div className="skeleton h-[22px] w-[180px]" />
          <div className="skeleton h-3 w-[min(420px,80%)]" />
        </div>
      </div>
      <div className="h-statusbar border-t border-border bg-surface" />
    </div>
  );
}
