import { SIGN_IN } from "./account";

export type CheckoutResult = "owned" | "failed";

const BUY_PARAM = "buy";
/** The Starfish terms the buyer agrees to; the server refuses any other version. */
export const TERMS_VERSION = "2026-11-24";

/** Sign-in link that brings the visitor back here and straight on to checkout. */
export function signInToBuy() {
  const back = new URL(location.href);
  back.searchParams.set(BUY_PARAM, "");
  return `${SIGN_IN}?return=${encodeURIComponent(back.href)}`;
}

/** True once, on the visit that returns from signing in to buy. */
export function takeBuyIntent() {
  const url = new URL(location.href);
  if (!url.searchParams.has(BUY_PARAM)) return false;
  url.searchParams.delete(BUY_PARAM);
  history.replaceState(null, "", url);
  return true;
}

/** Sends the visitor to PayNow's checkout; resolves only when that could not happen. */
export async function openCheckout(): Promise<CheckoutResult> {
  const response = await fetch("/me/starfish/checkout", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ terms_version: TERMS_VERSION }),
  }).catch(() => undefined);
  if (response?.status === 409) return "owned";
  const checkout = response?.ok ? await response.json() : undefined;
  if (!checkout?.url) return "failed";
  location.assign(checkout.url);
  return new Promise(() => {});
}

export type LicenseCheck = "licensed" | "unlicensed" | "signed-out" | "unknown";

/** Reads the license fresh; "unknown" means the check itself failed and is worth retrying. */
export async function checkLicense(): Promise<LicenseCheck> {
  const response = await fetch("/auth/me", { cache: "no-store" }).catch(
    () => undefined,
  );
  if (response?.status === 401) return "signed-out";
  if (!response?.ok) return "unknown";
  return (await response.json()).starfish_licensed ? "licensed" : "unlicensed";
}
