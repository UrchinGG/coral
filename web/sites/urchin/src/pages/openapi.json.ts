import type { APIRoute } from "astro";
import { CORAL_API_URL } from "astro:env/server";

export const GET: APIRoute = () => fetch(`${CORAL_API_URL}/openapi.json`);
