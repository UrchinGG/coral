export function Placeholder({ title, description }: { title: string; description: string }) {
  return (
    <div className="flex flex-col items-center justify-center gap-2 py-24 text-center">
      <h1 className="text-lg font-semibold text-gray-200">{title}</h1>
      <p className="max-w-sm text-sm text-gray-500">{description}</p>
    </div>
  );
}
