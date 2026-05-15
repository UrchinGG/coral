import Link from "next/link";
import type { Metadata } from "next";

export const metadata: Metadata = {
  title: "Starfish Docs",
  description: "Plugin API reference for Starfish.",
};

export default function DocsPage() {
  return (
    <div>
      <h1 className="text-3xl font-bold tracking-tight mb-2">Plugin API</h1>
      <p className="text-sm text-white/40 mb-10">Lua scripting reference for Starfish plugins.</p>

      <div className="space-y-3">
        <DocLink href="/starfish/docs/http" title="HTTP" description="Make HTTP requests with async callbacks." />
        <DocLink href="/starfish/docs/webhooks" title="Webhooks" description="Register HTTP endpoints that external services can call." />
      </div>
    </div>
  );
}

function DocLink({ href, title, description }: { href: string; title: string; description: string }) {
  return (
    <Link href={href} className="block rounded-lg border border-white/[0.08] bg-[rgba(0,0,0,0.5)] p-4 hover:border-white/[0.14] transition-colors">
      <h3 className="text-sm font-semibold text-white/70 mb-1">{title}</h3>
      <p className="text-[12px] text-white/35">{description}</p>
    </Link>
  );
}
