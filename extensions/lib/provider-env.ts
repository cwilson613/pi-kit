// @secret ANTHROPIC_API_KEY "Anthropic API key for Claude models (archmagos/magos tier)"
// @secret OPENAI_API_KEY "OpenAI API key for GPT models (all tiers)"
// @secret GEMINI_API_KEY "Google Gemini API key (victory/gloriana tier)"
// @secret XAI_API_KEY "xAI API key for Grok models (victory tier)"
// @secret GROQ_API_KEY "Groq API key for fast inference (retribution tier)"
// @secret MISTRAL_API_KEY "Mistral API key for Mistral/Codestral models (retribution tier)"
// @secret OPENROUTER_API_KEY "OpenRouter API key for multi-provider routing"
// @secret AZURE_OPENAI_API_KEY "Azure OpenAI API key"

/**
 * Mapping from pi model provider names to their env var API keys.
 *
 * Mirrors the envMap in pi-ai's env-api-keys.js — must stay in sync.
 * Used by /providers to show remediation hints, and by bootstrap to
 * detect which providers are configured.
 */
export const PROVIDER_ENV_VARS: Record<string, { envVar: string; description: string; tier?: string }> = {
  anthropic: {
    envVar: "ANTHROPIC_API_KEY",
    description: "Claude models (opus, sonnet, haiku)",
    tier: "gloriana → retribution",
  },
  openai: {
    envVar: "OPENAI_API_KEY",
    description: "GPT models (5.4, 5.3, 5.1)",
    tier: "gloriana → retribution",
  },
  "github-copilot": {
    envVar: "GITHUB_TOKEN",
    description: "GitHub Copilot (Claude, GPT, Gemini, Grok via OAuth)",
    tier: "gloriana → retribution",
  },
  google: {
    envVar: "GEMINI_API_KEY",
    description: "Google Gemini models",
    tier: "gloriana → victory",
  },
  xai: {
    envVar: "XAI_API_KEY",
    description: "xAI Grok models",
    tier: "victory",
  },
  groq: {
    envVar: "GROQ_API_KEY",
    description: "Groq fast inference",
    tier: "retribution",
  },
  mistral: {
    envVar: "MISTRAL_API_KEY",
    description: "Mistral / Codestral",
    tier: "retribution",
  },
  openrouter: {
    envVar: "OPENROUTER_API_KEY",
    description: "OpenRouter multi-provider gateway",
    tier: "varies",
  },
  "azure-openai-responses": {
    envVar: "AZURE_OPENAI_API_KEY",
    description: "Azure OpenAI",
    tier: "gloriana → retribution",
  },
  "amazon-bedrock": {
    envVar: "AWS_ACCESS_KEY_ID",
    description: "AWS Bedrock (uses AWS credentials)",
    tier: "gloriana → retribution",
  },
  "google-vertex": {
    envVar: "GOOGLE_CLOUD_API_KEY",
    description: "Google Vertex AI (or ADC credentials)",
    tier: "gloriana → retribution",
  },
};

/**
 * Get the env var name for a provider, or undefined if unknown.
 */
export function getProviderEnvVar(provider: string): string | undefined {
  return PROVIDER_ENV_VARS[provider]?.envVar;
}

/**
 * Get remediation hint for an unconfigured provider.
 */
export function getProviderRemediationHint(provider: string): string | undefined {
  const entry = PROVIDER_ENV_VARS[provider];
  if (!entry) return undefined;
  if (provider === "github-copilot") {
    return "Run `/login github` or set GITHUB_TOKEN via `/secrets configure GITHUB_TOKEN`";
  }
  if (provider === "amazon-bedrock") {
    return "Run `aws sso login` or configure AWS credentials via `/secrets configure AWS_ACCESS_KEY_ID`";
  }
  if (provider === "google-vertex") {
    return "Run `gcloud auth application-default login` or set GOOGLE_CLOUD_API_KEY via `/secrets configure GOOGLE_CLOUD_API_KEY`";
  }
  return `Run \`/secrets configure ${entry.envVar}\``;
}
