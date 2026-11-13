import type { HarmonizeResponse, HealthResponse } from "../types";

const BASE_URL = "/api";

async function request<T>(endpoint: string, params?: Record<string, string>): Promise<T> {
  const url = new URL(`${BASE_URL}${endpoint}`, window.location.origin);
  if (params) {
    Object.entries(params).forEach(([k, v]) => url.searchParams.set(k, v));
  }

  const res = await fetch(url.toString());
  if (!res.ok) {
    throw new Error(`API error: ${res.status} ${res.statusText}`);
  }
  return res.json();
}

export async function getHealth(): Promise<HealthResponse> {
  return request<HealthResponse>("/health");
}

export async function getHarmonized(
  west: number,
  south: number,
  east: number,
  north: number,
  days?: number,
): Promise<HarmonizeResponse> {
  const params: Record<string, string> = {
    west: String(west),
    south: String(south),
    east: String(east),
    north: String(north),
  };
  if (days) params.days = String(days);
  return request<HarmonizeResponse>("/harmonize", params);
}
