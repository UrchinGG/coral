import type { Camera, Viewer } from "../viewer/pkg/replay_viewer.js";
import { CAMERAS } from "./cameras";
import { bindCanvas } from "./canvas";
import { $, afterPaint, clock, h } from "./dom";
import { download, fileSource, startViewer, urlSource } from "./engine";
import { EVENT_LABELS, feedText, involved, type GameEvent } from "./events";
import { bindHud } from "./hud";
import { bindKeys } from "./keys";
import { bindLibrary } from "./library";
import { keepSource, momentLink, requested } from "./link";
import {
  readMatch,
  stateOf,
  successorOf,
  type Match,
  type Player,
  type PlayerState,
} from "./match";
import { nickSkins } from "./nicks";
import {
  bindTooltips,
  hideError,
  loading,
  showError,
  showGpuNotice,
  toast,
} from "./overlays";
import { bindPanel } from "./panel";
import { bindPill } from "./pill";
import { bindPlayback } from "./playback";
import { showSkins } from "./skins";
import { bindSound } from "./sound";
import { showLoadingTitle, showSummary } from "./summary";
import { bindTimeline } from "./timeline";

interface Session {
  match: Match;
  everyFrame: ((time: number) => void)[];
  everyTick: ((time: number, presence: Uint8Array, watched?: number) => void)[];
}

const TICK_MS = 250;
const DOWNLOADED = 0.5;
const DECODED = 0.6;
const LEAD_MS = 3000;
const STEP_MARGIN_MS = 1500;

const cameras = matchMedia("(any-pointer: fine)").matches
  ? CAMERAS
  : CAMERAS.filter(({ id }) => id !== "fly");
const requestedDownload = requested.src ? download(requested.src) : undefined;

const run = async (viewer: Viewer) => {
  const stage = $("#stage");
  const help = $<HTMLDialogElement>("#help");
  const copy = $("#copy-moment");
  let session: Session | undefined;
  let opening = 0;
  let ticked = 0;

  const now = () => viewer.time;
  const seek = (time: number) => viewer.seek(time);
  const seekBefore = (time: number) => seek(time - LEAD_MS);
  const seekBy = (ms: number) => seek(now() + ms);
  const follow = (index: number) => viewer.follow(index);
  const setCamera = (camera: Camera) => viewer.setCamera(camera);
  const watched = () =>
    viewer.camera === "follow" || viewer.camera === "pov"
      ? viewer.followed
      : undefined;

  const cycleCamera = () => {
    const index = cameras.findIndex(({ id }) => id === viewer.camera);
    const next = cameras[(index + 1) % cameras.length];
    setCamera(next.id);
    toast(`Camera: ${next.name}`, 1400);
  };

  const jump = (event: GameEvent) => {
    seekBefore(event.t);
    const player = involved(event);
    if (player) follow(player.index);
    const text = h("span", {}, ...feedText(event)).textContent;
    toast(`${EVENT_LABELS[event.kind]}: ${text}`);
  };

  const stepPlayer = (direction: 1 | -1) => {
    if (!session) return;
    const { match } = session;
    const presence = viewer.presence();
    const order = direction > 0 ? match.roster : match.roster.toReversed();
    const from = order.findIndex(({ index }) => index === viewer.followed);
    const others = [
      ...order.slice(from + 1),
      ...order.slice(0, Math.max(from, 0)),
    ];
    const next = others.find(
      (player) => stateOf(match, player, presence, now()) === "alive",
    );
    if (next) follow(next.index);
    else toast("Nobody else is in the world right now.");
  };

  const stepEvent = (direction: 1 | -1) => {
    const at = now() + LEAD_MS;
    const events = panel.shownEvents();
    const next =
      direction > 0
        ? events.find(({ t }) => t > at + STEP_MARGIN_MS)
        : events.findLast(({ t }) => t < at - STEP_MARGIN_MS);
    if (!next) {
      toast(direction > 0 ? "No later events." : "No earlier events.");
      return;
    }
    panel.select("events");
    jump(next);
  };

  const stepSpeed = (direction: 1 | -1) =>
    toast(`Speed ${playback.stepSpeed(direction)}`, 1200);

  const watchAfterFinal = (match: Match) => {
    let last: { player?: Player; state?: PlayerState } = {};
    return (time: number, presence: Uint8Array, watched?: number) => {
      const player = watched === undefined ? undefined : match.players[watched];
      const state = player && stateOf(match, player, presence, time);
      const finalDeath =
        player &&
        state === "out" &&
        last.player === player &&
        last.state !== "out";
      if (finalDeath) {
        const next = successorOf(match, player, presence, time);
        if (next) follow(next.index);
      }
      last = { player, state };
    };
  };

  const openChat = () => {
    if (!session) return;
    document.exitPointerLock();
    viewer.input(new Event("blur"));
    stage.toggleAttribute("data-chat", true);
    hud.open();
  };

  const copyMoment = async (src: string) => {
    try {
      await navigator.clipboard.writeText(
        momentLink(src, now(), viewer.camera, watched()),
      );
      toast(`Link to ${clock(now())} copied`);
    } catch {
      toast("The browser did not allow copying the link.");
    }
  };

  const buildMap = () =>
    new Promise<void>((resolve) => {
      const total = viewer.pendingSections;
      const check = () => {
        const left = viewer.pendingSections;
        loading.progress(
          DECODED + (1 - DECODED) * (total ? 1 - left / total : 1),
        );
        if (left) requestAnimationFrame(check);
        else resolve();
      };
      check();
    });

  const open = async (
    input: File | string,
    downloading?: ReturnType<typeof download>,
  ) => {
    const current = ++opening;
    const restoreTitle = showLoadingTitle();
    hideError();
    loading.progress(null);
    try {
      const { name, src, read } =
        typeof input === "string"
          ? urlSource(input, downloading)
          : fileSource(input);
      const bytes = await read((fraction) =>
        loading.progress(fraction === null ? null : fraction * DOWNLOADED),
      );
      if (current !== opening) return;
      loading.progress(DOWNLOADED);
      await afterPaint();
      const match = readMatch(viewer.load(bytes), await nickSkins);
      seek(match.info.startMs);
      const [first] = match.roster;
      if (first) viewer.setFollowed(first.index);
      session = {
        match,
        everyFrame: [
          showTimeline(match),
          hud.show(match),
          sound.show(viewer.soundNames),
        ],
        everyTick: [panel.show(match), showPill(match), watchAfterFinal(match)],
      };
      showSkins(viewer, match.info.skins);
      showSummary(match, name, bytes);
      copy.hidden = !src;
      copy.onclick = src ? () => copyMoment(src) : null;
      keepSource(src);
      stage.toggleAttribute("data-empty", false);
      library.setOpen(false);
      loading.progress(DECODED);
      await buildMap();
      return match;
    } catch (error) {
      restoreTitle();
      showError("This replay could not be opened.", error);
      library.setOpen(true);
    } finally {
      if (current === opening) loading.done();
    }
  };

  const playback = bindPlayback(viewer, seekBy, setCamera);
  const sound = bindSound(viewer);
  const showTimeline = bindTimeline(seek);
  const hud = bindHud(() => stage.toggleAttribute("data-chat", false));
  const panel = bindPanel({ seekBefore, jump, follow, step: stepEvent });
  const showPill = bindPill(stepPlayer, {
    on: () => viewer.autoCamera,
    toggle: () => (viewer.autoCamera = !viewer.autoCamera),
  });
  bindCanvas(viewer, follow, hud.isOpen);
  bindTooltips();
  if (viewer.adapter.software) showGpuNotice();
  $("#help-toggle").addEventListener("click", () => help.showModal());

  bindKeys(viewer, {
    Space: () => {
      if (viewer.camera !== "fly") viewer.toggle();
    },
    ArrowLeft: (event) => seekBy(event.shiftKey ? -10000 : -1000),
    ArrowRight: (event) => seekBy(event.shiftKey ? 10000 : 1000),
    KeyC: cycleCamera,
    Minus: () => stepSpeed(-1),
    Equal: () => stepSpeed(1),
    NumpadSubtract: () => stepSpeed(-1),
    NumpadAdd: () => stepSpeed(1),
    Comma: () => stepPlayer(-1),
    Period: () => stepPlayer(1),
    KeyT: openChat,
    KeyP: () => viewer.toggle(),
    "?": () => help.showModal(),
    Escape: () => (hud.isOpen() ? hud.close() : library.setOpen(false)),
  });

  const frame = (time: number) => {
    viewer.frame(time);
    playback.update();
    if (session) {
      for (const update of session.everyFrame) update(viewer.time);
      if (time - ticked > TICK_MS) {
        ticked = time;
        const presence = viewer.presence();
        for (const update of session.everyTick) {
          update(viewer.time, presence, watched());
        }
      }
    }
    requestAnimationFrame(frame);
  };
  requestAnimationFrame(frame);

  const { src, time, camera, player, chat, autoplay } = requested;
  const match = src ? await open(src, requestedDownload) : undefined;
  if (!match) {
    loading.done();
    library.setOpen(true);
    return open;
  }
  if (time !== undefined) seek(time);
  if (
    player !== undefined &&
    match.players[player] &&
    (camera === "follow" || camera === "pov")
  ) {
    follow(player);
    if (camera === "pov") setCamera("pov");
  } else if (camera && cameras.some(({ id }) => id === camera)) {
    setCamera(camera);
  }
  if (chat) openChat();
  if (autoplay) viewer.toggle();
  return open;
};

const started = startViewer($<HTMLCanvasElement>("#view")).then(run);
const library = bindLibrary((input) => started.then((open) => open(input)));

if (requested.src) loading.progress(null);
else library.setOpen(true);

started.catch((error) => {
  loading.done();
  showError("The replay viewer could not start.", error);
});
