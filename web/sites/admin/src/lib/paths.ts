export const BASE = import.meta.env.BASE_URL.replace(/\/$/, "");
export const API = `${BASE}/api`;

export const page = (path: string) => `${BASE}${path}`;

export const pagePath = (pathname: string) =>
  pathname.startsWith(BASE) ? pathname.slice(BASE.length) || "/" : pathname;
