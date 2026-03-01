// Web search provider implementations

export interface SearchResult {
  title: string;
  url: string;
  snippet: string;
  content?: string; // Tavily returns extracted content
  provider: string;
}

export interface SearchProvider {
  name: string;
  available: boolean;
  search(query: string, maxResults: number, topic: string): Promise<SearchResult[]>;
}

// --- Brave Search ---

function braveProvider(): SearchProvider {
  const apiKey = process.env.BRAVE_API_KEY;
  return {
    name: "brave",
    available: !!apiKey,
    async search(query, maxResults, topic) {
      const params = new URLSearchParams({
        q: query,
        count: String(maxResults),
        ...(topic === "news" ? { freshness: "pd" } : {}),
      });
      const res = await fetch(
        `https://api.search.brave.com/res/v1/web/search?${params}`,
        { headers: { "X-Subscription-Token": apiKey!, Accept: "application/json" } }
      );
      if (!res.ok) throw new Error(`Brave ${res.status}: ${await res.text()}`);
      const data = await res.json();
      return (data.web?.results || []).slice(0, maxResults).map((r: any) => ({
        title: r.title,
        url: r.url,
        snippet: r.description || "",
        provider: "brave",
      }));
    },
  };
}

// --- Tavily ---

function tavilyProvider(): SearchProvider {
  const apiKey = process.env.TAVILY_API_KEY;
  return {
    name: "tavily",
    available: !!apiKey,
    async search(query, maxResults, topic) {
      const res = await fetch("https://api.tavily.com/search", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          api_key: apiKey,
          query,
          max_results: maxResults,
          include_answer: false,
          include_raw_content: false,
          topic: topic === "news" ? "news" : "general",
        }),
      });
      if (!res.ok) throw new Error(`Tavily ${res.status}: ${await res.text()}`);
      const data = await res.json();
      return (data.results || []).slice(0, maxResults).map((r: any) => ({
        title: r.title,
        url: r.url,
        snippet: r.content || "",
        content: r.raw_content || undefined,
        provider: "tavily",
      }));
    },
  };
}

// --- Serper (Google) ---

function serperProvider(): SearchProvider {
  const apiKey = process.env.SERPER_API_KEY;
  return {
    name: "serper",
    available: !!apiKey,
    async search(query, maxResults, topic) {
      const endpoint =
        topic === "news"
          ? "https://google.serper.dev/news"
          : "https://google.serper.dev/search";
      const res = await fetch(endpoint, {
        method: "POST",
        headers: { "X-API-KEY": apiKey!, "Content-Type": "application/json" },
        body: JSON.stringify({ q: query, num: maxResults }),
      });
      if (!res.ok) throw new Error(`Serper ${res.status}: ${await res.text()}`);
      const data = await res.json();
      const results = topic === "news" ? data.news || [] : data.organic || [];
      return results.slice(0, maxResults).map((r: any) => ({
        title: r.title,
        url: r.link,
        snippet: r.snippet || r.description || "",
        provider: "serper",
      }));
    },
  };
}

// --- Registry ---

export function getProviders(): SearchProvider[] {
  return [braveProvider(), tavilyProvider(), serperProvider()];
}

export function getAvailableProviders(): SearchProvider[] {
  return getProviders().filter((p) => p.available);
}

export function getProvider(name: string): SearchProvider | undefined {
  return getProviders().find((p) => p.name === name && p.available);
}
