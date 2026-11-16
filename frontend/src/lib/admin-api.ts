const API_BASE = "/api/admin";

import { tr } from "./i18n";

export interface Vendor {
  id: number;
  name: string;
  upstream: string;
  auth_header: string;
  auth_format: string;
  models: string;
}

export interface TokenInfo {
  name: string | null;
  prefix: string;
  created_at: string;
  revoked: boolean;
}

export interface CreatedToken {
  token: string;
  prefix: string;
  name: string | null;
}

export interface DailyPoint {
  day: string;
  series: string;
  requests: number;
}

export interface DailyStats {
  by_provider: DailyPoint[];
  by_token: DailyPoint[];
}

export interface ProviderTestResult {
  ok: boolean;
  models?: number;
  latency_ms?: number;
  status?: number | null;
  error?: string;
  /** Present when specific models were tested. */
  tested?: number;
  passed?: number;
  results?: {
    model: string;
    ok: boolean;
    latency_ms?: number;
    status?: number | null;
    error?: string;
  }[];
}

export interface ProviderModelsResult {
  models: string[];
  latency_ms?: number;
  error?: string;
  status?: number | null;
}

export interface ProviderStat {
  provider_name: string;
  count: number;
  first_seen: string;
  last_seen: string;
}

export interface StatsSummary {
  total_requests: number;
  by_provider: ProviderStat[];
}

export class AdminApi {
  constructor(private token: string) {}

  private headers() {
    return {
      Authorization: `Bearer ${this.token}`,
      "Content-Type": "application/json",
    };
  }

  /**
   * Turns a failed response into an Error with a human-readable message.
   * Several endpoints answer with an empty body (201/204), so the status code
   * is the only signal available.
   */
  private throwOnError(res: Response): void {
    if (res.ok) return;

    switch (res.status) {
      case 401:
        throw new Error(tr("error.unauthorized"));
      case 409:
        throw new Error(tr("error.conflict"));
      case 404:
        throw new Error(tr("error.notFound"));
      case 429:
        throw new Error(tr("error.tooManyRequests"));
      case 400:
        // The server rejects malformed vendor settings (a bad upstream URL, an
        // invalid auth header, a name it could never route to). Saying only
        // "request failed" sends the operator looking in the wrong place.
        throw new Error(tr("error.invalidInput"));
      case 503:
        throw new Error(tr("error.serviceUnavailable"));
      default:
        throw new Error(tr("error.requestFailed", { status: res.status }));
    }
  }

  async login(username: string, password: string): Promise<string> {
    const res = await fetch(`${API_BASE}/login`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ username, password }),
    });
    if (res.status === 401) throw new Error(tr("login.invalid"));
    if (!res.ok)
      throw new Error(tr("error.requestFailed", { status: res.status }));
    const json = await res.json();
    return json.token;
  }

  async logout(): Promise<void> {
    await fetch(`${API_BASE}/logout`, {
      method: "POST",
      headers: this.headers(),
    });
  }

  /**
   * Creates a platform API key. The raw token is only returned by this call —
   * afterwards only its prefix is visible. The name must be unique.
   */
  async createToken(name?: string): Promise<CreatedToken> {
    const res = await fetch(`${API_BASE}/tokens`, {
      method: "POST",
      headers: this.headers(),
      body: JSON.stringify({ name: name ?? null }),
    });
    if (res.status === 409) throw new Error(tr("tokens.nameDuplicate"));
    if (res.status === 400) throw new Error(tr("tokens.nameRequired"));
    this.throwOnError(res);
    return res.json();
  }
  async revokeToken(prefix: string): Promise<void> {
    const res = await fetch(
      `${API_BASE}/tokens/${encodeURIComponent(prefix)}`,
      {
        method: "DELETE",
        headers: this.headers(),
      },
    );
    this.throwOnError(res);
  }

  /**
   * Verifies a vendor. Pass the inline config to test a vendor that has not been
   * saved yet; omit `api_key` to use the stored one. Pass `models` to test only
   * those models — otherwise a single model-list call is used as a cheap
   * connectivity check.
   */
  async testProvider(
    name: string,
    config?: {
      upstream: string;
      auth_header: string;
      auth_format: string;
      api_key?: string;
      models?: string[];
    },
  ): Promise<ProviderTestResult> {
    const res = await fetch(
      `${API_BASE}/providers/${encodeURIComponent(name)}/test`,
      {
        method: "POST",
        headers: this.headers(),
        body: JSON.stringify(config ?? {}),
      },
    );
    this.throwOnError(res);
    return res.json();
  }

  /** Lists the models an upstream exposes. */
  async fetchProviderModels(
    name: string,
    config?: {
      upstream: string;
      auth_header: string;
      auth_format: string;
      api_key?: string;
    },
  ): Promise<ProviderModelsResult> {
    const res = await fetch(
      `${API_BASE}/providers/${encodeURIComponent(name)}/models`,
      {
        method: "POST",
        headers: this.headers(),
        body: JSON.stringify(config ?? {}),
      },
    );
    this.throwOnError(res);
    return res.json();
  }

  /**
   * Serves the values the console needs that are not stored per-vendor.
   *
   * The reserved-name list lives on the server so the browser cannot drift out
   * of step with what the router will actually refuse.
   */
  async getConfig(): Promise<{ reserved_names: string[] }> {
    const res = await fetch(`${API_BASE}/config`, { headers: this.headers() });
    this.throwOnError(res);
    return res.json();
  }

  async getProviders(): Promise<Vendor[]> {
    const res = await fetch(`${API_BASE}/providers`, {
      headers: this.headers(),
    });
    this.throwOnError(res);
    const json = await res.json();
    return json.providers || [];
  }

  async createProvider(data: {
    name: string;
    upstream: string;
    auth_header: string;
    auth_format: string;
    models?: string;
    api_key: string;
  }): Promise<void> {
    const res = await fetch(`${API_BASE}/providers`, {
      method: "POST",
      headers: this.headers(),
      body: JSON.stringify(data),
    });
    this.throwOnError(res);
  }

  async updateProvider(
    name: string,
    data: {
      upstream?: string;
      auth_header?: string;
      auth_format?: string;
      models?: string;
      api_key?: string;
    },
  ): Promise<void> {
    const res = await fetch(
      `${API_BASE}/providers/${encodeURIComponent(name)}`,
      {
        method: "PUT",
        headers: this.headers(),
        body: JSON.stringify(data),
      },
    );
    this.throwOnError(res);
  }

  async deleteProvider(name: string): Promise<void> {
    const res = await fetch(
      `${API_BASE}/providers/${encodeURIComponent(name)}`,
      {
        method: "DELETE",
        headers: this.headers(),
      },
    );
    this.throwOnError(res);
  }

  async getTokens(): Promise<TokenInfo[]> {
    const res = await fetch(`${API_BASE}/tokens`, { headers: this.headers() });
    this.throwOnError(res);
    const json = await res.json();
    return json.tokens || [];
  }

  async getStats(): Promise<StatsSummary> {
    const res = await fetch(`${API_BASE}/stats`, { headers: this.headers() });
    this.throwOnError(res);
    return res.json();
  }

  async getDailyStats(): Promise<DailyStats> {
    const res = await fetch(`${API_BASE}/stats/daily`, {
      headers: this.headers(),
    });
    this.throwOnError(res);
    return res.json();
  }
}
