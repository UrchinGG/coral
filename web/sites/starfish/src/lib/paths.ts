export const BASE = import.meta.env.BASE_URL.replace(/\/$/, "");

export const page = (path: string) => `${BASE}${path}`;
