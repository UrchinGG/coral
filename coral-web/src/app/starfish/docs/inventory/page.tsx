import type { Metadata } from "next";
import { Section, P, Mono, Signature, PropTable } from "@/components/starfish/DocsKit";

export const metadata: Metadata = {
  title: "Inventory - Starfish Docs",
  description: "Inventory API reference for Starfish plugins.",
};

export default function InventoryPage() {
  return (
    <div>
      <h1 className="text-3xl font-bold tracking-tight mb-2">Inventory</h1>
      <p className="text-sm text-white/40 mb-10">Read the local player's inventory and build item tables.</p>

      <Section title="Reading">
        <Signature>{"starfish.inventory.slot(slot) -> Item | nil"}</Signature>
        <Signature>{"starfish.inventory.heldItem() -> Item | nil"}</Signature>
        <Signature>{"starfish.inventory.heldSlot() -> number"}</Signature>
        <Signature>{"starfish.inventory.armor() -> table"}</Signature>
        <P><Mono>{"{helmet?, chestplate?, leggings?, boots?}"}</Mono> — keys present only when occupied.</P>
        <Signature>{"starfish.inventory.windowId() -> number"}</Signature>
        <P>0 when no window is open.</P>
        <Signature>{"starfish.inventory.isWindowOpen() -> boolean"}</Signature>
      </Section>

      <Section title="Building">
        <Signature>{"starfish.inventory.item(opts) -> Item"}</Signature>
        <P><Mono>opts</Mono>: <Mono>id</Mono>, <Mono>count?</Mono>, <Mono>name?</Mono>, <Mono>lore?</Mono>, <Mono>enchantments?</Mono>. Errors if <Mono>id</Mono>/enchantment identifiers don't resolve.</P>
      </Section>

      <Section title="Item fields">
        <PropTable rows={[
          ["id", "string", ""],
          ["count", "number", ""],
          ["name", "string | nil", ""],
          ["lore", "string[]", ""],
          ["durability", "{damage, max} | nil", ""],
          ["enchantments", "{id, level}[]", ""],
        ]} />
      </Section>

      <Section title="WindowType">
        <P><Mono>starfish.inventory.WindowType</Mono> constants: <Mono>CHEST</Mono>, <Mono>CRAFTING_TABLE</Mono>, <Mono>FURNACE</Mono>, <Mono>DISPENSER</Mono>, <Mono>ENCHANTING_TABLE</Mono>, <Mono>BREWING_STAND</Mono>, <Mono>VILLAGER</Mono>, <Mono>BEACON</Mono>, <Mono>ANVIL</Mono>, <Mono>HOPPER</Mono>, <Mono>DROPPER</Mono>, <Mono>HORSE</Mono>.</P>
      </Section>
    </div>
  );
}
