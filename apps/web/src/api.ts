import type { RuntimeStatus, SyncReport } from "@scrobble-bridge/ui";

export interface PairingRequest {
  code: string;
  expires_at: string;
}

export interface PairedDevice {
  id: string;
  label: string;
  created_at: string;
}

export class ApiClient {
  constructor(private readonly token: string) {}

  status(): Promise<RuntimeStatus> {
    return this.request("/api/v1/status");
  }

  sync(): Promise<SyncReport> {
    return this.request("/api/v1/sync", { method: "POST" });
  }

  pause(): Promise<void> {
    return this.request("/api/v1/pause", { method: "POST" });
  }

  resume(): Promise<void> {
    return this.request("/api/v1/resume", { method: "POST" });
  }

  saveYouTubeMusic(payload: {
    account_id: string;
    auth_user: number;
    cookie_header: string;
  }): Promise<void> {
    return this.request("/api/v1/credentials/ytmusic", {
      method: "PUT",
      body: JSON.stringify(payload),
    });
  }

  saveLastFmApplication(payload: {
    api_key: string;
    shared_secret: string;
  }): Promise<void> {
    return this.request("/api/v1/lastfm/application", {
      method: "PUT",
      body: JSON.stringify(payload),
    });
  }

  startLastFm(): Promise<{ authorization_url: string }> {
    return this.request("/api/v1/lastfm/auth/start", { method: "POST" });
  }

  finishLastFm(): Promise<void> {
    return this.request("/api/v1/lastfm/auth/finish", { method: "POST" });
  }

  startPairing(label: string): Promise<PairingRequest> {
    return this.request("/api/v1/pairing/start", {
      method: "POST",
      body: JSON.stringify({ label }),
    });
  }

  devices(): Promise<PairedDevice[]> {
    return this.request("/api/v1/devices");
  }

  revokeDevice(id: string): Promise<void> {
    return this.request(`/api/v1/devices/${encodeURIComponent(id)}`, {
      method: "DELETE",
    });
  }

  private async request<T>(path: string, init: RequestInit = {}): Promise<T> {
    const response = await fetch(path, {
      ...init,
      headers: {
        Authorization: `Bearer ${this.token}`,
        ...(init.body ? { "Content-Type": "application/json" } : {}),
        ...init.headers,
      },
    });
    const payload = await response.json().catch(() => ({}));
    if (!response.ok) {
      throw new Error(
        payload.message ?? `Request failed with HTTP ${response.status}`,
      );
    }
    return payload as T;
  }
}
