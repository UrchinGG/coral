import type { Metadata } from "next";
import { Section, Signature } from "@/components/starfish/DocsKit";

export const metadata: Metadata = {
  title: "World - Starfish Docs",
  description: "World state API reference for Starfish plugins.",
};

export default function WorldPage() {
  return (
    <div>
      <h1 className="text-3xl font-bold tracking-tight mb-2">World</h1>
      <p className="text-sm text-white/40 mb-10">Read block, chunk, and dimension state.</p>

      <Section title="Blocks and chunks">
        <Signature>{"starfish.world.block(position) -> {id, properties} | nil"}</Signature>
        <Signature>{"starfish.world.isChunkLoaded(chunkX, chunkZ) -> boolean"}</Signature>
        <Signature>{"starfish.world.loadedChunks() -> {x, z}[]"}</Signature>
        <Signature>{"starfish.world.chunkCount() -> number"}</Signature>
      </Section>

      <Section title="Dimension">
        <Signature>{"starfish.world.biome(x, z) -> string | nil"}</Signature>
        <Signature>{"starfish.world.dimension() -> string | nil"}</Signature>
        <Signature>{"starfish.world.minY() -> number"}</Signature>
        <Signature>{"starfish.world.height() -> number"}</Signature>
      </Section>
    </div>
  );
}
