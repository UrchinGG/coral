import { afterEach, beforeEach, describe, expect, it, vi, type Mock } from "vitest";

import type { Labels } from "./types/refs";

vi.mock("./http", async (importOriginal) => ({ ...(await importOriginal<typeof import("./http")>()), postJson: vi.fn() }));

type LabelsModule = typeof import("./labels");
type LabelBody = { discord: string[]; minecraft: string[] };

const BATCH_DELAY_MS = 10;
const BATCH_LIMIT = 200;
const PENDING_RETRY_MS = 4_000;
const FAILURE_COOLDOWN_MS = 60_000;
const PLAYER = "94589c87999b4a7ab3add2ae50711e64";
const ACCOUNT = "1218840276405911602";

let labels: LabelsModule;
let postJson: Mock<(path: string, body: LabelBody) => Promise<Labels>>;

function response(partial: Partial<Labels> = {}): Labels {
  return { discord: {}, minecraft: {}, pending: { discord: [], minecraft: [] }, ...partial };
}

function bodies(): LabelBody[] {
  return postJson.mock.calls.map(([, body]) => body);
}

beforeEach(async () => {
  vi.stubGlobal("window", globalThis);
  vi.useFakeTimers();
  vi.resetModules();
  postJson = vi.mocked((await import("./http")).postJson) as unknown as typeof postJson;
  postJson.mockResolvedValue(response());
  labels = await import("./labels");
});

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe("label store", () => {
  it("batches the links rendered within 10 ms into one request per batch", async () => {
    labels.requestLabel({ space: "minecraft", id: PLAYER });
    labels.requestLabel({ space: "discord", id: ACCOUNT });
    labels.requestLabel({ space: "minecraft", id: PLAYER });
    await vi.advanceTimersByTimeAsync(BATCH_DELAY_MS - 1);
    expect(postJson).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(1);
    expect(postJson).toHaveBeenCalledTimes(1);
    expect(postJson.mock.calls[0][0]).toBe("/labels");
    expect(bodies()).toEqual([{ discord: [ACCOUNT], minecraft: [PLAYER] }]);
  });

  it("caps a batch at 200 ids and sends the rest right after", async () => {
    const ids = Array.from({ length: BATCH_LIMIT + 5 }, (_, index) => index.toString(16).padStart(32, "0"));
    for (const id of ids) labels.requestLabel({ space: "minecraft", id });
    await vi.advanceTimersByTimeAsync(BATCH_DELAY_MS * 2);
    expect(bodies().map((body) => body.minecraft.length)).toEqual([BATCH_LIMIT, 5]);
  });

  it("stores names, remembers ids without a name, and does not ask again while fresh", async () => {
    postJson.mockResolvedValue(response({ minecraft: { [PLAYER]: "Kestrel_" } }));
    labels.requestLabel({ space: "minecraft", id: PLAYER });
    labels.requestLabel({ space: "discord", id: ACCOUNT });
    await vi.advanceTimersByTimeAsync(BATCH_DELAY_MS);

    expect(labels.labelOf({ space: "minecraft", id: PLAYER })).toBe("Kestrel_");
    expect(labels.labelOf({ space: "discord", id: ACCOUNT })).toBeNull();
    labels.requestLabel({ space: "minecraft", id: PLAYER });
    labels.requestLabel({ space: "discord", id: ACCOUNT });
    await vi.advanceTimersByTimeAsync(BATCH_DELAY_MS);
    expect(postJson).toHaveBeenCalledTimes(1);
  });

  it("asks again for pending ids with a growing delay, three times at most", async () => {
    postJson.mockResolvedValue(response({ pending: { discord: [ACCOUNT], minecraft: [] } }));
    labels.requestLabel({ space: "discord", id: ACCOUNT });
    await vi.advanceTimersByTimeAsync(BATCH_DELAY_MS);
    for (let attempt = 1; attempt <= 4; attempt += 1) await vi.advanceTimersByTimeAsync(PENDING_RETRY_MS * attempt + BATCH_DELAY_MS);
    expect(bodies()).toEqual(Array.from({ length: 4 }, () => ({ discord: [ACCOUNT], minecraft: [] })));
  });

  it("cools a failed batch's ids down before asking again", async () => {
    postJson.mockRejectedValueOnce(new Error("labels unavailable"));
    labels.requestLabel({ space: "minecraft", id: PLAYER });
    await vi.advanceTimersByTimeAsync(BATCH_DELAY_MS);
    expect(labels.labelOf({ space: "minecraft", id: PLAYER })).toBeNull();

    labels.requestLabel({ space: "minecraft", id: PLAYER });
    await vi.advanceTimersByTimeAsync(FAILURE_COOLDOWN_MS - BATCH_DELAY_MS - 1);
    expect(postJson).toHaveBeenCalledTimes(1);

    await vi.advanceTimersByTimeAsync(BATCH_DELAY_MS + 1);
    labels.requestLabel({ space: "minecraft", id: PLAYER });
    await vi.advanceTimersByTimeAsync(BATCH_DELAY_MS);
    expect(postJson).toHaveBeenCalledTimes(2);
  });
});
