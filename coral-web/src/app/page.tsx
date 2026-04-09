"use client";

import { useState } from "react";
import Link from "next/link";
import { SearchSuggest } from "@/components/common/SearchSuggest";


export default function Home() {
  const error = (() => {
    if (typeof window === "undefined") return undefined;
    const e = new URLSearchParams(window.location.search).get("e");
    return e === "inv" ? "Invalid player or UUID."
      : e === "np" ? "This player has never played on Hypixel."
      : e === "iapikey" ? "Internal error. Please try again later."
      : undefined;
  })();

  return (
    <div className="min-h-screen flex flex-col">
      <Nav />

      {/* Hero */}
      <section className="flex items-center justify-center pt-14 min-h-[70vh]">
        <div className="w-full max-w-xl mx-auto px-6 text-center py-20">
          <img src="/logo.png" alt="Coral" width={48} height={48} className="mx-auto mb-5 pixelated" />
          <h1 className="text-4xl sm:text-5xl font-bold tracking-tight mb-3">Coral</h1>
          <p className="text-base text-white/40 mb-8">Community-driven API and tools for Hypixel.</p>
          <form action="/search" method="GET" autoComplete="off">
            <SearchSuggest
              placeholder="Search for a player..."
              inputHeightClass="h-13" buttonSizeClass="h-13 w-13"
              listMaxHeightClass="max-h-[280px]" rowHeightClass="h-11"
              imgSize={26} scrollClass="scroll-hidden" autoFocus={false}
            />
          </form>
          {error && (
            <div className="mt-4 inline-flex">
              <div className="rounded-md border border-red-400/20 bg-red-500/10 text-red-300 px-4 py-2 text-sm">{error}</div>
            </div>
          )}
        </div>
      </section>

      <BotSection />
      <ApiSection />
      <StarfishSection />
      <Footer />
    </div>
  );
}



function BotSection() {
  return (
    <Section wide>
      <SectionLabel>Discord Bot</SectionLabel>
      <h2 className="text-2xl font-bold mb-2">Hypixel stats, blacklist tags, and Discord sync.</h2>
      <p className="text-sm text-white/40 mb-8 max-w-lg">
        Check current or historical Hypixel stats for any player, see if they're cheating,
        and configure server-wide nickname and role syncing based on Hypixel data.
      </p>
      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        <div className="space-y-4">
          <TagViewMock />
          <BedwarsMock />
        </div>
        <div className="flex flex-col gap-4">
          <SetupMock />
          <BotCard />
        </div>
      </div>
    </Section>
  );
}

function TagViewMock() {
  return (
    <div>
      <SlashLabel>/tag view F1nalist</SlashLabel>
      <Cv2>
        <Cv2Row>
          <div className="flex-1 min-w-0">
            <div className="text-[13px] font-semibold text-white/80 mb-0.5 flex items-center gap-1.5">
              <Mdi d={MDI.tagTextOutline} fill="#C0C0C0" /> Tagged User
            </div>
            <div className="text-[12px] text-white/50">
              IGN - <Code>F1nalist</Code>
            </div>
          </div>
          <img src="https://vzge.me/face/256/43910c95ba604266a61f86936a901f4e.png"
            alt="" width={44} height={44} className="pixelated rounded shrink-0" />
        </Cv2Row>

        <div className="space-y-2">
          <TagEntry icon={MDI.octagramOutline} fill="#AF00AF" name="Confirmed Cheater"
            reason="legitscaff (doubleshifting), lagrange, speedmine"
            by="hexze" time="9 days ago" />
        </div>

        <Cv2Sep />
        <Cv2Muted>UUID: 43910c95-ba60-4266-a61f-86936a901f4e | Evidence</Cv2Muted>
      </Cv2>
    </div>
  );
}

function BedwarsMock() {
  return (
    <div>
      <SlashLabel>/bedwars Seifig</SlashLabel>
      <div className="space-y-2">
        <div className="rounded-lg border border-white/[0.08] bg-[rgba(0,0,0,0.5)] overflow-hidden">
          <img src="/showcase/bedwars-card.png" alt="Bedwars stats for Seifig" width={800} height={600} className="w-full block pixelated" />
        </div>
        <DiscordSelect options={["Solos", "Doubles", "Threes", "Fours", "4v4"]} />
      </div>
    </div>
  );
}

function BotCard() {
  return (
    <div className="flex-1 flex items-center gap-4 rounded-lg border border-white/[0.08] bg-[rgba(0,0,0,0.5)] p-4 overflow-hidden">
      <img src="/logo.png" alt="Coral" width={56} height={56} className="pixelated" />
      <span className="text-[16px] font-bold text-white/70 flex-1">Coral</span>
      <a href="#" className="px-4 py-1.5 rounded-md bg-[#5865F2] hover:bg-[#4752C4] text-white text-[13px] transition-colors shrink-0">
        Add to Server
      </a>
    </div>
  );
}

function SetupMock() {
  return (
    <div>
      <SlashLabel>/setup</SlashLabel>
      <Cv2>
        <div className="text-[13px] font-semibold text-white/80">Server Configuration</div>
        <Cv2Sep />

        <Cv2Field label="Linked Role">
          <MockSelect>@Linked</MockSelect>
        </Cv2Field>
        <Cv2Field label="Unlinked Role">
          <MockSelect>@Unlinked</MockSelect>
        </Cv2Field>
        <Cv2Field label="Link Channel">
          <MockSelect>#verify</MockSelect>
        </Cv2Field>

        <Cv2Sep />

        <Cv2Field label="Display Name Format">
          <div className="text-[11px] text-white/50 mb-1"><Code>[482✫] Technoblade</Code></div>
          <pre className="text-[10px] text-white/30 bg-white/[0.04] border border-white/[0.07] rounded px-2 py-1.5 font-mono overflow-hidden">
{"[{achievements.bedwars_level}✫] {displayname}"}
          </pre>
        </Cv2Field>
        <Cv2Buttons>
          <Cv2Btn>Display Name Config</Cv2Btn>
        </Cv2Buttons>

        <Cv2Sep />

        <Cv2Field label="Autoroles">
          <div className="flex flex-wrap gap-1 text-[11px]">
            <RolePill color="#e74c3c">Cheater</RolePill>
            <RolePill color="#9b59b6">[0-999✫]</RolePill>
            <RolePill color="#3498db">[1000-1999✫]</RolePill>
            <RolePill color="#f1c40f">[2000+✫]</RolePill>
          </div>
        </Cv2Field>
        <Cv2Buttons>
          <Cv2Btn>Autorole Config</Cv2Btn>
        </Cv2Buttons>
      </Cv2>
    </div>
  );
}



function ApiSection() {
  const uuid = "43910c95ba604266a61f86936a901f4e";
  const endpoints = [
    {
      label: "Player Tags",
      path: `/v3/player/tags?uuid=${uuid}`,
      response: `{
  "uuid": "${uuid}",
  "tags": [
    {
      "tag_type": "confirmed_cheater",
      "reason": "legitscaff (doubleshifting), lagrange, speedmine",
      "added_by": 354598337325826048,
      "added_on": 1743206400000,
      "hide_username": false
    }
  ]
}`,
    },
    {
      label: "Monthly Delta",
      path: `/v3/player/sessions/monthly?player=${uuid}`,
      response: `{
  "uuid": "${uuid}",
  "from": 1741017000000,
  "from_readable": "Mar 01, 2026 2:30 PM EST",
  "delta": {
    "stats": {
      "Bedwars": {
        "wins_bedwars": 341,
        "final_kills_bedwars": 1290,
        "losses_bedwars": 128,
        "beds_broken_bedwars": 412
      }
    }
  }
}`,
    },
    {
      label: "Winstreaks",
      path: `/v3/player/winstreaks?player=${uuid}`,
      response: `{
  "uuid": "${uuid}",
  "modes": {
    "overall": [
      { "value": 23, "approximate": false,
        "timestamp": 1743206400000,
        "readable": "Mar 29, 2026 00:00 UTC" },
      { "value": 14, "approximate": true,
        "timestamp": 1743120000000,
        "readable": "Mar 28, 2026 00:00 UTC" }
    ],
    "doubles": [
      { "value": 31, "approximate": false,
        "timestamp": 1743206400000,
        "readable": "Mar 29, 2026 00:00 UTC" }
    ]
  }
}`,
    },
  ];

  const [idx, setIdx] = useState(0);
  const ep = endpoints[idx];

  return (
    <Section wide>
      <SectionLabel>Developer API</SectionLabel>
      <h2 className="text-2xl font-bold mb-2">Build on top of Coral.</h2>
      <p className="text-sm text-white/40 mb-8 max-w-lg">
        Free REST API for everything we can share without compromising player privacy;
        player tags, session deltas, winstreak history, and more to come.
      </p>
      <div className="rounded-lg border border-white/[0.08] overflow-hidden bg-[rgba(0,0,0,0.5)]">
        <div className="flex border-b border-white/[0.08]">
          {endpoints.map((e, i) => (
            <button key={i} onClick={() => setIdx(i)}
              className={`flex-1 py-2.5 text-[12px] text-center transition-colors border-b-2 -mb-px ${i === idx
                ? "text-white/70 border-white/30 bg-white/[0.03]"
                : "text-white/25 border-transparent hover:text-white/40"}`}
            >{e.label}</button>
          ))}
        </div>
        <div className="p-5">
          <div className="flex items-center gap-2 mb-4 text-[12px]">
            <span className="px-1.5 py-0.5 rounded bg-emerald-500/15 text-emerald-400/80 text-[10px] font-bold tracking-wide">GET</span>
            <code className="text-white/40 font-mono">{ep.path}</code>
          </div>
          <pre className="text-[11px] text-white/45 font-mono leading-[1.6] overflow-x-auto">{ep.response}</pre>
        </div>
      </div>
      <div className="mt-6">
        <Link href="/docs" className="inline-block px-5 py-2 rounded-md bg-white/[0.06] hover:bg-white/[0.10] text-white/60 text-sm transition-colors">
          API Documentation →
        </Link>
      </div>
    </Section>
  );
}



function StarfishSection() {
  return (
    <Section>
      <div className="flex items-center gap-4">
        <img src="/starfish.png" alt="Starfish" width={40} height={40} className="pixelated" />
        <div>
          <h2 className="text-2xl font-bold">Starfish</h2>
          <p className="text-sm text-white/40">Hypixel proxy + injectable with Lua scripting. Coming soon.</p>
        </div>
      </div>
    </Section>
  );
}



function Cv2({ children, className = "" }: { children: React.ReactNode; className?: string }) {
  return <div className={`rounded-lg border border-white/[0.08] bg-[rgba(0,0,0,0.5)] p-4 space-y-2.5 ${className}`}>{children}</div>;
}

function Cv2Row({ children }: { children: React.ReactNode }) {
  return <div className="flex gap-3">{children}</div>;
}

function Cv2Sep() {
  return <div className="border-t border-white/[0.07]" />;
}

function Cv2Muted({ children }: { children: React.ReactNode }) {
  return <div className="text-[10px] text-white/20">{children}</div>;
}

function Cv2Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div>
      <div className="text-[12px] text-white/60 font-medium mb-1">{label}</div>
      <div className="text-[12px] text-white/40">{children}</div>
    </div>
  );
}

function Cv2Buttons({ children }: { children: React.ReactNode }) {
  return <div className="flex gap-1.5">{children}</div>;
}

function Cv2Btn({ children }: { children: React.ReactNode }) {
  return <span className="inline-block px-3 py-1 rounded text-[11px] font-medium bg-white/[0.07] text-white/50">{children}</span>;
}

function DiscordSelect({ options }: { options: string[] }) {
  return (
    <div className="flex items-center rounded-lg bg-[rgba(0,0,0,0.5)] border border-white/[0.07]">
      <div className="flex items-center flex-1 px-2 py-2.5">
        {options.map(o => (
          <span key={o} className="text-[13px] px-2.5 text-white/50">{o}</span>
        ))}
      </div>
      <div className="flex items-center px-3 border-l border-white/[0.07] self-stretch">
        <svg width="18" height="18" viewBox="0 0 24 24" className="text-white/25">
          <path d="M7 10l5 5 5-5" stroke="currentColor" strokeWidth="2" fill="none" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
      </div>
    </div>
  );
}

function MockSelect({ children, className = "" }: { children: React.ReactNode; className?: string }) {
  return (
    <div className={`flex items-center justify-between px-2.5 py-1.5 rounded bg-white/[0.04] border border-white/[0.07] text-[11px] text-white/50 ${className}`}>
      <span>{children}</span>
      <svg width="10" height="10" viewBox="0 0 24 24" className="text-white/20"><path d="M7 10l5 5 5-5" stroke="currentColor" strokeWidth="2.5" fill="none" strokeLinecap="round" /></svg>
    </div>
  );
}

function RolePill({ color, children }: { color: string; children: React.ReactNode }) {
  return <span className="inline-block px-1.5 py-0.5 rounded text-[10px] font-medium" style={{ backgroundColor: color + "25", color }}>{children}</span>;
}

function TagEntry({ icon, fill, name, reason, by, time }: {
  icon: string; fill: string; name: string; reason: string; by: string; time: string;
}) {
  return (
    <div className="text-[12px]">
      <div className="font-semibold text-white/70 flex items-center gap-1">
        <Mdi d={icon} fill={fill} /> {name}
      </div>
      <div className="ml-4 border-l-2 border-white/[0.08] pl-2.5 mt-0.5 space-y-0.5">
        <div className="text-white/40">{reason}</div>
        <div className="text-[10px] text-white/25 font-semibold">— Added by <Code>@{by}</Code> {time}</div>
      </div>
    </div>
  );
}

function SlashLabel({ children }: { children: React.ReactNode }) {
  return <div className="text-[14px] text-white/30 pl-1 mb-2"><span className="text-[#7289da]">{children}</span></div>;
}



function Code({ children }: { children: React.ReactNode }) {
  return <code className="bg-white/[0.06] px-1 rounded text-[11px]">{children}</code>;
}

const MDI = {
  tagTextOutline: "M21.4 11.6L12.4 2.6C12 2.2 11.5 2 11 2H4C2.9 2 2 2.9 2 4V11C2 11.5 2.2 12 2.6 12.4L11.6 21.4C12 21.8 12.5 22 13 22C13.5 22 14 21.8 14.4 21.4L21.4 14.4C21.8 14 22 13.5 22 13C22 12.5 21.8 12 21.4 11.6M13 20L4 11V4H11L20 13M6.5 5C7.3 5 8 5.7 8 6.5S7.3 8 6.5 8 5 7.3 5 6.5 5.7 5 6.5 5M10.1 8.9L11.5 7.5L17 13L15.6 14.4L10.1 8.9M7.6 11.4L9 10L13 14L11.6 15.4L7.6 11.4Z",
  octagramOutline: "M2.2,16.06L3.88,12L2.2,7.94L6.26,6.26L7.94,2.2L12,3.88L16.06,2.2L17.74,6.26L21.8,7.94L20.12,12L21.8,16.06L17.74,17.74L16.06,21.8L12,20.12L7.94,21.8L6.26,17.74L2.2,16.06M4.81,9L6.05,12L4.81,15L7.79,16.21L9,19.19L12,17.95L15,19.19L16.21,16.21L19.19,15L17.95,12L19.19,9L16.21,7.79L15,4.81L12,6.05L9,4.81L7.79,7.79L4.81,9M11,15H13V17H11V15M11,7H13V13H11V7",
};

function Mdi({ d, fill, size = 13 }: { d: string; fill: string; size?: number }) {
  return <svg viewBox="0 0 24 24" width={size} height={size} className="shrink-0 inline-block align-middle"><path d={d} fill={fill} /></svg>;
}

function Section({ children, wide }: { children: React.ReactNode; wide?: boolean }) {
  return <section className={`w-full mx-auto px-6 py-16 ${wide ? "max-w-4xl" : "max-w-3xl"}`}>{children}</section>;
}

function SectionLabel({ children }: { children: React.ReactNode }) {
  return <div className="text-xs text-white/25 uppercase tracking-widest mb-3">{children}</div>;
}

function Nav() {
  return (
    <nav className="fixed top-0 w-full z-50 bg-[rgba(10,10,10,0.8)] backdrop-blur-xl border-b border-white/[0.06]">
      <div className="max-w-6xl mx-auto px-6 h-14 flex items-center justify-between">
        <Link href="/" className="flex items-center gap-2.5">
          <img src="/logo.png" alt="Coral" width={22} height={22} className="pixelated" />
          <span className="text-base font-bold tracking-tight">Coral</span>
        </Link>
        <div className="flex items-center gap-6 text-sm">
          <Link href="/docs" className="text-white/40 hover:text-white/70 transition-colors">API</Link>
          <Link href="/starfish" className="text-white/40 hover:text-white/70 transition-colors">Starfish</Link>
          <a href="https://discord.gg/jgKEVUJj3H" target="_blank" rel="noreferrer" className="text-white/40 hover:text-white/70 transition-colors">Discord</a>
        </div>
      </div>
    </nav>
  );
}

function Footer() {
  return (
    <footer className="border-t border-white/[0.06] py-6 px-6 mt-8">
      <div className="max-w-6xl mx-auto flex flex-col sm:flex-row items-center justify-between gap-3 text-xs text-white/20">
        <div className="flex items-center gap-4">
          <span>&copy; {new Date().getFullYear()} Coral</span>
          <Link href="/privacy" className="hover:text-white/40 transition-colors">Privacy</Link>
        </div>
        <p>Not affiliated with Hypixel Inc., Mojang AB, or Microsoft.</p>
      </div>
    </footer>
  );
}
