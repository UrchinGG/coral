import type { Metadata } from "next";
import { Section, P, Mono, Signature } from "@/components/starfish/DocsKit";

export const metadata: Metadata = {
  title: "Client - Starfish Docs",
  description: "Client-side world and entity control API reference for Starfish plugins.",
};

export default function ClientPage() {
  return (
    <div>
      <h1 className="text-3xl font-bold tracking-tight mb-2">Client</h1>
      <p className="text-sm text-white/40 mb-10">Client-side-only state changes — nothing here sends a packet to the server. All functions return <Mono>nil</Mono> unless noted.</p>

      <Section title="starfish.client.player">
        <Signature>{"setHealth(opts) -> nil"}</Signature>
        <P><Mono>{"{health, food, saturation}"}</Mono></P>
        <Signature>{"setPosition(opts) -> nil"}</Signature>
        <P><Mono>{"{x, y, z, yaw?, pitch?}"}</Mono> — yaw/pitch default 0.</P>
        <Signature>{"setExperience(opts) -> nil"}</Signature>
        <P><Mono>{"{bar, level, total?}"}</Mono> — total defaults 0.</P>
        <Signature>{"setAbilities(opts) -> nil"}</Signature>
        <P><Mono>{"{invulnerable?, flying?, allowFlying?, creative?, flySpeed?, walkSpeed?}"}</Mono> — flySpeed/walkSpeed default 0.05/0.1.</P>
        <Signature>{"setSpawn(position) -> nil"}</Signature>
        <Signature>{"setHeldItemSlot(slot) -> nil"}</Signature>
      </Section>

      <Section title="starfish.client.world">
        <Signature>{"setBlock(position, identifier) -> nil"}</Signature>
        <P><Mono>identifier</Mono> e.g. <Mono>{'"minecraft:stone"'}</Mono>; errors on unknown identifier.</P>
        <Signature>{"setBlocks(list) -> nil"}</Signature>
        <P>Each entry <Mono>{"{x, y, z, id}"}</Mono>.</P>
        <Signature>{"spawnParticle(particleId, position, opts?) -> nil"}</Signature>
        <P><Mono>opts</Mono>: <Mono>longDistance?, offsetX?, offsetY?, offsetZ?, speed?, count?</Mono> (count default 1).</P>
        <Signature>{"playSound(name, opts?) -> nil"}</Signature>
        <P><Mono>opts</Mono>: <Mono>position?</Mono> (default local player), <Mono>volume?</Mono> (default 1.0), <Mono>pitch?</Mono> (default 1.0).</P>
        <Signature>{"setTime(worldAge, timeOfDay) -> nil"}</Signature>
        <Signature>{"explosion(position, opts?) -> nil"}</Signature>
        <P><Mono>opts</Mono>: <Mono>records?</Mono> (array of <Mono>{"{x,y,z}"}</Mono>), <Mono>motion?</Mono> (default 0,0,0), <Mono>radius?</Mono> (default 1.0).</P>
        <Signature>{"worldEvent(effectId, position, opts?) -> nil"}</Signature>
        <P><Mono>effectId</Mono> is a numeric id or a named event (e.g. <Mono>{'"smoke"'}</Mono>); errors on unknown name. <Mono>opts</Mono>: <Mono>data?</Mono> (default 0), <Mono>global?</Mono> (default false).</P>
        <Signature>{"gameState(reason, value) -> nil"}</Signature>
        <P><Mono>reason</Mono> is a named reason (e.g. <Mono>{'"changeGamemode"'}</Mono>); errors on unknown name.</P>
        <P><Mono>starfish.client.world.Particle</Mono> is a table mapping particle name to id.</P>
      </Section>

      <Section title="starfish.client.window">
        <Signature>{"openWindow(windowId, windowType, title, slotCount) -> nil"}</Signature>
        <Signature>{"closeWindow(windowId) -> nil"}</Signature>
        <Signature>{"createChest(title, size?) -> windowId"}</Signature>
        <P>Size defaults 27.</P>
        <Signature>{"createHopper(title) -> windowId"}</Signature>
        <Signature>{"createDispenser(title) -> windowId"}</Signature>
        <Signature>{"setSlot(windowId, slot, item) -> nil"}</Signature>
        <Signature>{"setWindowItems(windowId, items) -> nil"}</Signature>
        <Signature>{"fillWindow(windowId, item) -> nil"}</Signature>
        <Signature>{"clearWindow(windowId) -> nil"}</Signature>
        <Signature>{"clickSlot(windowId, slot, button?, mode?, item?) -> nil"}</Signature>
        <P>button/mode default 0.</P>
        <Signature>{"sendTransaction(windowId, action, accepted) -> nil"}</Signature>
        <Signature>{"sendCraftProgress(windowId, property, value) -> nil"}</Signature>
        <Signature>{"openSignEditor(position) -> nil"}</Signature>
      </Section>

      <Section title="starfish.client.entity">
        <Signature>{"spawnPlayer(opts) -> nil"}</Signature>
        <P><Mono>{"{position, entityId, uuid, yaw?, pitch?, metadata?}"}</Mono></P>
        <Signature>{"spawnObject(opts) -> nil"}</Signature>
        <P><Mono>{"{position, entityId, entityType, pitch?, yaw?, data?}"}</Mono> (pitch/yaw/data default 0).</P>
        <Signature>{"spawnMob(opts) -> nil"}</Signature>
        <P><Mono>{"{position, entityId, entityType, yaw?, pitch?, headPitch?, metadata?}"}</Mono></P>
        <Signature>{"destroy(entityIds) -> nil"}</Signature>
        <Signature>{"teleport(entityId, position, opts?) -> nil"}</Signature>
        <P><Mono>opts</Mono>: <Mono>yaw?, pitch?, onGround?</Mono> (default 0, 0, true).</P>
        <Signature>{"move(entityId, opts) -> nil"}</Signature>
        <P><Mono>{"{dx, dy, dz, onGround?}"}</Mono> — onGround defaults true.</P>
        <Signature>{"look(entityId, opts) -> nil"}</Signature>
        <P><Mono>{"{yaw, pitch, onGround?}"}</Mono></P>
        <Signature>{"moveLook(entityId, opts) -> nil"}</Signature>
        <P><Mono>{"{dx, dy, dz, yaw, pitch, onGround?}"}</Mono></P>
        <Signature>{"setVelocity(entityId, opts) -> nil"}</Signature>
        <P><Mono>{"{x, y, z}"}</Mono> — errors if <Mono>entityId</Mono> is the local player's own tracked entity.</P>
        <Signature>{"setHeadRotation(entityId, headYaw) -> nil"}</Signature>
        <Signature>{"setMetadata(entityId, entries) -> nil"}</Signature>
        <P>Each entry <Mono>{'{index, type, value}'}</Mono> — <Mono>type</Mono> is <Mono>byte | short | int | float | string | slot | position | rotation</Mono>.</P>
        <Signature>{"setEquipment(entityId, slot, item) -> nil"}</Signature>
        <P>Errors on an unrecognized slot name.</P>
        <Signature>{"animate(entityId, animation) -> nil"}</Signature>
        <Signature>{"setStatus(entityId, status) -> nil"}</Signature>
        <Signature>{"addEffect(entityId, effect, opts?) -> nil"}</Signature>
        <P><Mono>opts</Mono>: <Mono>amplifier?</Mono> (default 0), <Mono>duration?</Mono> (default 200), <Mono>hideParticles?</Mono> (default false). Errors on unknown effect, or if <Mono>entityId</Mono> is the local player.</P>
        <Signature>{"removeEffect(entityId, effect) -> nil"}</Signature>
        <P>Same restrictions as <Mono>addEffect</Mono>.</P>
        <Signature>{"attach(entityId, vehicleId, leash?) -> nil"}</Signature>
        <P>leash defaults false; errors if <Mono>entityId</Mono> is the local player.</P>
        <Signature>{"collectItem(collectedId, collectorId) -> nil"}</Signature>
      </Section>
    </div>
  );
}
