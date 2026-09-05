import type { Metadata } from "next";
import { Section, P, Mono, Signature, PropTable } from "@/components/starfish/DocsKit";

export const metadata: Metadata = {
  title: "Entities - Starfish Docs",
  description: "Entity tracking API reference for Starfish plugins.",
};

export default function EntitiesPage() {
  return (
    <div>
      <h1 className="text-3xl font-bold tracking-tight mb-2">Entities</h1>
      <p className="text-sm text-white/40 mb-10">Read tracked entity state. Positions can be passed as <Mono>{"{x, y, z}"}</Mono>, an entity, or the local player — anywhere a function below takes a position.</p>

      <Section title="Lookup">
        <Signature>{"starfish.entities.all() -> Entity[]"}</Signature>
        <Signature>{"starfish.entities.byId(id) -> Entity | nil"}</Signature>
        <Signature>{"starfish.entities.byUuid(uuid) -> Entity | nil"}</Signature>
        <Signature>{"starfish.entities.players() -> Entity[]"}</Signature>
        <P>Tracked entities whose source is a player.</P>
        <Signature>{"starfish.entities.near(position, radius) -> Entity[]"}</Signature>
        <P>Entities within <Mono>radius</Mono> (Euclidean) of a position.</P>
      </Section>

      <Section title="Events">
        <Signature>{"starfish.entities.onStateChange(handler) -> Subscription"}</Signature>
        <P>Fires when an entity's boolean state (sneaking, sprinting, etc.) changes. Payload: <Mono>{"{entity, changed, state}"}</Mono>.</P>
        <Signature>{"starfish.entities.onSpawn(handler) -> Subscription"}</Signature>
        <Signature>{"starfish.entities.onDespawn(handler) -> Subscription"}</Signature>
        <Signature>{"starfish.entities.onMove(handler) -> Subscription"}</Signature>
        <Signature>{"starfish.entities.onEquipmentChange(handler) -> Subscription"}</Signature>
      </Section>

      <Section title="Entity fields">
        <PropTable rows={[
          ["id", "number", "Entity id"],
          ["kind", "string", ""],
          ["uuid", "string | nil", ""],
          ["onGround", "boolean", ""],
          ["position", "{x, y, z}", ""],
          ["velocity", "{x, y, z}", ""],
          ["rotation", "{yaw, pitch}", ""],
          ["state", "table", "{sneaking, sprinting, usingItem, invisible, onFire, glowing, gliding, swimming}"],
          ["equipment", "table", "{hand, offHand, boots, leggings, chestplate, helmet}, each an Item or nil"],
        ]} />
      </Section>
    </div>
  );
}
