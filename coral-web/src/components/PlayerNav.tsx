"use client";

import Link from "next/link";
import { SearchSuggest } from "@/components/common/SearchSuggest";

export function PlayerNav() {
  return (
    <header className="fixed top-0 w-full z-50 bg-[rgba(10,10,10,0.8)] backdrop-blur-xl border-b border-white/[0.06]">
      <div className="max-w-7xl mx-auto flex items-center gap-3 px-4 h-14">
        <Link href="/" className="flex items-center gap-2">
          <img src="/logo.png" alt="Coral" width={20} height={20} />
          <span className="text-lg font-bold tracking-tight">Coral</span>
        </Link>
        <form action="/search" method="GET" autoComplete="off" className="ml-auto w-full max-w-sm">
          <SearchSuggest
            placeholder="Search player..."
            inputHeightClass="h-9"
            buttonSizeClass="h-9 w-9"
            listMaxHeightClass="max-h-[200px]"
            rowHeightClass="h-10"
            imgSize={22}
            scrollClass="scroll-hidden"
            autoFocus={false}
          />
        </form>
      </div>
    </header>
  );
}
