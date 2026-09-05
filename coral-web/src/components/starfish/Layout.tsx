import Link from "next/link";

export function StarfishNav({ active }: { active?: "home" | "docs" }) {
  return (
    <nav className="fixed top-0 w-full z-50 bg-[rgba(10,10,10,0.8)] backdrop-blur-xl border-b border-white/[0.06]">
      <div className="max-w-6xl mx-auto px-6 h-14 flex items-center justify-between">
        <Link href="/" className="flex items-center gap-2.5">
          <img src="/starfish.png" alt="Starfish" width={22} height={22} className="pixelated" />
          <span className="text-base font-bold tracking-tight">Starfish</span>
        </Link>
        <div className="flex items-center gap-6 text-sm">
          <Link href="/docs" className={`hover:text-white/70 transition-colors ${active === "docs" ? "text-white/70" : "text-white/40"}`}>Docs</Link>
          <a href="https://discord.gg/jgKEVUJj3H" target="_blank" rel="noreferrer" className="text-white/40 hover:text-white/70 transition-colors">Discord</a>
        </div>
      </div>
    </nav>
  );
}

export function StarfishFooter() {
  return (
    <footer className="border-t border-white/[0.06] py-6 px-6 mt-auto">
      <div className="max-w-6xl mx-auto flex flex-col sm:flex-row items-center justify-between gap-3 text-xs text-white/20">
        <span>&copy; {new Date().getFullYear()} Starfish</span>
        <p>Not affiliated with Hypixel Inc., Mojang AB, or Microsoft.</p>
      </div>
    </footer>
  );
}
