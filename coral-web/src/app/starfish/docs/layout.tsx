import { StarfishNav, StarfishFooter } from "@/components/starfish/Layout";
import { DocsSidebar } from "@/components/starfish/DocsSidebar";

export default function DocsLayout({ children }: { children: React.ReactNode }) {
  return (
    <div className="min-h-screen flex flex-col">
      <StarfishNav active="docs" />
      <div className="flex-1 w-full max-w-5xl mx-auto px-6 pt-20 pb-16 flex gap-10">
        <DocsSidebar />
        <main className="flex-1 min-w-0">{children}</main>
      </div>
      <StarfishFooter />
    </div>
  );
}
